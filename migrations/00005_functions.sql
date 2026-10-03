-- 00005_functions.sql — the functions the RLS policies and the queries need.
--
-- NOTHING HERE DECIDES TRUST. Every function in this file is either a mechanical
-- helper (`touch_updated_at`) or the ONE identity function the RLS boundary keys
-- on (`current_publisher`). A function that decided "is this row visible" would be
-- the thing MVP-SCOPE rules out, because the addendum's whole decision is that
-- trust is a policy and not a function this service happens to call.

-- THE TOUCH FUNCTION. One statement, and it exists so `updated_at` cannot be
-- forgotten — see `00003`'s index comment. `security invoker` is explicit rather
-- than assumed: this is a trigger on a table and it runs as the invoking role, and
-- writing it out means a reader who is looking for `security definer` functions in
-- this directory finds the affirmative statement instead of having to check.
-- +goose Up

-- +goose StatementBegin

create or replace function pantry.touch_updated_at()
returns trigger
language plpgsql
security invoker
as $$
begin
  new.updated_at := now();
  return new;
end;
$$;

-- =============================================================================
-- THE PUBLISHER IDENTITY. The one seam every RLS policy in `00006` keys on, and
-- it is deliberately the same shape as kit's `cafaye.current_account_id()`.
-- =============================================================================

-- WHY A GUC AND NOT A CLAIM. Two reasons, both measured rather than assumed.
--
-- The first is that the policy must be evaluable as a `(select …)` — an InitPlan
-- hoisted once per statement — rather than once per candidate row. A policy that
-- called a plpgsql function which read `current_setting` per row would be
-- measured at the 178,000-calls-for-12ms row of Supabase's table, against
-- 179-calls-for-9ms for the wrapped form. `refs/supabase`'s
-- `rls-performance-and-best-practices` doc has the numbers and the fix is
-- structural and free now: the policy reads a STABLE sql function once, and the
-- stable function reads the GUC.
--
-- The second is transaction-locality. Every driver in this fleet holds a POOL.
-- A session-level `pantry.publisher_id` on a pooled connection survives the
-- request, the connection returns to the pool still carrying publisher A, and
-- publisher B is handed it. kit's substrate documents the same reasoning at
-- length; repeating it here is the point, because the alternative is to rediscover
-- it.
--
-- RETURNS NULL WHEN NOTHING IS SET, and that is the load-bearing half. "No
-- identity" and "an identity that owns none of these rows" are indistinguishable
-- by construction, which is the right answer: a policy that RAISED on an absent
-- identity would tell a caller its request was not authenticated, and a caller can
-- be made to believe that about somebody else's request (core's D33 arriving from
-- the database end). An absent identity reads zero rows.

-- +goose StatementEnd
-- +goose StatementBegin

create or replace function pantry.current_publisher_id()
returns uuid
language plpgsql
stable
security invoker
as $caf$
declare
  raw text;
begin
  -- `missing_ok = true` is what makes an unset GUC NULL rather than an error. A
  -- dotted custom GUC needs no CREATE to be read: Postgres treats the
  -- placeholder as set-to-empty, which is the second branch.
  raw := current_setting('pantry.publisher_id', true);
  if raw is null or raw = '' then
    return null;
  end if;
  return raw::uuid;
exception
  when invalid_text_representation then
    -- An ABSENT identity is legitimate and reads nothing. A MALFORMED one is a bug
    -- in the plumbing that authenticated the request, and reading nothing would
    -- report it as a service with no rows — the worst possible way to find out.
    raise exception 'pantry.publisher_id is set to % which is not a uuid', raw
      using errcode = '22023',
            hint = 'pantry.begin_publisher/1 takes a uuid. A request that reaches the database with a malformed publisher has a bug in whatever authenticated it, and this is the cheapest place to find it.';
end
$caf$;

-- SET THE IDENTITY, ONCE PER REQUEST, TRANSACTION-LOCAL.
--
-- `is_local = true` is the design and the reason is the pool, above. A NULL
-- argument CLEARS rather than failing, because "this worker has no publisher" is a
-- legitimate state and it goes through here rather than through a raw
-- `set_config`, so there is exactly one place in this service that writes this
-- GUC.
--
-- THE AUTOCOMMIT CONSEQUENCE, STATED RATHER THAN HIDDEN. Outside an explicit
-- transaction `is_local = true` expires at the end of the statement that set it,
-- so a caller in autocommit reads zero rows. That cannot leak and it is loud
-- rather than silent — a service that suddenly sees none of its own data fails
-- its first integration test — and `00007`'s isolation assertions run every case
-- inside an explicit transaction for exactly this reason.

-- +goose StatementEnd
-- +goose StatementBegin

create or replace function pantry.begin_publisher(p_publisher_id uuid)
returns void
language plpgsql
as $caf$
begin
  if p_publisher_id is null then
    perform set_config('pantry.publisher_id', '', true);
    return;
  end if;
  perform set_config('pantry.publisher_id', p_publisher_id::text, true);
end
$caf$;

-- =============================================================================
-- THE VISIBILITY PREDICATE. The one function that says what a read may see, and
-- it is the whole reason there is one read path for first-party and third-party
-- rows (addendum: "one schema, one read path, one query planner").
-- =============================================================================

-- +goose StatementEnd
-- +goose StatementBegin

create or replace function pantry.service_is_visible(p_service pantry.services)
returns boolean
language sql
stable
security invoker
as $caf$
  -- Three ways a row is visible, and the ordering is not arbitrary:
  --
  --   1. PUBLISHED and not unlisted. The public catalog. Unlisted is a PUBLISHED
  --      row whose publisher took it off the listing, and it is visible to nobody
  --      through this predicate — see the note below.
  --   2. first_party, whatever its state. A first-party service is in the
  --      catalog because a reviewed commit put it in `registry/index.yml`, and
  --      that is a review. Requiring it to be `published` would mean the fleet's
  --      own services are invisible until a migration marks them published, which
  --      is one state nobody is going to remember to set.
  --   3. the caller's OWN row, in any state. A publisher must be able to read
  --      their own draft.
  --
  -- WHY UNLISTED IS NOT VISIBLE HERE. `unlisted` is the state a publisher moves a
  -- row to when they stop advertising it, and it is distinct from `published` in
  -- exactly one way: it is not in the listing. A direct GET by name should still
  -- answer — someone who has the URL deserves the answer — but that is the API's
  -- decision, made in the query, and it is not this function's: a predicate that
  -- leaked unlisted rows into `GET /v1/services` would put a withdrawn service
  -- back in the catalog, which is the one thing unlisting is for.
  --
  -- `(select …)` on every helper, for the InitPlan reason documented above. The
  -- `current_publisher_id()` call is the expensive one; the column reads are free
  -- and wrapping them costs nothing and keeps the shape uniform.
  --
  -- `p_service` is passed as a whole row rather than as three arguments because a
  -- plpgsql or sql function taking a composite is one argument the planner has to
  -- inline, and the alternative — three scalar arguments — is a signature a policy
  -- would have to get right three times.
  select case
           when p_service.state = 'published' then true
           when p_service.trust = 'first_party' then true
           when p_service.publisher_id is not null
             and p_service.publisher_id = (select pantry.current_publisher_id()) then true
           else false
         end
$caf$;

-- +goose StatementEnd

-- =============================================================================
-- THE GRAPH TRAVERSAL. The query the moat exists for, written down here because
-- a recursive traversal nobody has written is a table nobody has tested.
--
-- NOT a FUNCTION. It is left as SQL rather than wrapped in plpgsql, and the reason
-- is deliberate: `search_path` in a `security invoker` function is caller-
-- controlled and a recursive CTE inside a function is one `SECURITY DEFINER`
-- away from being a privilege-escalation hole. A view over it would be worse —
-- Postgres 15+ requires `security_invoker = true` on a view that touches a
-- protected table, which is one more setting to forget. So this is a query a
-- caller pastes, and `00007`'s suite runs it as all three roles, which is the only
-- way a traversal is known to respect RLS.
--
-- A view over a protected table DOES NOT INHERIT its RLS by default before
-- Postgres 15, and on 15+ it inherits only when `security_invoker` is set. Both
-- facts point the same way and this sets it explicitly: without it, the view
-- would run as its DEFINER and return rows the calling role cannot see, which is
-- the worst failure shape in this directory — a read that succeeds and lies.

-- VISITED-SET IS NOT OPTIONAL, and the omission named in `00004`'s header is what
-- makes it necessary: `requires` edges are not symmetric and not acyclic, so
-- `A requires B, B requires A` is a legal row pair and a naive recursion is an
-- infinite loop. The `not service_id = any(array(agg(service_id)))` clause is the
-- visited-set, and it is in the query rather than in the constraint because a
-- cycle is a legitimate topology and a constraint would forbid it.
-- +goose StatementBegin

create or replace view pantry.compat_closure
  with (security_invoker = true)
as
with recursive
  edges as (
    select c.service_id, c.target_id, c.version_range, c.dependency,
           1 as depth,
           array[c.service_id] as visited
      from pantry.service_compat c
     where c.service_id = current_setting('pantry.compat_from', true)::uuid
       and c.kind = 'requires'
       and c.dependency <> 'dev'
    union all
    select c.service_id, c.target_id, c.version_range, c.dependency,
           e.depth + 1,
           e.visited || c.service_id
      from edges e
      join pantry.service_compat c
        on c.service_id = e.target_id
     where c.kind = 'requires'
       and c.dependency <> 'dev'
       and not c.service_id = any (e.visited)
  )
select service_id, target_id, version_range, dependency, depth from edges;

-- +goose StatementEnd

-- =============================================================================
-- THE TOUCH TRIGGER, attached to the table `00003_services.sql` created.
--
-- It lives here rather than there for the reason that header records and the
-- reason is worth repeating because it is the ordinary way a migration directory
-- by topic produces a file that cannot apply: goose runs in FILENAME order, so a
-- trigger in `00003` naming a function `00005` defines fails with `42883 function
-- pantry.touch_updated_at() does not exist`. Measured on the run that found it.
-- =============================================================================
create trigger services_touch_updated_at
  before update on pantry.services
  for each row execute function pantry.touch_updated_at();

-- +goose Down

-- The trigger first, then the view, then the functions it names. The order is
-- dependency order and it is the whole content of the note at the top of this
-- Down: a view or trigger whose function is already gone cannot be dropped, and
-- Postgres will say so rather than leaving a broken object behind.
drop trigger if exists services_touch_updated_at on pantry.services;

drop view if exists pantry.compat_closure;

-- The functions are dropped, unlike the ENUM TYPES in `00003`'s Down, and the
-- asymmetry is the reason: `00006_rls.sql`'s policies reference
-- `pantry.current_publisher_id()` by name, so a Down that removed this function
-- while those policies still existed would leave policies whose expression no
-- longer resolves. Postgres would refuse to drop a function a policy depends on
-- anyway — which is the correct outcome, and this statement reaching it means the
-- order of these Downs matters, which is what the directory README records.
drop function if exists pantry.begin_publisher(uuid);
drop function if exists pantry.current_publisher_id();
drop function if exists pantry.service_is_visible(pantry.services);
drop function if exists pantry.touch_updated_at();
