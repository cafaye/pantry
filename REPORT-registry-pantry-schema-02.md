# REPORT — registry-pantry-schema-02: the service's own role can do nothing

`worker/reg-schema-02` · PostgreSQL 18.4 (Homebrew) · every command bounded by `timeout`

---

## 1. The three defects

### 1. `pantry` held no `USAGE` on its own schema — **confirmed, and it was worse than reported**

The packet's reproduction is exact. Measured on 18.4 with the schema owned by the
provisioning role and the tables handed to `pantry`:

```
set role pantry_public; select count(*) from pantry.services;   ERROR:  permission denied for schema pantry
set role pantry_public; insert into pantry.publishers …;         ERROR:  permission denied for schema pantry
set role pantry;      select count(*) from pantry.services;   ERROR:  permission denied for schema pantry
set role pantry;      insert into pantry.publishers …;         ERROR:  permission denied for schema pantry
```

**The packet reports `pantry_public` working and only `pantry` locked out. At the
committed tree, BOTH roles are locked out** — I could not reproduce a green public
read, and the cause is one line the packet did not name.
`00006_rls.sql:425` grants `usage on schema pantry to pantry_public` and
`00006_rls.sql:441` runs `revoke all on schema pantry from pantry_public`, under a
comment about DDL. `revoke all` removes `USAGE`, so it cancels the grant sixteen
lines earlier and leaves the public read role holding nothing on the schema it was
just given `SELECT` on four tables of. **A revoke written to subtract one privilege
and subtracting another.** Every other denial in the directory is affected, and in
the most expensive way: a public read answers `permission denied` rather than
`0 rows`, so a broken deployment looks like a broken policy rather than a missing
grant.

Fix: one `revoke … from public`, then one `grant usage … to` **all four** roles,
with nothing after it. Four names in one statement is what makes a reader able to
see that none was missed.

**Why the original comment looked right, which is the part worth keeping.** An
owner *does* hold `SELECT/INSERT/UPDATE/DELETE` on its tables implicitly — the
argument was correct about tables, which is why there is still no table grant to
`pantry`. And an owner *of the schema* holds `USAGE` implicitly too, so on a
machine where `pantry` owns the schema the omission is invisible: owner's implicit
privileges are not in `nspacl`. It is invisible precisely where a developer looks.
In production `pantry` does not own the schema, because `00001` needs
`create role` — **measured: applying the directory connected as `pantry` fails with
`permission denied to create role`** — so a provisioning role creates the objects
and the grants are the only thing that supplies `USAGE`. See `DECISIONS.md` D32.

**The one thing I did not expect.** Removing that single grant and re-running the
suite turns **34 of 77 checks red, and only 4 of them are about `pantry`.** The
rest fail because a referential-integrity check runs as the **owner of the
referencing table**, so `services.publisher_id`'s FK to `publishers` fails for
*every* writer:

```
set role pantry_admin;
insert into pantry.services (… publisher_id …) values (…);
ERROR:  42501: permission denied for schema pantry
LINE 1: SELECT 1 FROM ONLY "pantry"."publishers" x WHERE "id" OPERATOR(=) $1 FOR KEY SHARE OF x
LOCATION:  aclcheck_error, aclchk.c:2795
```

An admin cannot insert a service; the fixture seed fails. The table owner's
privileges are load-bearing for everybody's writes, and the error names a table
nobody was thinking about. This is now in `00006_rls.sql` next to the grant,
because it is the strongest argument for making the grant explicit rather than
relying on ownership.

### 2. `FORCE` plus no INSERT policy — **the measurement was wrong; the decision was missing**

The packet's table says `insert-policies-for-pantry = 0` on all four tables. It is
not. Measured from `pg_policies` at the committed tree:

```
publishers        insert policies naming pantry = 0
services          insert policies naming pantry = 1
service_versions  insert policies naming pantry = 1
service_compat    insert policies naming pantry = 1
```

`services_publisher_insert`, `service_versions_publisher_insert` and
`service_compat_publisher_insert` all name `pantry` alongside `pantry_publisher`.
Only `publishers` has none — and `publishers` has no INSERT policy for
`pantry_publisher` either, which is MVP-SCOPE's answer (Phase 1 is official-only,
"adding a service is a commit and a reviewed pull request"), not an oversight.

**But the underlying defect is real and bigger than the one reported**, and it is
not about INSERT at all:

* **`pantry_admin` had eight policies and zero grants.** Four policies per table,
  all correct, all naming the role — and no table privilege anywhere. Postgres
  checks the table privilege *before* RLS, so every statement died in
  `aclcheck_error` and no policy was ever consulted. `pantry_admin` could not review
  a submission, could not publish one, and could not run the first-party ingest: the
  three things it exists for. `\dp` reads as a complete admin boundary because the
  policies were complete. Fixed by a grant that mirrors the policies exactly —
  every command they allow, on every table they cover — and widens nothing.
* **The publisher-shaped insert policies cannot admit a first-party row at all.**
  A first-party service has `publisher_id IS NULL` by `00003`'s
  `services_first_party_has_no_publisher` CHECK; every publisher-shaped insert
  policy requires `publisher_id = current_publisher_id()`. So no policy in the
  directory could have ingested anything, and the one that could would be an
  unconditional one on the role that owns the tables.

### 3. No executable test — **built, and it runs**

`tests/rls.sh` + `tests/rls_checks.sh`. 77 checks, `initdb` a scratch cluster,
apply the six Up sections goose-style, assert, remove the cluster. No driver in
`Cargo.toml` (this packet may not touch it), no dependency added to `go.mod` for a
suite whose subject is SQL. `tests/` is cargo's integration-test directory, which
compiles `tests/*.rs` and ignores everything else — and `bin/prime` counts test
binaries with `find tests -maxdepth 1 -name '*.rs'`, so neither file moves a
number the gate prints.

---

## 2. The role/operation mapping I settled on, and why

Full table and the rejected alternative in `DECISIONS.md` D31; the reasoning is in
`00006_rls.sql` next to the grants.

| operation | role | why this one |
|---|---|---|
| public catalog read | `pantry_public` | `SELECT` on four tables and nothing else — cannot write even if a policy were wrong |
| a publisher's own rows | `pantry_publisher` | scoped by policy to `current_publisher_id()` |
| review, publish, **and the first-party ingest** | `pantry_admin` | the decision-maker, deliberately not the deploy path |
| migrations, DDL | the provisioning role | `00001` needs `create role`; `pantry` cannot apply its own migrations |
| backfill | `pantry`, with `begin_publisher/1` called | so the owner obeys the same rule as everybody rather than being the one identity that bypasses the boundary it wrote |

**`pantry` has no INSERT policy, deliberately.** The alternative was an
unconditional `for insert to pantry with check (true)` on four tables: it makes the
deploy path the ingest path, which is the identity `pantry_admin` exists to keep
separate, and it reopens in policy form the hole `FORCE` just closed. Chosen
instead: the ingest runs as `pantry_admin`, which already holds
`services_admin_insert with check (true)` and now holds the grant for it.

`FORCE` is still worth it and I did not touch it. Its cost is asserted rather than
described: `tests/rls.sh` F2 measures the owner reading **zero** rows with no
identity, F3 measures it scoped to one publisher with an identity, and F7 measures
the owner's `DELETE` of its own row changing **0 rows** — which only happens
because FORCE subjects the owner to a `using (false)` policy.

**Cost of the choice, stated rather than discovered later:** the fleet's manifests
reach the database through a role that is not the one serving HTTP, so the sync
needs a login that is a member of `pantry_admin`. One provisioning statement, and
`00001` already says credentials are the cluster's business. `pantry` is
`NOINHERIT`, so the membership cannot leak into the serving role.

---

## 3. How the RLS denials are proved

Every denial is an **executed statement** asserting a SQLSTATE *and which barrier
refused it*:

* `permission denied for table X` — the **grant** boundary; Postgres rejected it
  before RLS was consulted.
* `row-level security` — the **policy** boundary.

Asserting the SQLSTATE alone would pass a suite whose policies were all
`using (false)` and whose grants were all missing. Three checks changed their
expected barrier because measurement said so:

* **D13** (publisher deletes its own service) is a *grant* denial —
  `pantry_publisher` has no `DELETE` on `services`, so the `using (false)` policy
  is never reached. The file's own comment says this; the check now asserts the
  mechanism that actually runs.
* **F7** (the owner deletes its own service) is a *silent* `DELETE 0`. A `using`
  clause that matches nothing removes the row from the candidate set rather than
  raising, so asserting an error would have asserted a mechanism Postgres does not
  have.
* **D17** (publisher self-registration) is a grant denial too — one barrier, not
  two, and I say so rather than implying defence in depth.

**Publisher isolation is an exact row set, not a count.** `D1` expects
`alpha-api,alpha-draft`; `D4` expects exactly the two edges of those services.
"Only its own rows" names the rows it saw and, by the exactness, not the third one.
`D5` asserts the negative case directly: another publisher's row is *absent from
the result*, not an error. `D1` doubles as a self-check of the harness — if `SET
ROLE` from a superuser session bypassed RLS it would answer five rows instead of
two, and every isolation check after it would be asserting nothing.

**Three invariants derived from `pg_policies` rather than typed out**, so a table
added tomorrow is covered the day it lands:

* **G1** every role named by any policy holds `USAGE` on the schema.
* **G2** every command any policy allows has a matching table privilege for the role
  it names.
* **G3** nobody holds a table privilege they cannot reach.

G1 and G2 are red on the tree this packet started from (below). G2 is the one that
would have caught `pantry_admin`'s missing grants — a role with no privilege never
reaches a policy, so that gap is *invisible in `pg_policies`*, which is exactly
where a reviewer would have looked.

**A refusal that would have proved nothing, and was replaced.** My first C13 was
"attempt `set role pantry_admin` as `pantry_public` and watch it fail". It passes:
measured, `set role pantry_public; set role pantry_admin;` returns `SET` twice and
`current_user` is `pantry_admin`, on a cluster where `pg_auth_members` has no row
for either role — **a superuser's `SET ROLE` is not subject to membership.** The
check would have passed for every role in the schema while proving nothing. Replaced
with the two catalog facts that actually make escalation impossible: `pantry_public`
is a member of no role, and every role here is `NOINHERIT` (C13/C14/C15).

---

## 4. Can the tests be made to fail, and how I checked

`tests/rls.sh` — **77 run, 77 passed, 0 failed, 0 skipped**, exit 0.

Four mutations, each verified **in the file by grep before the suite ran** (a
mutation that silently failed to apply produces a green suite that proves nothing),
each reverted, and the tree re-run green at the end.

| # | mutation | grep confirmed | result |
|---|---|---|---|
| M1 | drop `, pantry` from the usage grant | mutated line found ×1 | **34 of 77 red**, exit 1 |
| M2 | re-add `revoke all on schema pantry from pantry_public` | revoke found ×1 | **18 of 77 red**, exit 1 |
| M3 | delete the `pantry_admin` table grant | remaining grants ×0 | **29 of 77 red**, exit 1 |
| M4 | flip one assertion (C1's expected row set) | flipped text found ×1 | **1 of 77 red** — `FAIL C1` |

M1's named failures: `A1`, `B1`–`B4`, `F2`–`F9`, and invariants `G1`/`G3` — plus
the fixture seed, because of the FK mechanism in §1.
M2's: `A2`, the whole `C` tier (`C1`–`C16`), `E8`, `G1`, `G3`, `H3`, `H4`.
M3's: `A0` (the fixture seed), all of `E`, and `G5`.
M4 is the control: one wrong literal, one red row, named.

Baseline before the mutations and after the reverts: `77 run, 77 passed`, exit 0.
Transcript: the four `=== ` sections above, from one run of a script that applies,
greps, runs, restores and re-runs.

**Two harness bugs the mutations did not find, and the first run did.** `psql -q`
suppresses command tags — `INSERT 0 1`, `UPDATE 0`, `DELETE 0` — so 26 of the first
run's failures were the harness asserting against output that was not printed. The
fix is `-t -A` and not `-q`: tags in, headers out. All 26 were my mistake and none
was the schema's; a suite whose first run is 26 red for a formatting reason is a
suite nobody trusts on the run that matters.

---

## 5. Findings outside the packet

1. **`00006_rls.sql` cites a test that does not exist, eight times.**
   `00005_functions.sql:102,186` and `00006_rls.sql:21,67,141,217,376` and
   `00003_services.sql:219` all say "`00007` asserts it". There is no `00007`, and
   no `tests/` file. `00006_rls.sql:377` also names a `required_by` **query** that
   does not exist anywhere in the directory. Every one of those is a forward
   reference standing in for a check — the prose-is-not-a-gate defect in its purest
   form. I did not renumber them: they are `0000N_rls.sql`'s own internal promises
   about a file the first packet planned, and rewriting nine citations is a
   separate decision about naming. **`tests/rls_checks.sh` now asserts the claims
   that were checkable** (the refused publisher DELETE and the rows surviving it,
   D13/D14; the traversal running as every role, D4/E3); the `required_by` query
   itself is still a comment about a query nobody has written.
2. **`DECISIONS.md` had no D31**, though `00006_rls.sql` says "the reasoning is in
   DECISIONS.md as D31". Written. The pointer now resolves.
3. **The migrations cannot be applied by the service's own role.** `00001` needs
   `create role`, and `pantry` has no `CREATEROLE`. Measured: `permission denied to
   create role`. This is what makes the ownership question in D32 real, and it means
   "the service role runs its own migrations" was never true in this design.
4. **`pantry` is created only if absent, so `00001` assumes the role may or may not
   exist** — and in shape 2 the harness creates it as part of applying. Not a
   defect; noted because it is the one statement in `00001` whose behaviour depends
   on cluster state.
5. **Another session is live in `wt-reg-pantry-schema-01`** (packet 01's worker,
   still running at the time of writing, with uncommitted edits to
   `00001_roles.sql`/`00006_rls.sql` and a new `migrations/README.md`). I did not
   touch it. Its diff independently found the same `revoke … from pantry_public`
   ordering bug and the same missing `pantry_admin` grant — two independent
   measurements agreeing, which is worth more than either. **It also creates
   `migrations/README.md`, which I deliberately did not**, so the merge is not a
   conflict over one path.

---

## 6. What I ran, and what I did not

**Ran, every command bounded by `timeout`:**

| command | result |
|---|---|
| `./tests/rls.sh` | `77 run, 77 passed, 0 failed, 0 skipped`, exit 0, twice (before and after the mutations) |
| `./tests/rls.sh --list` | 76 names, no database started |
| `PANTRY_PG_BIN=/nonexistent ./tests/rls.sh` | exit 0, one counted `skip:` line naming the directory |
| `PANTRY_PG_BIN=/nonexistent PANTRY_RLS_REQUIRED=1 ./tests/rls.sh` | exit 3 |
| mutation script (M1–M4 + baseline + after) | 4/4 red, tree restored green |
| `./bin/prime-go` | green: gofmt, build, vet, test, generated-server check |
| the new `bin/prime` RLS block, extracted and run verbatim | `rls: 77 of 77 checks passed, 0 tier skipped` |
| the same block with no postgres | `rls: 0 of 0 checks passed, 1 tier skipped` + the named reason |
| `bash -n` on all three scripts | clean |

**Not run, and why:**

* **The Rust tier of `bin/prime`** (`cargo fmt --check`, `cargo build
  --all-targets`, `cargo clippy -D warnings`, `cargo test`) — 110 crates, and this
  packet changed no file under `src/`, no `Cargo.toml` and no `tests/*.rs`. I ran
  `./bin/prime-go`, the other tier, plus the block I added to `bin/prime`, extracted
  verbatim so the new step was verified without the cargo cost. **The full
  `bin/prime` has not been run end to end on this branch.**
* **CI has no postgres service**, so I could not check that the job passes there.
  The suite needs `initdb` and a free port, which a GitHub runner has by default;
  nothing else in `.github/workflows/` was touched, and **wiring the RLS job into
  CI is not done** — `PANTRY_RLS_REQUIRED=1` is what a CI job should set, and that
  is a one-line change somebody with the workflow open should make.
* **PostgreSQL 16 and 17.** Only 18.4 was available; the suite finds 16/17 if
  installed but nothing here was run against them. The `security_invoker` view in
  `00005` is 15+, and nothing in the fix is version-specific, but "green on 18.4" is
  the whole of the claim.
* **A concurrent-write or pool-reuse test** for the transaction-local
  `pantry.publisher_id` GUC. `00005`'s comment claims a pool is why `is_local=true`;
  the suite proves identity scoping, not pool behaviour. That needs the Go service,
  which this packet may not touch.

**Cleanup:** every scratch cluster I started was stopped and its data directory
removed (`pantry-rls.*` workdirs, plus two stale ones from an earlier session in the
same temp tree). `lsof` on 55430–55520 is clear. I did not touch the Homebrew
cluster on 5432 or the one that was listening on 55432 when I started, which is not
mine.

---

## 7. The single next move

**Wire `tests/rls.sh` into CI as a required job** — one workflow step, `apt-get
install postgresql` (or `brew`), `PANTRY_RLS_REQUIRED=1 ./tests/rls.sh`, no
`POSTGRES_HOST` and no service container, because the suite starts its own server
and that is the only reason it can run on a machine that has nothing.

It is next because this packet's whole finding is that a schema with no executable
check is a schema with no data path, and a suite that only runs when a developer
happens to have Homebrew Postgres installed is the same defect one level up. The
eight `00007` citations should be repointed at it in the same commit — they are
promises the tree is currently making to a file that does not exist.