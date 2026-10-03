-- catalog.sql — the read path, as SQL.
--
-- Four statements: the list, the get, the count `/readyz` asks for, and the
-- compatibility graph. Each carries below why it is written the way it is; this
-- header is only the one fact that applies to all of them, which is that NONE of
-- them filters on `publisher_id` or on `state`, because RLS already decided both
-- questions underneath whatever they say.

-- name: ListServices :many
--
-- NO `where publisher_id = …` AND NO `where state = 'published'`, and the absence
-- is the design rather than an omission:
--
--   * Tenant isolation is a POLICY. `00006_rls.sql` names `pantry_public` in
--     `services_public_read using (pantry.service_is_visible(services))`, and RLS
--     applies underneath every query in this file whatever it says. A hand-written
--     predicate that duplicated it would be a second implementation of the isolation
--     boundary, in a language `tests/rls.sh` cannot reach, that could be wrong in
--     the direction that leaks.
--   * `state` is the more interesting one. The public catalog is "published rows",
--     and `and state = 'published'` is the obvious thing to type. `pantry
--     .service_is_visible` does NOT say that: it admits a published row, a
--     first-party row in ANY state, and the caller's own row. Writing the clause
--     anyway would silently hide the fleet's own draft services — a wrong answer
--     rather than a missing one — and it would contradict the policy that already
--     settled the question.
--
-- So what a caller can see is decided by the ROLE the connection is using (see
-- `internal/pantrydb/db.go`), and what this statement decides is the document's
-- filters, paging and ordering.
--
-- `GET /v1/services`. Ordered by name, which is the registry's natural key and is
-- UNIQUE (00003's `services_name_key`), which is what makes the cursor a plain
-- keyset on one column with no tie to break.
--
-- THE FILTERS are the three the document declares, and each one is a column that
-- exists:
--
--   kind      services.kind    (enum `pantry.service_kind`, compared as text)
--   language  services.language
--   contract  services.core_constraint, by RANGE INTERSECTION — see below
--
-- `kind` and `language` are compared as `::text` on purpose rather than bound as
-- their own types, and the reason is a real disagreement between the two
-- vocabularies rather than taste: `openapi/v1.yaml`'s `ServiceKind` enum is
-- `[api, worker, both, cli]` while `pantry.service_kind` is a CLOSED SET of
-- `(api, cli)` — `00003` records that `worker` and `both` were both refused.
-- Casting a caller's `worker` to `pantry.service_kind` raises `invalid input
-- value for enum`, which is a 500 for a question the document says is answerable.
-- Compared as text, `?kind=worker` is what it actually means — no service has
-- that kind — and answers 200 with `data: []`. The same holds for `?language=zig`
-- and `?language=javascript`, which `00003`'s CHECK admits and the document's
-- `Language` enum does not; the generated binder refuses those with a 400 before
-- this SQL is reached, which is the correct layer for that disagreement.
--
-- THE CONTRACT FILTER IS RANGE INTERSECTION, and it is not string equality —
-- `>=0.2.0` matches a service pinned to `^0.2.0` and `^0.2.0` does not match one
-- pinned to `^0.1.0`, because before 1.0 a caret pins the minor. That is the
-- document's own example and it is the reason the filter cannot be `=`.
--
-- `services.core_constraint` stores core's four forms verbatim as TEXT and is NOT
-- parsed into columns — `00003` refuses that on purpose, because the document
-- publishes the string and a registry that stores a re-rendered version has a way
-- of disagreeing with the manifest it claims to be a copy of. So the intersection
-- is computed here, in SQL, from the string, and it is written as half-open
-- integer intervals `[lo, hi)`:
--
--     exact a.b.c   [a.b.c,        a.b.(c+1) )
--     ^a.b.c  a=0   [0.b.c,        0.(b+1).0 )   -- pre-1.0 a caret pins the MINOR
--     ^a.b.c  a>0   [a.b.c,        (a+1).0.0  )
--     ~a.b.c        [a.b.c,        a.(b+1).0  )
--     >=a.b.c       [a.b.c,        infinity   )
--
-- Triples are compared as zero-padded fixed-width strings, which makes `<` and
-- `>=` lexicographic and removes every `<` chain from the query. Two half-open
-- intervals over integer triples are non-empty exactly when `lo_a < hi_b and lo_b
-- < hi_a`, and the strictness is load-bearing: `^0.1.0` and `^0.2.0` touch at
-- `0.2.0`, which is in neither, and `<=` would call that a match.
with rows as (
  select s.name,
         s.description,
         s.language,
         s.kind::text as kind,
         s.core_constraint,
         s.base_path,
         s.manifest
    from pantry.services s
   where (sqlc.narg('kind')::text     is null or s.kind::text = sqlc.narg('kind')::text)
     and (sqlc.narg('language')::text is null or s.language    = sqlc.narg('language')::text)
     -- The cursor, resolved to a name: `where name > $after`, ordered by name.
     and (sqlc.narg('after_name')::text is null or s.name      > sqlc.narg('after_name')::text)
),
-- NORMALISED, ONCE. `regexp_replace` is what turns core's four forms into
-- something numeric — `^0.1.0` and `>=0.1.0` both reduce to `0.1.0` and the
-- operator stays in `op` — and it happens here, in one place, before anything
-- does arithmetic on it.
--
-- The first version of this query stripped the prefix in the `lo` branch and
-- forgot to in the `hi` branch, so `?contract=^0.2.0` answered 503 with
-- `invalid input syntax for type integer: "^0"`. Normalising first is what makes
-- the four cases below differ only in the `op` they are given and nowhere else.
norm as (
  select r.name,
         r.description,
         r.language,
         r.kind,
         r.core_constraint,
         r.base_path,
         r.manifest,
         case when left(r.core_constraint, 1) in ('^', '~', '>')
              then left(r.core_constraint, 1) else '' end as op,
         split_part(regexp_replace(r.core_constraint, '[^0-9.]', '', 'g'), '.', 1)::int as a,
         split_part(regexp_replace(r.core_constraint, '[^0-9.]', '', 'g'), '.', 2)::int as b,
         split_part(regexp_replace(r.core_constraint, '[^0-9.]', '', 'g'), '.', 3)::int as c
    from rows r
),
asked as (
  select case when left(nullif(sqlc.narg('contract')::text, ''), 1) in ('^', '~', '>')
              then left(nullif(sqlc.narg('contract')::text, ''), 1) else '' end as op,
         split_part(regexp_replace(nullif(sqlc.narg('contract')::text, ''), '[^0-9.]', '', 'g'), '.', 1)::int as a,
         split_part(regexp_replace(nullif(sqlc.narg('contract')::text, ''), '[^0-9.]', '', 'g'), '.', 2)::int as b,
         split_part(regexp_replace(nullif(sqlc.narg('contract')::text, ''), '[^0-9.]', '', 'g'), '.', 3)::int as c
),
-- A triple as ONE comparable string: `lpad` to a fixed width makes `<` and `>=`
-- lexicographic over the digits, which turns each interval into two strings and
-- the intersection into two comparisons.
--
-- The two CTEs below are the same `op` ladder over the same three integers, and
-- they are written out twice because a shared SQL function would be a migration
-- and a migration in the read path is a schema decision this packet does not make
-- on its own. Both are driven by the same four-arm case, and the test asserts all
-- four arms through the document's own examples.
row_bounds as (
  select n.*,
         lpad(n.a::text, 6, '0') || lpad(n.b::text, 6, '0') || lpad(n.c::text, 6, '0') as lo,
         case
           when n.op = '^' then case when n.a = 0
                 then lpad('0', 6, '0') || lpad((n.b + 1)::text, 6, '0') || lpad('0', 6, '0')
                 else lpad((n.a + 1)::text, 6, '0') || lpad('0', 6, '0') || lpad('0', 6, '0')
             end
           when n.op = '~' then lpad(n.a::text, 6, '0') || lpad((n.b + 1)::text, 6, '0') || lpad('0', 6, '0')
           -- `>=x` has NO upper bound, and that is NULL rather than a sentinel.
           -- The first version used '~~~~~~~~~~~~' and it was correct under the C
           -- collation and wrong under anything else: glibc and ICU ignore
           -- punctuation at the primary comparison level, so '~' and '0' compare
           -- as two empty strings and `>=0.2.0` matched nothing at all on this
           -- cluster. A collation is a property of the DATABASE, not of the
           -- query, so the comparison below handles the open end explicitly
           -- instead of encoding an ordering into a literal.
           when n.op = '>' then null::text
           else lpad(n.a::text, 6, '0') || lpad(n.b::text, 6, '0') || lpad((n.c + 1)::text, 6, '0')
         end as hi
    from norm n
),
asked_bounds as (
  select lpad(w.a::text, 6, '0') || lpad(w.b::text, 6, '0') || lpad(w.c::text, 6, '0') as lo,
         case
           when w.op = '^' then case when w.a = 0
                 then lpad('0', 6, '0') || lpad((w.b + 1)::text, 6, '0') || lpad('0', 6, '0')
                 else lpad((w.a + 1)::text, 6, '0') || lpad('0', 6, '0') || lpad('0', 6, '0')
             end
           when w.op = '~' then lpad(w.a::text, 6, '0') || lpad((w.b + 1)::text, 6, '0') || lpad('0', 6, '0')
           when w.op = '>' then null::text
           else lpad(w.a::text, 6, '0') || lpad(w.b::text, 6, '0') || lpad((w.c + 1)::text, 6, '0')
         end as hi
    from (select * from asked where op is not null) w
)
select r.name,
       r.description,
       r.language,
       r.kind,
       r.core_constraint,
       r.base_path,
       r.manifest
  from row_bounds r
  left join asked_bounds b on true
 -- `b.lo is null` is "no contract filter was asked for", so every row passes. The
 -- intersection is `b.lo < r.hi and r.lo < b.hi` with each side's unbounded end
 -- skipped rather than compared, and the STRICTNESS is load-bearing: ^0.1.0 is
 -- [0.1.0, 0.2.0) and ^0.2.0 is [0.2.0, 0.3.0), they touch at 0.2.0, and 0.2.0 is
 -- in NEITHER, so `<=` would call that a match.
 where b.lo is null
    or ((r.hi is null or b.lo < r.hi) and (b.hi is null or r.lo < b.hi))
 order by r.name asc
 limit sqlc.arg('page_size')::int + 1;

-- name: GetService :one
--
-- `GET /v1/services/{name}`. One row or `pgx.ErrNoRows`, and the caller turns
-- that into the document's 404.
--
-- NOT `where name = $1 and state = 'published'`, for the reason the header gives:
-- a direct GET by name should still answer for an `unlisted` row — someone who
-- has the URL deserves the answer — but that is a decision the API makes, and the
-- PUBLIC ROLE CANNOT MAKE IT: `services_public_read` is the only SELECT policy
-- naming `pantry_public` and `service_is_visible` is false for `unlisted`, so an
-- unlisted row is not merely unlisted by this query, it is unreachable by this
-- role. The decision is the role's, and the role is in `db.go`.
select name,
       description,
       language,
       kind::text           as kind,
       core_constraint,
       base_path,
       manifest
  from pantry.services
 where name = sqlc.arg('name');

-- name: CountServices :one
--
-- `/readyz`'s `services` field. A COUNT and not `len(List(...))`, which is why
-- `Catalog` has a third method: the probe asks "is there anything to serve" and
-- loading a page to answer it turns a health check into a table scan.
--
-- It counts what RLS lets this role see, which is the number the document means —
-- "how many services are loaded" — rather than the number of rows in the table.
select count(*) from pantry.services;

-- name: RequirementsForService :many
--
-- THE COMPATIBILITY GRAPH, forward direction: what does running this need?
--
-- `service_compat` is the moat — it answers "which services can I run that
-- compose with what I already run?", which is a question about composition and
-- not about similarity. This is that question, for one service.
--
-- ON THE WIRE SINCE registry-compat-05: `GET /v1/services/{name}/requirements`,
-- with the backward direction beside it as `GET /v1/services/{name}/required-by`.
-- `data-01` left this query off the wire on purpose — the published `Service`
-- object had no field for it and `cafaye-ts` already had a client generated from
-- the document — and the decision that packet asked for is now made: the graph
-- gets TWO OPERATIONS rather than a field, because the two directions are two
-- questions asked by two different people (an operator choosing what to run, an
-- operator deciding whether a change is safe), and a field on `Service` would
-- have made every list row carry graph data nobody asked for on that call.
--
-- `requires` edges only. `conflicts_with` is a different question ("what must I
-- NOT run alongside"), and answering both under one name is how a caller ends up
-- installing a conflict.
--
-- `service_is_visible` is applied to the TARGET here even though the role's own
-- policy on `service_compat` filters on `service_id`, because `00006` says so
-- explicitly: "an edge is visible exactly when its `target_id` service is
-- visible… a partial map of the graph is a map of the parts somebody has not
-- finished hardening". The graph is a map of what every service depends on.
select t.name        as target_name,
       c.version_range,
       c.dependency
  from pantry.service_compat c
  join pantry.services t on t.id = c.target_id
 where c.service_id = (select s2.id from pantry.services s2 where s2.name = sqlc.arg('service_name'))
   and c.kind = 'requires'
   and pantry.service_is_visible(t)
 order by t.name asc;

-- name: RequiredByForService :many
--
-- THE COMPATIBILITY GRAPH, backward direction: who requires this service?
--
-- The upgrade-safety question, and the reason 00004 indexes `service_compat` in
-- BOTH directions: before changing a service's contract, an operator reads this
-- to learn who is reading it. The forward query answers "what do I need to
-- run?"; this one answers "what breaks if I change?".
--
-- The mirror of `RequirementsForService` in every respect that matters, and the
-- one respect that differs is the one to get right: the visibility predicate
-- sits on the REQUIRING side (`service_id`), not the target. In the forward
-- direction the subject is `service_id` and the other endpoint is the target;
-- here the subject is the target and the other endpoint is the requirer. An
-- edge from a draft service into a published one is work in progress and is
-- hidden here, exactly as its mirror image is hidden there — which is
-- 00007's policy anyway, since the policy requires BOTH endpoints visible and
-- these predicates only re-state the half the join would otherwise leak
-- through.
--
-- `requires` edges only, like the forward query. A `conflicts_with` edge in
-- this answer would be worse than useless: it names a service the caller must
-- NOT run alongside, and this operation's whole meaning is "who runs me".
select s.name        as requirer_name,
       c.version_range,
       c.dependency
  from pantry.service_compat c
  join pantry.services s on s.id = c.service_id
 where c.target_id = (select s2.id from pantry.services s2 where s2.name = sqlc.arg('service_name'))
   and c.kind = 'requires'
   and pantry.service_is_visible(s)
 order by s.name asc;
