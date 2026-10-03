# -pantry-narrowing-01 — the barrier nobody named, and a narrowing mutation

**Branch** `worker/pantry-narrowing-01` · **base** `3ccc679` (the merge of
`worker/pantry-rls-01`) · **recipe**
`reports/pantry-narrowing-01/narrowing.sh` + `reports/pantry-narrowing-01/probe.sh`
· **PostgreSQL** 18.4 (Homebrew) · **suite** `tests/rls.sh`, 89 checks

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
refusal. It is asserted by no check in the suite. And of four narrowing
breakages planted, exactly one went red, on the one check that asserts a
legitimate read succeeds; the other three refused a publisher's legitimate write
and left the suite at 89 of 89 green.**

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
---

# PART 2 — narrowings: can the suite catch a policy that has become too strict?

## 2.0 The recipe, and why it is a sibling

`reports/pantry-narrowing-01/narrowing.sh`, beside the existing recipe rather
than inside it. Three reasons, and the third is the one that decided it:

1. `REPORT-pantry-rls-mutations-01.md` is merged and cited, and it has tallies
   in it — 12 controls, 10 breakages, 106 red rows, `0` `POLICY`. Appending a
   Part 3 to that script would change what those numbers mean, and a reader
   holding the old report would find a script that no longer produces them.
2. The two recipes ask opposite questions and their verdicts are **mirrored**.
   For a widening the strong result is `ABSENT-ALLOWED` / `ABSENT-WIDENED`
   (nothing refused). For a narrowing it is `POLICY` / `ABSENT-NARROWED` (a
   legitimate operation was refused). One table covering both would need a
   "which direction was this" column, and that column is where a mutation recipe
   starts reporting its own expectation back to itself.
3. The exit status means different things. `mutations.sh` exits 0 when the
   *recipe* was sound, and says so in words precisely so "exited 0" is never
   readable as "every breakage was caught". Same here, and the same sentence is
   the last thing the file prints.

The shared machinery — exact-replace-with-an-abort, literal-containment
confirmations, restore-and-verify-three-ways, TSV copied out before the cluster
workdir goes, verdict-from-the-record — is **duplicated deliberately**. ~180
lines that could be sourced out of the other file is cheaper than two recipes
whose verdicts share a variable.

```
$ ./reports/pantry-narrowing-01/narrowing.sh --list
$ ./reports/pantry-narrowing-01/narrowing.sh --phase part1
$ ./reports/pantry-narrowing-01/narrowing.sh --phase control
```

## 2.1 What a red has to be, or the result is not the point

Every breakage must go red on a check that asserts a **legitimate operation
works** — a rowset that must be non-empty, an `ok` that must see `INSERT 0 1`. It
must not go red on a check that asserts a refusal, because **a check that wants a
refusal cannot be falsified by refusing more.** So `observe` reads each red's
**polarity** off what `tests/rls.sh` recorded, and the recipe keeps three tallies
that are never added together:

| outcome | meaning |
|---|---|
| `RED-LEGITIMATE` | ≥ 1 red on a check asserting a legitimate operation works. **This is the result the recipe exists for.** |
| `RED-WRONG-POLARITY` | every red is on a check asserting a refusal. Nothing proved. This is the shape a *widening* breakage produces. |
| `NOT-RED` | nothing went red at all. The packet's most valuable possible result. |

### The polarity is read from the record, and getting it wrong flips the answer

`tests/rls.sh` records three different things and the difference between them is
the whole argument:

| helper | records | polarity |
|---|---|---|
| `rowset` | `exp="wanted: <csv>"` | **non-empty** expectation ⇒ `LEGIT` (`D2`'s `wanted: alpha`). **empty** expectation ⇒ refusal (`D5` asserts an *absence*, which no refusal can falsify). |
| `ok` | `exp="<fragment the output had to contain>"` | `LEGIT` — always an assertion that something happened. |
| `deny` | `exp="<the SQLSTATE it demanded>"`, or `statement SUCCEEDED` / `wrong sqlstate…` / `refused by the wrong barrier…` | **refusal**. |

**The first version of this classifier got it backwards, on the one red that
mattered.** It graded polarity by looking for `insert into` / `update ` /
`delete from` / `select ` inside the expectation. That works for `ok` and `deny`
and fails completely for `rowset`, because `D2`'s expectation is the literal
`wanted: alpha` — no verb anywhere in it. So `N2`'s red, the only genuinely
positive result in the whole recipe, was graded `RED-WRONG-POLARITY` and the
recipe printed "NOTHING PROVED" about it. A classifier that gets polarity
backwards reports a packet's only positive result as a null one, and it did so
until the table above was written down. The fix is the table, not the heuristic.

## 2.2 The witness — because "all green" is also what a no-op mutation prints

A narrowing that does not go red is only a finding about the **suite** if the
narrowing was **real**. "Every check passed" is also what a mutation that changed
nothing at runtime prints, and a `grep`-confirmation of the file does not settle
it.

So every breakage runs a **witness**: the recipe stands up the same cluster
`tests/rls.sh` builds — `./tests/rls.sh --serve --empty`, which applies
`migrations/` **off the disk, with the mutation in place** — seeds the fixtures,
and prints the publisher's own answer. No second mechanism: there is no
`alter policy` here and no hand-written fixture that could drift from the file.
If the file excludes the publisher, the cluster the suite would have built
excludes it.

The baseline is printed once, from the unmutated tree, so every witness is a
diff:

```
== WITNESS BASELINE — master, untouched.
    D2  read its own publisher row    -> alpha
    D7  insert its own service        -> INSERT 0 1
    its own publisher row, UPDATE     -> UPDATE 1
```

**The witness earned its place on its first run, by failing a breakage I had
written to be a finding.** `N2`'s first clause was
`and exists (select 1 from pantry.services where publisher_id = …)`, on the
reasoning that "publishers are accounts, not services, so none of them has a row
in `pantry.services`". That reasoning was about the wrong table: alpha owns
`alpha-api` and `alpha-draft`. The suite said green, the witness printed
`D2 read its own publisher row -> alpha`, and the mutation was a runtime no-op.
**A narrowing has to be measured to be a narrowing.**

## 2.3 The four narrowings

| # | narrowing | file | written for | reds | outcome | witness vs baseline |
|---|---|---|---|---|---|---|
| N1 | `publishers_publisher_select`: `using (…)` → `using (… and verified)` | `00006_rls.sql` | `D2` + every D read | **0** | `NOT-RED` | read `alpha` → read `alpha`; **publisher row UPDATE `UPDATE 1` → `ERROR 42501`** |
| N2 | `publishers_publisher_select`: `… and claimed_at < now() - interval '1 year'` | `00006_rls.sql` | `D2` + every D read | **1** | **`RED-LEGITIMATE` — `D2`** | **read `alpha` → `(no rows)`**; publisher row UPDATE `UPDATE 1` → `UPDATE 0` |
| N3 | `services_publisher_insert`: `with check (… and trust = 'third_party')` → `… and state <> 'published'` | `00006_rls.sql` | `D7`/`D8` | **0** | `NOT-RED` | **`INSERT 0 1` → `ERROR 42501`** on `services` |
| N4 | `publishers_publisher_update`: `using (…)` → `using (… and is_first_party)` | `00006_rls.sql` | nothing, and that is the measurement | **0** | `NOT-RED` | **publisher row UPDATE `UPDATE 1` → `UPDATE 0`** |

### N2 — the one that went red, and it went red on the right check

```diff
   create policy publishers_publisher_select
     on pantry.publishers
     for select
     to pantry_publisher, pantry
-    using (id = (select pantry.current_publisher_id()));
+    using (id = (select pantry.current_publisher_id())
+           and claimed_at < now() - interval '1 year');
```

`claimed_at` is `now()` on every seeded row, so alpha — a verified third-party
publisher with a valid account and two services — reads **nothing**. It reads as
a real anti-abuse rule ("a brand new account cannot read its own row"), which is
the shape of narrowing this packet is about.

```
tests/rls.sh -> 89 run, 88 passed, 1 failed, 0 skipped (exit 1)
ABSENT-NARROWED    LEGIT    D2  publisher isolation: sees its own publisher row and no other
-> RED on a check asserting a LEGITIMATE operation works: D2
-> 1 of 1 reds are in the D tier
-- WITNESS: …
    D2  read its own publisher row  -> (no rows)
```

`ABSENT-NARROWED` — the read returned **fewer** rows than the expectation, which
is the narrowing mirror of the `ABSENT-WIDENED` the last recipe measured. **This
is the suite catching an over-refusal, on the same check that catches a widening,
in the same tier.** One row of one table, and it is a red on a legitimate
operation, not on a refusal.

### N1 — a real narrowing, invisible, and Part 1 explains why

```diff
-    using (id = (select pantry.current_publisher_id()));
+    using (id = (select pantry.current_publisher_id()) and verified);
```

Alpha is `verified = true` in the fixtures, so this satisfies every fixture and
changes nothing they can see: `89 run, 89 passed`. The witness shows it is **not**
a no-op — a publisher whose own row is not verified can no longer *write* to it:

```
    D2  read its own publisher row  -> alpha
    D7  insert its own service      -> INSERT 0 1
    its own publisher row, UPDATE   -> ERROR: 42501: new row violates row-level
                                       security policy for table "publishers"
                                       LOCATION: ExecWithCheckOptions, execMain.c:2340
```

**That 42501 is Part 1's mechanism showing up as a live consequence, from the
other direction.** A publisher's `update publishers set verified = false` writes
a new row on which `verified` is now false — and Part 1 established that the new
row of an `UPDATE` is checked against `publishers_publisher_select`'s `USING`. So
the clause refuses its own write. `42501`, at `ExecWithCheckOptions`: the same
error text, the same source line, arrived at from the widening side in §1.3 and
from the narrowing side here.

And nothing in the suite notices, because **no check ever writes `verified` as a
publisher.** The only two writes of that column anywhere in
`tests/rls_checks.sh` are the fixture seed (lines 50 and 54) and `E4`'s
`pantry_admin` insert (line 413). `C2` and `E2` do *read* an unverified row, so
the fixture is not blind to the value — but `C2` is `pantry_public`'s read, which
`publishers_public_read` governs, and `E2` is `pantry_admin`'s, which no policy
narrows. Neither is the role `N1` acts on, and neither statement can see a
clause added to `publishers_publisher_select`. **The column the narrowing turns
on is invisible to the tier that owns that role, and the fixture's unverified row
is already spent on a different role's check.**

### N3 — the sharpest one: the D tier's write path is not asserting the insert policy

```diff
   with check (publisher_id = (select pantry.current_publisher_id())
-              and trust = 'third_party');
+              and trust = 'third_party'
+              and state <> 'published');
```

`89 run, 89 passed, 0 failed, 0 skipped`. And the witness:

```
    D7  insert its own service  -> ERROR: 42501: new row violates row-level
                                   security policy for table "services"
                                   LOCATION: ExecWithCheckOptions, execMain.c:2340
```

**Every publisher service insert in the fleet is now refused, and the suite is
89 of 89 green.**

This is not a gap in `D7`; `D7` did exactly what it is written to do. `D7` is
an `ok` whose assertion is `INSERT 0 1`, and the fixtures are **all `draft`** —
`D7`'s own row is `('alpha-new', …, 'draft', …)` — so the spurious clause is
satisfied by every insert the suite makes and refused by the workflow the
migration's own comment describes:

> `state` is deliberately NOT constrained. A publisher creating a row as
> `published` is a real workflow question and it is not this migration's to
> settle: MVP-SCOPE's pipeline is draft → submitted → published

**The check that was written for this policy cannot see this policy being
narrowed, because the fixture never makes the write the narrowing refuses.** The
assertion is a statement tag, not a row: `INSERT 0 1` is what a policy that
permits the insert produces, and it is also what a policy that permits *only
drafts* produces. The check has no negative half. `D8` does not help — by then
`alpha-new` does not exist.

The honest statement: **`services_publisher_insert`'s `WITH CHECK` is not
asserted by the suite.** Not its `publisher_id` clause, not its `trust` clause,
not any clause added to it. `D9` and `D10` run *through* that policy and come
out the other side refused — they are evidence that the policy is **present**,
never that its predicate is the **right** predicate, because a policy of
`using (false)` would satisfy both of them too. That is precisely the vacuous
shape `tests/rls_checks.sh`'s own header says `deny` exists to prevent, reached
by the opposite route.

### N4 — the whole `publishers` write path, and no check at all

```diff
   create policy publishers_publisher_update
     on pantry.publishers
     for update
     to pantry_publisher, pantry
-    using (id = (select pantry.current_publisher_id()))
+    using (id = (select pantry.current_publisher_id()) and is_first_party)
     with check (id = (select pantry.current_publisher_id()));
```

"Only a first-party account may change a publisher row" reads as a sensible
tightening. It means **no third-party publisher in the fleet can update anything
about itself**, ever — including `verified`, which is the column the review path
would want a publisher to be able to track.

```
89 run, 89 passed, 0 failed, 0 skipped
-- WITNESS: …
    D2  read its own publisher row  -> alpha
    D7  insert its own service      -> INSERT 0 1
    its own publisher row, UPDATE   -> UPDATE 0
```

`UPDATE 1` → `UPDATE 0`. Silent, no error, the row simply stops being updatable.
`89 of 89` green.

This is §1.5's finding turned into a measurement: **no check in the suite
executes an `UPDATE` on `publishers` as `pantry_publisher`,** so nothing can
notice. It is the narrowing a reader is most likely to believe is covered,
because `publishers` looks like the most protected table in the schema.

## 2.4 Where D has no check at all

The packet asked for a narrowing on a table where `D` has no check, if one
exists. Two candidates, and the answer differs by command:

| table | D's read checks | D's write checks | a policy to narrow? |
|---|---|---|---|
| `publishers` | `D2` | **none** | yes — **N1**, **N4** above |
| `services` | `D1`, `D5` | `D7`–`D12` | yes — **N3** above |
| `service_versions` | `D3` | `D15`, `D16` (both at the **grant** boundary) | `service_versions_publisher_insert` exists; a narrowing there would hit the same write-path gap |
| `service_compat` | `D4` | **none** | **no publisher write policy exists at all**, so there is nothing to narrow |

`service_compat` is the table with genuinely no publisher write surface, and it
is therefore the one place where "narrow the policy" is not available — the
policy is the thing that is missing, not the check.

## 2.5 Part 2 tallies, kept apart from the passes

Straight from the recipe's own output.

```
controls run (every one green):        6
breakages planted:                     4
  of which the plant FAILED:           0
breakages reverted byte-for-byte:      4
  of which the restore FAILED:         0
breakages red on a LEGITIMATE op:      1
breakages red only on refusal checks:  0
breakages NOT RED AT ALL:              3
recipe problems (structural):          0
```

| | planted | `RED-LEGITIMATE` | `NOT-RED` |
|---|---|---|---|
| **Part 2** (narrowings) | **4** | **1** — `N2`, on `D2` | **3** — `N1`, `N3`, `N4` |

**If this packet is summarised in one number, the number is "1 of 4".** Not "4 of
4". One narrowing of four was caught by the suite, and it was the one that
refuses a *read*. Three were not caught, and all three refuse a **write**.

Read together with Part 1, the shape of the whole packet is one sentence:
**the D tier asserts what a publisher may *see*, and almost nothing about what a
publisher may *do*.** Every not-red above is a write. The one red is a read.

### Restore, and the proof

```
$ git diff --quiet master -- migrations/ tests/  ->  CLEAN (no differences from master)
$ git diff --quiet -- migrations/ tests/         ->  CLEAN (nothing unstaged)
$ git status --porcelain -- migrations/ tests/   ->  empty (nothing untracked)
$ ./tests/rls.sh
checks: 89 run, 89 passed, 0 failed, 0 skipped
```

Six controls across the run — one before, one after each of the four reverts,
one at the end — every one green, 0 skipped. `migrations/` and `tests/` are
byte-identical to master; `tests/` is never written at all, because every
mutation here is in `migrations/`.

## 2.6 Three bugs this recipe found in itself

Recorded because a mutation recipe that hides its own failures is the thing this
packet is arguing against.

1. **`kill $!` under `timeout` leaked a scratch cluster per breakage.** The
   witness ran `timeout 300 ./tests/rls.sh --serve --empty &`, so `$!` was
   `timeout`'s PID; killing it left the harness running, its `trap 'exit 0'
   INT TERM` never fired, and postmaster + socket + data directory survived all
   four breakages. Found by looking for leftover processes, not by reading the
   code. The harness is now run directly so `$!` *is* the process that cleans up,
   and `w_stop` additionally stops the cluster by its own data directory and
   removes it, so "no cluster was left behind" holds even if the harness is
   wedged. `probe.sh` had the same bug and the same fix.
2. **The polarity classifier, §2.1.** It graded the packet's only positive red
   as `NOTHING PROVED`.
3. **`grep -c` on a needle containing a newline matched 711 lines** and aborted
   `N3` instead of confirming it — and the two-line needle it was replaced with
   was ambiguous, because `services_publisher_update`'s `with check` ends with
   exactly those two lines, so a negative confirmation found one occurrence
   whether or not the mutation had applied. **A confirmation that confirms
   nothing is a comment.** Both are now literal-containment checks against the
   whole policy block.

One more incident, recorded because it nearly cost the run: a `N4-restored` suite
run aborted with `tests/rls.sh printed no 'checks:' line` — its server received a
smart shutdown ~90 ms after becoming ready, from outside the recipe, and the
apply log's only line was `connection refused`. The recipe did what it is
written to do: it counted the run as a structural problem, refused to report it
as a result, and the tree was still `CLEAN`. A second run of the identical
recipe exited 0 with every control green. **A recipe that aborts on the
unexpected and then produces the same numbers on the next run is the recipe
working; one that had quietly counted the bad run is the recipe failing.**

---

# PART 3 — the two smaller items the last report raised

## 3.1 `D`'s header should say what the tier is *not* about

Agreed, and it is the right fix, but **this packet does not apply it**, because
the packet's own standard is that `tests/` ends byte-identical to master. The
change below is written out verbatim so it is a copy-pasteable diff and not a
suggestion.

Today `tests/rls_checks.sh:298-301` is a banner and one line:

```sh
# ===========================================================================
say ""
say "-- pantry_publisher: its own rows, and only its own rows"
# ===========================================================================
```

Proposed, immediately after the `say` line:

```sh
-- WHAT THIS TIER IS NOT ABOUT, because the name implies otherwise and the
-- implication is wrong. `pantry_publisher` is not the owner of any of these
-- tables, so nothing here can observe FORCE ROW LEVEL SECURITY: FORCE is about
-- the OWNER's exemption, and a role that never had the exemption cannot notice
-- it being withdrawn. `B4` in reports/pantry-rls-mutations-01/mutations.sh
-- removed FORCE from pantry.services and produced nine reds — F1, F2, F3, F5,
-- F7, F8, F9, I4, J5 — and ZERO in this tier. FORCE is held down by F, and it
-- is not fixable here and should not be: this tier's role is the wrong role for
-- the question. Read F before citing this tier as evidence about FORCE.
--
-- And this tier does not execute an UPDATE on pantry.publishers at all, so it
-- says nothing about the publishers write path — see
-- -pantry-narrowing-01.md §1.5 and §2.3 (N4).
```

Why it has to be a line in the header rather than a note elsewhere: the reader
who needs it is the one who has *just* seen `D2` pass and is about to write "the
publisher isolation tier covers this". A note in a report is read by the person
who already suspects; a line in the header is read by the person who does not.

`00006_rls.sql` is already honest about this on its own subject — it writes
"`00007` asserts it by RUNNING a denial as the owner", and the DELETE-is-refused
comment says "`00007` asserts it" — and both point at the **F** tier, not D. The
tier that *should* be self-describing is the one whose name is load-bearing.

## 3.2 Is `publishers` publisher isolation resting on one check a gap, or an
accurate description of the table?

**Both, and the split is exact — and it is not the split the question assumes.**

The question assumes one check is doing two jobs. It is doing one job, and there
is a *second* job with **zero** checks on it.

| the boundary | checks that assert it | verdict |
|---|---|---|
| a publisher **reads** only its own `publishers` row | `D2` — one | **accurate.** `publishers` is one row per tenant and one predicate: no join, no state machine, no second path. `B1` widened that predicate and exactly one check noticed, which for a single-predicate single-row table is the right ratio, not a thin one. |
| a publisher **writes** nothing outside its own row | **none** | **a gap, and it is not new.** No check executes an `UPDATE` on `publishers` as `pantry_publisher`; `D17` is an `INSERT` refused at the grant, and its own comment says the policy is not what refuses it. |

So the table is accurately described by "one check" and the suite is not, because
the suite never claimed to cover the write path — it simply never noticed that it
was not covering it. `N4` is what that costs: a publisher unable to update
anything about itself, 89 of 89 green.

There is a third thing worth naming, and it is the reason the write path is not
merely under-tested but **un-testable as written**. Part 1 found the barrier on
`publishers.id` is `publishers_publisher_select`'s `USING`, applied by PostgreSQL
to the `UPDATE` as a filter on the old row and a check on the new one. So the
write boundary on `publishers` is *carried by the SELECT policy*, and the tier
named for reads is the only tier that can see it. **A check added to `D` for the
write path would have to narrow `publishers_publisher_update` and would prove
almost nothing** — `N4` is the demonstration: narrowing it changed the world and
moved no check. A check that would actually assert the write boundary has to
execute the statement that `B2` and `N4` both reach for:

```sh
deny "D18 publisher write: may NOT move its own publisher row to another id" \
  "$SHAPE1" "42501" "row-level security" \
  "$(as_publisher "$A1" "update pantry.publishers set id = '00000000-0000-4000-8000-0000000000a2' where id = '$A1';")"
```

That assertion is worth **two** things and neither is "it caught `N4`". First,
`42501` + `row-level security` is the *policy* verdict, which is the one verdict
across both recipes that has never once been earned by a breakage — 106 red rows
in `REPORT-pantry-rls-mutations-01.md` and 0 `POLICY`, and this would be the
first. Second, it pins the barrier Part 1 named, in the suite, against a
*regression* in the SELECT policy rather than only against a widening of the
UPDATE one. Proposed, not written: this packet changed no check.
