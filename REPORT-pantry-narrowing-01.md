# REPORT-pantry-narrowing-01 — the barrier nobody named, and a narrowing mutation

**Branch** `worker/pantry-narrowing-01` · **base** `3ccc679` (the merge of
`worker/pantry-rls-01`) · **probe**
`reports/pantry-narrowing-01/probe.sh` · **PostgreSQL** 18.4 (Homebrew) ·
**suite** `tests/rls.sh`, 89 checks

---

## 0. The two claims under test

**Claim one.** `REPORT-pantry-rls-mutations-01.md` §4 (`B2`) recorded this and
declined to resolve it:

> So `with check (true)` widens the *column* boundary (`U3` now allowed) but the
> *identity* boundary on `publishers.id` is held by something that is not this
> `WITH CHECK`. **I did not identify the mechanism and I am not claiming it as a
> result.**

The packet's hypothesis is the primary key — `id uuid primary key default
gen_random_uuid()` at `migrations/00002_publishers.sql:51` — with a unique
violation refusing the statement "regardless of RLS". **That hypothesis is
falsified, and Part 1 below names the real barrier.**

**Claim two, the larger one.** Every policy mutation in the recipe that preceded
this one is a **widening**. The previous report closed with the gap stated
plainly:

> This packet contains no narrowing mutation. … the opposite failure — a policy
> that refuses *too much*, breaking a legitimate publisher — is untested by this
> packet.

Part 2 closes it, or says why it cannot be closed.

**One-line result: the barrier on `publishers.id` is `publishers_publisher_select`
— a `FOR SELECT` policy — because PostgreSQL applies a SELECT policy to an
`UPDATE` whenever the statement reads a column of the relation. It is a `policy`
refusal. It is asserted by no check in the suite. And the suite's D tier cannot
tell an over-permissive `publishers` write from a correctly restrictive one,
because it never executes one.**

---

# PART 1 — what actually refuses a `publishers.id` move

## 1.0 Method, and why it is not a mutation

`reports/pantry-narrowing-01/probe.sh` stands up the same scratch cluster
`tests/rls.sh` builds (`--serve --empty`), seeds **the fixtures
`tests/rls_checks.sh` seeds, byte for byte**, then alters policies on that
cluster and prints the server's own words.

It reads nothing and writes nothing in the repository. `migrations/` and `tests/`
are never touched, so there is no revert to verify and the file cannot leave the
tree dirty even if it is interrupted. That is why it is not built like
`mutations.sh` is — the "exactly one match, or abort" machinery exists to make a
*mutation* trustworthy, and there is no mutation here.

Every statement is one `psql -c` string, which PostgreSQL runs as a **single
implicit transaction**. That is not a style choice: `pantry.begin_publisher()`
sets the identity with `set_config(…, true)`, which is transaction-local, so a
statement outside the same transaction reads no identity at all and every row set
comes back empty. `tests/rls.sh`'s `run` does the same and its header says why.

```
$ ./reports/pantry-narrowing-01/probe.sh
pg bin:  /opt/homebrew/opt/postgresql@18/bin
cluster: 127.0.0.1:55471 (scratch, --serve --empty, removed on exit)
…
```

The control, run on the untouched tree first, because a finding measured against
a red baseline measures nothing:

```
$ ./tests/rls.sh
checks: 89 run, 89 passed, 0 failed, 0 skipped
skips: 0
ok
```

## 1.1 World 0 — the hypothesis is already dead before anything is altered

The decisive form of the question has never been run: **as a publisher that owns
no rows anywhere else**, so a foreign key cannot be what refuses the statement,
on an **untouched tree**, with **every master policy in place**.

`bravo` owns `bravo-api` and `bravo-hidden`. Moving `bravo`'s id to a uuid
nobody holds cannot collide with `publishers_pkey` and cannot orphan
`services.publisher_id`.

```
WORLD 0 — masters' policies, nothing altered. bravo owns two services, so a
         foreign key CANNOT be what refuses its id move.
    catalog: r  (id = ( SELECT pantry.current_publisher_id() …))  check=- ;;
              w  (id = ( SELECT pantry.current_publisher_id() …))  check=(id = ( … ))
    (as bravo, who owns bravo-api and bravo-hidden)
    id -> a FREE uuid (no PK/FK collision)  -> ERROR:  42501: new row violates
                                               row-level security policy for
                                               table "publishers"
```

**SQLSTATE `42501`. Not `23505`. Not `23503`.** `publishers_pkey` is not the
mechanism, and neither is the foreign key. Whatever is holding `publishers.id` is
a **policy**.

## 1.2 Worlds 1 and 2 — one variable, two different barriers

The single-variable experiment. The `UPDATE` policy is **byte-identical in both
worlds** — `using (true) with check (true)`, every publisher expression on that
policy opened to the literal `true`. The only thing that differs is the
`SELECT` policy.

```
WORLD 1 — SELECT USING (true),   UPDATE USING (true) WITH CHECK (true)
    catalog: r  true  check=- ;; w  true  check=true
    own row, non-key column      -> UPDATE 1
    verified = false             -> UPDATE 1
    id -> a FREE uuid            -> ERROR: 23503: update or delete on table
                                    "publishers" violates foreign key constraint
                                    "services_publisher_id_fkey" on table "services"
    id -> bravo (collides)       -> ERROR: 23505: duplicate key value violates
                                    unique constraint "publishers_pkey"

WORLD 2 — SELECT USING (id = current), UPDATE USING (true) WITH CHECK (true)
    catalog: r  (id = ( … ))  check=- ;; w  true  check=true
    own row, non-key column      -> UPDATE 1
    verified = false             -> UPDATE 1
    id -> a FREE uuid            -> ERROR: 42501: new row violates row-level
                                    security policy for table "publishers"
    id -> bravo (collides)       -> ERROR: 42501: new row violates row-level
                                    security policy for table "publishers"
```

**Same statement, same UPDATE policy, same session identity, same row.** In World
1 the refusal is a **constraint** and there are two of them pointing in two
directions. In World 2 it is a **policy**, and it comes back from a
`FOR SELECT` policy whose `WITH CHECK` does not exist and whose command is not
`UPDATE`.

Both constraint refusals in World 1 are the packet's hypothesis — and they are
real. They are just **downstream**: neither is reachable until the policy barrier
is removed. The PK refuses a *colliding* id; the FK refuses a *free* id that
would orphan a service. **Neither is the barrier; the barrier is in front of
them.**

And `verified = false` is `UPDATE 1` in both worlds, so `with check (true)` does
what it claims on the columns it names. The previous report's `U3` observation
stands and is not in dispute; what was missing is that the *key column* is held
elsewhere.

## 1.3 The barrier is named: `publishers_publisher_select`'s `USING`

Mechanism, from the PostgreSQL manual, `CREATE POLICY` → *Per-Command Policies* →
`SELECT`:

> Using SELECT for a policy means that it will apply to SELECT queries **and
> whenever SELECT permissions are required on the relation the policy is defined
> for**. … queries that require SELECT permissions, such as UPDATE, DELETE, and
> MERGE, **will also only see those records that are allowed by the SELECT
> policy**.

and `Table 300`, the `UPDATE` row, the `SELECT/ALL policy` column:

> **Filter existing row [a] & check new row [a]**

with footnote **[a]**:

> If read access is required to either the existing or new row (for example, a
> WHERE or RETURNING clause that refers to columns from the relation).

and *Application of Multiple Policies*:

> When multiple policies of different command types apply to the same command (for
> example, SELECT and UPDATE policies applied to an UPDATE command) … the
> expressions for one type of policy are combined with the expressions of the
> other type of policy **using the AND operator**.

So `update pantry.publishers set id = … where github_id = 101` is **filtered** by
`publishers_publisher_select`'s `USING` on the old row **and checked by the same
`USING` on the new row**, and — because the two command types combine with `AND` —
it must *also* pass `publishers_publisher_update`. Two policies, one statement,
and the one whose command is `SELECT` is the one that refuses the key move.

### This is measurable from the other end, and here is the measurement

```
WORLD 3 — no SELECT policy at all, UPDATE USING (true) WITH CHECK (true).
    catalog: w  true  check=true
    own row, non-key column      -> UPDATE 0
    id -> a FREE uuid            -> UPDATE 0

      Update on pantry.publishers
        ->  Result
              Output: 'w3'::text, ctid
              One-Time Filter: false
```

`One-Time Filter: false` — the planner folded the scan away because there is no
row the statement is allowed to update. An `UPDATE` policy reading `using (true)`
is **not sufficient on its own**; the statement also reads columns of
`pantry.publishers`, so it needs a SELECT policy, and without one it updates
nothing.

### And the condition is a property of the statement, not only of the schema

Footnote [a] says *if read access is required*. A statement that reads **no
column** of the relation is outside the SELECT policy's reach — and then the
`publishers` update path has **no policy barrier at all** once the UPDATE policy
is opened:

```
WORLD 4 — SELECT USING (id = current), UPDATE USING (true) WITH CHECK (true)
    id -> FREE, WHERE github_id = 101 (reads)   -> ERROR: 42501: new row violates
                                                    row-level security policy …
    id -> FREE, no WHERE at all (reads nothing) -> ERROR: 23505: duplicate key
                                                    value violates unique constraint
                                                    "publishers_pkey"

      the plan for the no-WHERE form — note there is no filter on the scan:
      Update on pantry.publishers
        ->  Seq Scan on pantry.publishers
              Output: '00000000-…-0000000000ff'::uuid, ctid
```

No filter on the scan. The `WITH CHECK (true)` let the rewrite through and the
only thing left was the primary key.

**So the barrier on `publishers.id` is a property of the statement as much as of
the schema.** On master, `publishers_publisher_update`'s own `WITH CHECK` says
only `id = current_publisher_id()` and there is no statement-shaped hole,
because the same clause is also the policy's `USING` and therefore also filters
the old row. The exposure is a property of a *widened* policy, not of master —
and that is exactly why it is a finding about `with check (true)` rather than a
finding about the shipped schema. Stated plainly: **master is sound; the recipe's
`with check (true)` probe is not reaching the interesting statement, and a future
widening that opened both clauses would leave only the constraints.**

## 1.4 What is NOT the barrier — the whole candidate list, measured

Every other candidate the packet named, checked and excluded, on this cluster:

| candidate | measured | verdict |
|---|---|---|
| a trigger on `publishers` | `select … from pg_trigger where tgrelid='pantry.publishers'::regclass and not tgisinternal` → **0 rows** | not a mechanism |
| a rewrite `RULE` on `publishers` | `pg_rules where tablename='publishers'` → **0 rows** | not a mechanism |
| a column-level privilege | `information_schema.column_privileges` on `publishers` names **only `pantry_admin`**, and only because table-level grants are expanded per column | not a mechanism |
| a `RESTRICTIVE` policy | `pg_policy` where `polpermissive = false` → **0 rows**. `pg_policies` does not show restrictive policies, so this was checked in `pg_policy` directly | not a mechanism |
| another `UPDATE` policy for the same command | `pg_policy` on `publishers`, `polcmd='w'`: **only** `publishers_admin_update`, to `pantry_admin` alone | not a mechanism |
| the table grant | `has_table_privilege('pantry_publisher','pantry.publishers','UPDATE')` = **t**, and the UPDATE matched a row in Worlds 1–4 | not a mechanism |
| **`publishers_publisher_select`'s `USING`** | single-variable test, World 1 vs World 2 | **the barrier** |

## 1.5 Which checks assert which barrier — and the answer is: none

The packet's framing was that a check might be passing for a reason nobody wrote
down. Measured, the shape is the other one: **the barrier is real, it is a
`policy`, and nothing asserts it.**

Every check in the suite that touches `pantry.publishers`, exhaustively
(`grep -n publishers tests/rls_checks.sh`):

| check | role | statement | barrier named by the check |
|---|---|---|---|
| `B2` | `pantry` | `select count(*)` | none — `ok`, no expectation |
| `C1`, `C2` | `pantry_public` | `select github_login …` | read predicate |
| `C5` | `pantry_public` | `insert` | **GRANT** — `permission denied for table publishers` |
| `C6` | `pantry_public` | `update … set github_login …` | **GRANT** — same |
| `C7` | `pantry_public` | `delete` | **GRANT** — same |
| `C16` | — | `count(*)` over four tables | none |
| `D2` | `pantry_publisher` | `select github_login …` | **read** — this is the one |
| `D17` | `pantry_publisher` | `insert` | **GRANT** — `permission denied for table publishers` |
| `E2`, `E4` | `pantry_admin` | `select` / `insert` | admin is unconditional |
| `F6` | `pantry` | `insert` | **POLICY** — `row-level security` |
| `G3` | — | catalog invariant | privilege reachability |

**Not one of them is an `UPDATE` by `pantry_publisher`.** The only publisher-role
*write* check on `publishers` is `D17`, and it is an `INSERT` refused at the
grant boundary before any policy runs — which `D17`'s own comment already says:

> It is refused by the GRANT — `pantry_publisher` holds `select, update` on
> `publishers` and nothing else — and the assertion says so rather than naming
> the policy, because the policy is not what refuses it.

So the following are asserted **nowhere**, by any tier:

- that `publishers_publisher_update`'s `USING` filters (no check updates as a
  publisher at all);
- that its `WITH CHECK` refuses a new row (no check executes the statement);
- that the SELECT policy's `USING` filters and checks the new row of an UPDATE —
  **the barrier established in §1.3**;
- that the constraints downstream of all three (`publishers_pkey`,
  `services_publisher_id_fkey`, `publishers_github_id_key`) hold.

**And this is why `B2` was behaviourally indistinguishable from master.** Not
because deleting a `WITH CHECK` changes nothing — the previous report was right
about that, and Postgres's `USING` fallback is the reason — but also because the
check the mutation was written for **does not exist**. The clause is load-bearing
and nothing reads the load. A future mutation of that clause, in the widening
direction, would also be green, and this time green would mean "the boundary is
gone and no check noticed".

## 1.6 The cost of finding #5: a comment in the migration that is false

`migrations/00006_rls.sql:104` reads:

> `with check` on the write is what stops a publisher from moving a row OUT of its
> own account by updating `github_id`/`github_login` — `using` alone would permit
> the update and `with check` is what refuses the new value.

Measured on an untouched tree, with every master policy in place:

```
WORLD 5 — masters' policies, untouched.
    github_login -> 'alpha-renamed' (free)  -> UPDATE 1
    github_id -> 999 (free, unclaimed)      -> UPDATE 1
    github_id -> 202 (bravo's, collides)    -> ERROR: 23505: duplicate key value
                                                 violates unique constraint
                                                 "publishers_github_id_key"
    is_first_party -> true                  -> UPDATE 1
```

Three of the four are **permitted**. The fourth is refused by a **unique
constraint**, not by a policy.

This is not an isolation break and I am not reporting it as one. A publisher
flipping its own row to `is_first_party = true` is a data-integrity oddity — it
does not let it publish a first-party *service*, because
`services_publisher_insert`'s `WITH CHECK` still says `trust = 'third_party'`.
What it does mean is narrower and worth the sentence: **on `publishers`, "no two
publishers may present the same GitHub identity" is a constraint, and "a publisher
may not move out of its own account" holds for `id` alone.** The comment names a
barrier that is not on this table, and it is the comment a future reader would
have trusted. Recorded, not fixed — this packet changed no file under
`migrations/`.

## 1.7 Part 1 tallies

| | |
|---|---|
| control runs, all green | **1** |
| probes on an untouched tree (World 0, World 5) | **2** |
| worlds with a policy altered on the throwaway cluster | **4** |
| distinct SQLSTATEs observed on the same statement | **3** — `42501`, `23505`, `23503` |
| barriers identified and named | **1** — `publishers_publisher_select`'s `USING` |
| candidate mechanisms measured and excluded | **6** |
| checks in the suite asserting that barrier | **0** |
| files written under `migrations/` or `tests/` | **0** |

```
$ git diff --quiet master -- migrations/ tests/  ->  CLEAN
$ git status --porcelain -- migrations/ tests/    ->  empty
$ ./tests/rls.sh
checks: 89 run, 89 passed, 0 failed, 0 skipped
```