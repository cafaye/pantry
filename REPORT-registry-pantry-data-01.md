# REPORT — registry-pantry-data-01: the catalog answers 200

`worker/reg-data-01` · PostgreSQL 18.4 (Homebrew) · Go 1.26.1 · sqlc v1.30.0 ·
every command bounded by `timeout`

Two commits. `0683722` is the connection, the role and the queries;
`1787b22` is the `Catalog`, the handlers and the tests.

---

## 1. The one thing

`internal/catalog.Catalog` is a real implementation over the migrations, and
`/v1/services` answers 200 with rows from the database.

**Measured, against a running binary connected to a migrated, seeded database:**

```
GET /v1/services            200  [caf courier identity muse parcel]
                                page: has_more false, next_cursor null
GET /v1/services/identity   200  the manifest's own fields; basePath "/v1"
GET /v1/services/caf        200  kind "cli", "basePath":null, "exposes":null
GET /v1/services/nope       404  application/problem+json, code not_found
GET /v1/services?contract=%3E%3D0.2.0
                              200  [muse parcel]
GET /readyz                 200  {"services":5,"status":"ok"}
GET /healthz                200
```

Seven rows are seeded and five come back. `draft-only` (a third-party draft) and
`unlisted-one` (published, then taken off the listing) are in the database and
absent from the answer, and **the only thing that removed them is
`pantry.service_is_visible`** — there is no `where state = 'published'` anywhere
in this branch, and that absence is asserted by naming the two rows the answer
must NOT contain.

---

## 2. The `Catalog` interface as it now stands

```go
type Catalog interface {
	List(ctx, filter Filter) (services []api.Service, next *string, err error)
	Get(ctx, name string) (api.Service, error)
	Count(ctx) (int, error)
}

type Filter struct {
	Kind     *api.ServiceKind   // the document's enum, in the GENERATED type
	Language *api.Language
	Contract *string            // core's four-form range, matched by INTERSECTION
	Limit    *int               // ← changed shape
	Cursor   *string
}
```

### What changed and why

**`Limit` went from `int` to `*int`.** The document declares `minimum: 1`,
`maximum: 100`, `default: 25`, and **oapi-codegen enforces none of the three** —
it binds the parameter as a string and leaves the range and the default to the
caller. With `int`, "the caller did not ask" and "the caller asked for zero" are
the same value, so `?limit=0` is either silently answered as 25 or silently
clamped. A parameter that is read and then not obeyed is worse than one that is
refused. A pointer is the difference between a default and a request.

**Filter validation lives in `Filter.Validate`, not in the handler.** The generated
binder does not check `kind` or `language` against their enums, does not apply
`limit`'s default, and does not refuse `limit=0`. Those checks are real work with
nowhere to live except somewhere this package chooses, and they live next to the
query that obeys them, where a handler cannot forget them. The two enums are
validated against the **generated constants**, not a list copied out of the
document, so a vocabulary that grows is a vocabulary this accepts unedited.

**What was dropped: nothing, because nothing needed dropping.** `kind`,
`language` and `contract` are all columns that exist and all three now do
something. Two provisional fields that had no column were never in `Filter` and
are not now.

### What is deliberately NOT a filter

`trust` and `state` are columns and they are absent, because
`pantry.service_is_visible` is the policy that reads them. Writing
`and state = 'published'` would hide the fleet's own draft services — `00005`
deliberately admits a first-party row in ANY state — and would be a second
implementation of a boundary that already exists and that `tests/rls.sh` can test.

### The one thing the schema changed that was not in the brief's list

**`GET /v1/services/{name}` cannot honour `00005`'s own note that "a direct GET by
name should still answer" for an unlisted row.** `00005` says that decision is
"the API's decision, made in the query". It cannot be, for this role:
`services_public_read` is the only SELECT policy naming `pantry_public`, and
`service_is_visible` is false for `unlisted`, so an unlisted row is unreachable
rather than merely unlisted. `pantry_admin` would see it and would also see every
draft and every publisher's private work.

So the decision is the **role's**, not the query's, and the role is in
`internal/pantrydb/db.go`. `draft-only` and `unlisted-one` both answer 404, and
that is correct: to a public reader a draft is not a service that exists, and
saying otherwise would be telling one caller what another publisher has not
published.

---

## 3. The role, and how it is set on a pooled connection

**`pantry_public`.** Set in `pgxpool`'s `AfterConnect`, which runs **once per
physical connection at the moment it is created**.

*Why `AfterConnect` and not per-checkout:* the role is a property of the
CONNECTION. A pool that issued `set role` at checkout would have a window in which
a connection is handed out holding whatever role it last held — "a pool that hands
out connections at different roles depending on checkout order is a bug that only
appears under load" is exactly the failure this avoids.

*Why it is verified rather than assumed:* `AfterConnect` issues `set role`, then
reads `current_user` back and **refuses the connection** if it is not the role
asked for. A role that could not be taken is a connection that never enters the
pool. This is load-bearing: `pantry` on its own reads **zero** catalog rows
(no identity ⇒ `current_publisher_id()` is NULL ⇒ every publisher predicate
compares against NULL), so a pool that silently failed to take the role would
answer every request with an empty list and `/readyz` would say zero.

*Why `pantry_public` and not the login role:* `00001_roles.sql` creates the three
trust levels `NOLOGIN` deliberately, so the service logs in as `pantry` (which IS
a login role, and is `NOINHERIT`) and takes the group role. `pantry_public` holds
`SELECT` and nothing else — "a role with no INSERT privilege cannot INSERT, and
RLS is never consulted", which is a different and stronger kind of guarantee than
a policy.

*Measured:* `TestTheConnectionReadsAsPantryPublic` reads `current_user` off a
pooled connection; `TestPantryPublicCannotWrite` attempts an INSERT through the
service's own connection and asserts the refusal is at the **grant** boundary
(`permission denied for table services`, 42501) and not at a policy — a different
and weaker claim.

`cmd/pantry` logs the role once at startup:
`"msg":"pantry: the registry catalog is mounted","role":"pantry_public"`.

---

## 4. The three cases, and how they are proved distinguishable

Each case is a **separate database**, not rows inserted and deleted:

| case | database | answer | what makes it different |
|---|---|---|---|
| rows present | migrated + `tests/seed.sql` | `200` `data: [caf courier identity muse parcel]` | the five rows, exactly |
| catalog genuinely empty | migrated, `--serve --empty` | `200` `data: []` | `[]` and **not** `null`; `/readyz` 200 `services=0` |
| database unreachable | a port bound and released | `503` on both data routes **and** on `/readyz`; `/healthz` still 200 | no `"data"` key in the body |

*Emptying a populated database would only prove the query returns nothing; "the
catalog is empty" is a claim about a database that was migrated and never seeded.*

*The unreachable case runs **beside** the healthy one, not instead of it.* A test
that proved 503 by stopping the shared database would also make every other test
in the package unable to run, and a 503 observed in a package where nothing else
worked would not be evidence that the 503 is specific to a dead endpoint.

`TestAnUnreachableDatabaseIsAnErrorAndNotAZero` also holds the layer below: a
store cannot even be **opened** against a dead database, so a `pantrydb.Open`
that succeeded would have turned case 3 into case 2.

### Verified by mutation, in the file, before the suite ran

| # | mutation | how it was confirmed | result |
|---|---|---|---|
| M1 | `PANTRY_PG_BIN=/nonexistent ./bin/prime-go` | — | `data path: 11 check(s) DID NOT RUN against a database (counted, and named above)`, exit 0 |
| M2 | the same with `PANTRY_DB_REQUIRED=1` | — | **exit 3**, `PANTRY_DB_REQUIRED=1 and there is no PostgreSQL to check against.` |
| M3 | a `?contract=^0.2.0` before the prefix-strip fix | the failing assertion named `invalid input syntax for type integer: "^0"` | **503** on that one query |
| M4 | a `>=` sentinel before the collation fix | the test expected `[muse parcel]` and got `[]` | **200 with an empty array** |
| M5 | `mountCatalog` deferring `store.Close()` | `curl` against the running binary: `503 … closed pool` | **503 on every route** |

M3, M4 and M5 are the three real bugs this packet found, and **none of them was
found by reading the code.** M3 and M4 were found by the contract test against a
real database; M5 was found by running the binary. That is the argument for the
harness, and it is why `tests/seed.sql` and the live run are in the report rather
than only the green.

---

## 5. Is `openapi/v1.yaml` right?

**Unchanged, and it was right.** Its shapes did not need to move. Two real
disagreements exist between it and the schema, and both are reported rather than
edited, because `cafaye-ts` has a client generated from this document and a change
is the manager's to make:

1. **`ServiceKind`.** The document publishes `[api, worker, both, cli]`;
   `pantry.service_kind` is a closed set of `(api, cli)` — `00003` records that
   `worker` and `both` were both refused (AGENTS.md MD1). Resolved in the QUERY,
   not the document: `kind` is compared as `::text`, so `?kind=worker` is a legal
   question with no rows behind it and answers `200` with `data: []`. Casting it
   to the enum would raise `invalid input value for enum`, which is a 500 for a
   question the document says is answerable.
2. **`Language`.** The document's enum is six values; `00003`'s CHECK admits eight,
   including `zig` and `javascript`. Resolved in the right layer:
   `?language=zig` is a **400** from `Filter.Validate`, naming the parameter and
   the six values it allows — which is what the document asks a 400 to do.

One thing the document says that the GENERATED server does not enforce, now
handled by hand and asserted: *"An unknown parameter is also a 400."*
`?sort=name` is `400` naming the undeclared parameters and the declared ones —
without it a client asking for a sort it did not get is handed the whole list and
concludes every entry is sorted.

---

## 6. The compatibility graph — measured, not reachable

**It is unreachable, and that is a finding rather than an omission.**

`pantry.service_compat` holds **4 rows** in the seeded database and
`pantry_public` reads **0** of them. Not the grant — `00006_rls.sql` grants
`SELECT` on that table to `pantry_public`, and `has_table_privilege` answers true.
**No SELECT policy on `service_compat` names `pantry_public`.** The eight that
exist are four publisher-shaped (`pantry`, `pantry_publisher`) and four admin
(`pantry_admin`). Measured with `pg_policies`:

```
service_compat_admin_select     |SELECT| {pantry_admin}
service_compat_publisher_select|SELECT| {pantry,pantry_publisher}
… six more, none naming pantry_public
```

So the moat is invisible to the only role the catalog reads as. The query is
correct and the join works; there is nothing on the other side of it.

**What is committed:** `RequirementsForService` in
`internal/pantrydb/queries/catalog.sql` (forward direction, `requires` only,
`service_is_visible` applied to the **target** because `00006` says "an edge is
visible exactly when its `target_id` service is visible"), the generated Go, and
`Postgres.Requirements` on the concrete type so it is callable.

**Why it is not on the wire, precisely:** the published `Service` object has no
field for it, and adding one is a deliberate contract change for the manager.
This packet does not fabricate a placeholder shape and does not quietly omit the
question.

**The tripwire.** `TestTheCompatibilityGraphIsUnreadableByTheCatalogRole` asserts
the **defect** and reads `pg_policies` for the mechanism, so the day somebody adds
`service_compat_public_read` it goes **red** and says the graph has become
readable. A test that skipped here would let the gap outlive the packet that found
it. Its failure message names what to do with the test when that happens.

---

## 7. Findings outside the packet

1. **`grant pantry_public to pantry` is missing from the migrations, and the read
   path cannot start without it.** `00001_roles.sql` creates `pantry` as LOGIN and
   `pantry_public` as NOLOGIN, and grants the GROUP its table privileges — but it
   never grants the LOGIN role **membership** in the group. `NOINHERIT` is the
   point (a membership must be taken with `set role`, never inherited), and taking
   one requires membership. On a database built purely from this directory,
   `set role pantry_public` fails with `permission denied to set role`.
   `tests/rls.sh --serve` performs the one statement, loudly, and it is named in
   the report rather than worked around in Go. `00006`'s own comment already
   records the same fact for `pantry_admin` ("the sync needs a login that is a
   member of `pantry_admin`") — so this is the second place the directory creates
   the role and leaves the membership to the operator, and neither is a comment a
   reader meets before the service fails to start.

2. **`services.language` and `services.core_constraint` are not constrained to
   equal the manifest they duplicate.** `manifest->>'language'` and the
   `services.language` column can disagree, in which case a row is **filtered** by
   one value and **described** by another. The projection reads the manifest, per
   the document's own field-provenance table. A `CHECK` tying a jsonb key to a
   sibling column belongs with the ingest, not with a read path.

3. **`00003`'s comment cites "`00007` adds the query this index exists for",** and
   the schema packet's report already records eight such forward references to a
   file that does not exist. Now that a real query exists over `services`, the
   search index (`services_search_idx`, GIN `to_tsvector`) still has no query, and
   `services_name_trgm_idx` has none either. Both are correct to have and neither
   is exercised; the packet was told not to build search, so this is a note.

4. **`tests/rls_checks.sh`'s fixture cannot be reused by a client that needs real
   manifests**, which is why `tests/seed.sql` is a second file rather than a
   parameter. Its shapes are deliberately abstract (`'{}'::jsonb`, fixed uuids the
   assertions read as literals); the served catalog has to be renderable. Two
   seeds, two subjects, one harness.

---

## 8. What I ran, and what I did not

**Ran, every command bounded by `timeout`:**

| command | result |
|---|---|
| `./bin/prime-go` | green, `data path: every check ran against a real PostgreSQL (0 skipped)`, exit 0 |
| `PANTRY_PG_BIN=/nonexistent ./bin/prime-go` | `11 check(s) DID NOT RUN … (counted, and named above)`, exit 0 |
| `PANTRY_PG_BIN=/nonexistent PANTRY_DB_REQUIRED=1 ./bin/prime-go` | exit **3** |
| `go test -count=1 -v ./internal/pantrydb/` | **11 gates entered, 0 did not run**, 11 PASS, exit 0 |
| `./tests/rls.sh` | **77 run, 77 passed, 0 failed, 0 skipped**, exit 0 — unchanged by this branch |
| `go run ./cmd/pantry` against `tests/rls.sh --serve` | the six measurements in §1 |
| `sqlc generate` | the committed `internal/pantrydb/gen` is its output, regenerated in this branch |
| `bash -n` on `tests/rls.sh`, `bin/prime-go` | clean |
| `git diff --name-only 0bfcb1f..HEAD \| grep -E '^(src/\|Cargo\.\|docker/Dockerfile\|registry/\|schemas/\|vendir\.lock\.yml\|openapi/)'` | **none** |

**Not run, and why:**

* **The Rust tier of `bin/prime`** (`cargo fmt --check`, `cargo build
  --all-targets`, `cargo clippy -D warnings`, `cargo test`) — 110 crates, and this
  branch changes no file under `src/`, no `Cargo.toml` and no `tests/*.rs`. I ran
  `./bin/prime-go`, the other tier. **`bin/prime` has not been run end to end on
  this branch**, and the handoff records that it is red on `master` for
  cross-repo drift reasons that predate this packet (`core_pin`, `drift`,
  `recorded_copy`, `schema` — sibling checkouts and a recorded-ref distance).
  I did not touch `registry/`, `schemas/` or `vendir.lock.yml`.
* **PostgreSQL 16 and 17.** Only 18.4 was available. `tests/rls.sh` finds 16/17 if
  installed; nothing here was run against them. "Green on 18.4" is the whole of
  the claim — and the collation bug in §4/M4 is exactly the kind of thing a
  different locale's default collation would have found differently, so **the
  interval arithmetic is collation-independent by construction (NULL bounds, not
  sentinel literals) rather than by having been run against several collations.**
* **A concurrent-write or pool-reuse test** for the transaction-local
  `pantry.publisher_id` GUC. The schema packet's report names this as its own gap
  and it needs the publisher write path, which this packet does not build.
* **A row whose `manifest` is missing a required key.** The projection refuses it
  (`catalog: service %q has no %q in its manifest, and the document marks it
  required`) and a unit-level assertion of that refusal would be cheap; it is not
  written, and the code path is currently unexercised. Named rather than implied.

**Cleanup:** every scratch cluster was stopped and its data directory removed.
`lsof` on 55470 and on the temporary 18080/18081 ports is clear; no
`pantry-rls.*` workdirs remain. No container, image or volume was created.

---

## 9. Anything surprising

* **`go test` discards a passing package's output entirely.** `internal/pantrydb`
  printed a correct counted-skip row and it appeared on no green run at all — a
  silent pass, which is the exact failure mode the working rules warn about. Found
  by noticing the row was missing, not by reasoning about it. `bin/prime-go` now
  runs verbosely and echoes it.
* **A collation is a property of the DATABASE, not of the query.** `>=0.2.0`
  matched nothing because the unbounded upper bound was the string
  `'~~~~~~~~~~~~'` and glibc/ICU ignore punctuation at the primary comparison
  level, so `'~'` and `'0'` compare as two empty strings. It is `NULL` now.
* **The hand-rolled skip counter read 6 for a run that entered 11 gates.** The
  counters moved inline at each call site; they move in one `t.Cleanup` in
  `dbGate` now, because a test that `t.Skip`s never returns from the skip and an
  increment before it counts a gate nobody entered.
* **A deferred `store.Close()` in `mountCatalog`** closed the pool the instant the
  function returned. `503 closed pool` on every route, found by `curl`.
* **oapi-codegen enforces none of `openapi/v1.yaml`'s constraints** on query
  parameters: not the two enums, not `limit`'s default, not its range, not the
  unknown-parameter rule. Four of the document's published 400s are hand-written
  as a result, and three of them are asserted.

---

## 10. The single next move

**Add the missing SELECT policy on `pantry.service_compat` for `pantry_public`,
and then put `Postgres.Requirements` on the wire.**

```sql
create policy service_compat_public_read
  on pantry.service_compat
  for select
  to pantry_public
  using (exists (select 1 from pantry.services t
                  where t.id = service_compat.target_id
                    and pantry.service_is_visible(t)));
```

It is first because **it is the moat**, and it is currently unreachable:
`00004_service_compat.sql` is the file that argues at length that this registry is
not a link directory, and the catalog role cannot read the table that holds the
argument. The `exists … service_is_visible(target)` shape is `00006`'s own stated
rule ("an edge is visible exactly when its `target_id` service is visible… a
partial map of the graph is a map of the parts somebody has not finished
hardening"), so the decision is already written down and the statement follows
from it — which is why this is a migration somebody with `00006` open should
write rather than a read-path packet.

**The statement above was MEASURED, not sketched.** Applied to a scratch cluster
with one first-party service (`hub`) and three `requires` edges out of it:

```
select count(*) from pantry.service_compat;                  -- 3, as the owner
create policy service_compat_public_read …;                   -- CREATE POLICY
set role pantry_public;
select t.name from … order by t.name;
  pub-rel                        -- published third-party target: visible
  (pub-draft absent)             -- third-party DRAFT target: hidden
  (gone absent)                  -- UNLISTED target: hidden
```

Three edges in the table, one readable, and the two that are hidden are hidden
because of their **target's** visibility rather than their own. That is the
property `00006` asks for and the one a `where service_id = …` policy would not
have: without it a caller learns the shape of a graph it cannot see.

`TestTheCompatibilityGraphIsUnreadableByTheCatalogRole` goes **red** the moment
the policy lands, and its failure message says to move the assertion into
`TestTheCompatibilityGraphIsReachable`. What remains after that is the contract
decision the manager owns: a `requirements` field on the `Service` object, or a
`GET /v1/services/{name}/requirements` route. The first is additive to a
document `cafaye-ts` has generated a client from; the second is a new operation.
Both are one line of handler once the field is chosen.
