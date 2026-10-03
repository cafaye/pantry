# REPORT-pantry-rls-mutations-01 — can pantry's RLS suite be made to fail by breaking the policies it is about?

**Branch** `worker/pantry-rls-01` · **base** `b7bf2a5` · **recipe**
`reports/pantry-rls-mutations-01/mutations.sh` · **PostgreSQL** 18.4 (Homebrew)

---

## 0. The claim under test

`REPORT-registry-pantry-schema-02.md` §4 recorded four mutations against
`tests/rls.sh` and reported "4 of 4 red". Three of the four removed a **grant** —
a schema `USAGE`, a `revoke`, a table grant — and the fourth flipped an expected
literal as a control. **Not one of them removed, narrowed or widened a policy.**

That is the defect this packet exists for. The policies are the isolation
question. `D2`, `D5`, `D10`, `D12` and the publisher tier are what "one publisher
cannot read another's rows" is made of, and none of them was ever broken. So
"4 of 4 red" is true, is about grants, and is silent about the thing a reader
would take it to mean. Six policy breakages are what was missing.

**What was proven, in one line:** three of the six went red in the publisher
isolation tier, one went red outside it for a structural reason worth acting on,
one was red on exactly the check it was written for, and **one did not go red at
all** — for a reason that is Postgres's, not the suite's, and is now measured
rather than argued.

---

## 1. First: the packet's check count is stale, and here is the measured one

The packet says the suite is 84 checks. It is not.

```
$ ./tests/rls.sh
checks: 89 run, 89 passed, 0 failed, 0 skipped
```

**89 = 88 distinct `# CHECK` lines + `A0`**, which runs once per ownership shape
and shares one name. `bin/prime`'s own header says "89 checks", so this is
corroborated outside this packet.

The delta is exactly 5, and it has a name:

| ref | named `# CHECK` lines | suite reports | delta |
|---|---|---|---|
| `817c213` (before `registry-migdown-04`) | 83 | 84 | — |
| `2fd259c` `registry-migdown-04` | +5 (`J1`–`J5`) | 89 | **+5** |

`git log --oneline -1 -S'# CHECK J1' -- tests/rls_checks.sh` →
`2fd259c`, which landed on master in the merge this branch is cut from. So the
packet's 84 was correct when written and is 5 stale. **Every number below is
against 89**, and §5 re-runs the recorded mutations against 89 for the same
reason: their numbers are about a suite that no longer exists.

---

## 2. Method, and the four verdicts

The recipe is `reports/pantry-rls-mutations-01/mutations.sh`. Per breakage:

```
exact string replacement that REFUSES anything but exactly one match
  → grep-confirm the mutated bytes in the file the suite is about to read
  → ./tests/rls.sh --keep
  → restore from master, verify byte-identical three ways
  → ./tests/rls.sh again, and require green
```

It is a shell script because `tests/rls.sh` is one. That suite stands up its own
PostgreSQL from binaries, needs no container, and reads no
`PANTRY_DATABASE_URL`. `tests/` is not a build directory any more, and this
recipe lives in `reports/`, so it is inert to every tier by construction. **No
kit adapter and no Go test tier were added** — a third instrument against a
boundary this suite already measures against a real database is a second number,
not a stronger one.

### The verdict taxonomy — why two barriers is not enough

`tests/rls_checks.sh`'s `deny` helper requires the SQLSTATE *and* the message
precisely so a suite cannot pass with every policy `using (false)` and every
grant missing. That distinction is the packet's whole question, so the recipe
refuses to collapse it — and it needs **five** verdicts, not the two the packet
named:

| verdict | means | count over the whole run |
|---|---|---|
| `GRANT` | `permission denied for table` / `for schema`. Postgres refused at `aclcheck` time, **before RLS was consulted**. Says nothing about policies. | 45 |
| `POLICY` | `violates row-level security policy`. **The policy refused.** This is the barrier `D9`/`D10`/`D11`/`D16`/`F5`/`F6`/`F9` name, and the only verdict that is evidence about a predicate. | **0** |
| `CONSTRAINT` | A unique / FK / check constraint refused. Neither barrier. | 2 |
| `ABSENT-ALLOWED` | A statement the check required to be **refused was permitted**. | 5 |
| `ABSENT-WIDENED` / `ABSENT-NARROWED` / `ABSENT-OBSERVATION` | No barrier refused; a read returned a row set that grew, shrank, or a different shape than the expectation. | 54 |
| `CATALOG` | A last-resort fallback for a check that asserts a `pg_roles` / `pg_class` fact rather than executing against the boundary. **It never fired** — `C15` and `C13`/`C15`'s siblings record their failure as `missing: <fragment>` first, which is caught above. Kept because removing it would mean a future catalog check classifies as `OTHER`. | 0 |

45 + 2 + 5 + 54 + 0 = **106 red rows across 10 breakages.**

The verdict is derived from **what the runner recorded** — its expectation and
the server's own words in the results TSV — never from the check's name and never
from what the mutation was trying to do. A recipe that decided in advance which
barrier a breakage "should" hit would be reporting its own expectation back to
itself, which is the failure mode this packet is about.

### The `ABSENT` family is the strongest outcome, not a weak one

For a **widening** mutation, the correct result is that nothing refused. There is
no barrier to name, because there is no longer a boundary at that point. Printing
`ABSENT-WIDENED` as "the policy refused" would be a lie in the exact direction
the packet is warning about, and a reader who believes it is exactly the reader
this packet is written for. **Across all 10 breakages and 106 red rows, zero were
`POLICY`** — see §6 for why that is the expected shape and what it does and does
not mean.

---

## 3. The control

Run first, on an untouched tree, before anything is planted. If it is not green
the recipe stops and exits non-zero having broken nothing.

```
$ ./tests/rls.sh
postgres: postgres (PostgreSQL) 18.4 (Homebrew)
checks: 89 run, 89 passed, 0 failed, 0 skipped
ok
```

**Green, exit 0, 0 skipped.** Run again at the end, after the tenth revert, and
green again — **12 controls across the full run, every one green, 0 skipped.**

---

## 4. Part 1 — the six policy breakages

Each row names the check the breakage was **written for**. "Red" is not a result;
"red, and the check that went red is the one that asks whether another
publisher's service is invisible by primary key" is a result.

| # | breakage | file | written for | reds | D-tier reds | the D-tier check that went red |
|---|---|---|---|---|---|---|
| B1 | `publishers_publisher_select`: `using` → `true` | `00006_rls.sql` | `D2` | 1 | 1 | **`D2`** |
| B2 | `publishers_publisher_update`: `with check` deleted | `00006_rls.sql` | update path | **0** | 0 | — |
| B3 | `current_publisher_id()` stubbed to a constant | `00005_functions.sql` | `D6` | 6 | 1 | **`D6`** |
| B4 | `force row level security` removed from `services` | `00006_rls.sql` | FORCE | 9 | **0** | — |
| B5 | `noinherit` dropped from `pantry_publisher` | `00001_roles.sql` | `C15` | 1 | **0** | — |
| B6 | `D1`'s expected literal flipped **[control]** | `rls_checks.sh` | `D1` | 1 | 1 | **`D1`** |

### B1 — the read predicate dropped. The most important one, and it worked.

```diff
  create policy publishers_publisher_select
    on pantry.publishers
    for select
    to pantry_publisher, pantry
-   using (id = (select pantry.current_publisher_id()));
+   using (true);
```

**1 red: `D2`, and only `D2`.**

```
tests/rls.sh -> 89 run, 88 passed, 1 failed, 0 skipped (exit 1)
ABSENT-WIDENED  D2  publisher isolation: sees its own publisher row and no other
```

`D2` asserts `select github_login from pantry.publishers` as `alpha` returns
exactly `alpha`. With the predicate gone it returned `alpha,bravo,cafaye,
unverified` — the whole table. **Barrier: none. Nothing refused; the read was
widened.** That is the verdict the packet's question turns on: `D2` is a `rowset`
check, and a widening cannot produce a refusal, it produces an extra row.

**What the single red means.** `publishers` is the one table whose publisher
isolation rests on a *single* check. Everything else about this policy set is
covered by the `services`/`service_versions`/`service_compat` tiers, but on
`publishers` the entire read boundary is `D2` and nothing else. Widen the
predicate and exactly one check notices. That is thin, and it is now measured
rather than assumed.

### B2 — the `with check` deleted. It did not go red, and Postgres is why.

```diff
  create policy publishers_publisher_update
    on pantry.publishers
    for update
    to pantry_publisher, pantry
    using (id = (select pantry.current_publisher_id()))
-   with check (id = (select pantry.current_publisher_id()));
+   using (id = (select pantry.current_publisher_id()));
```

**0 reds. Not one check in the suite.**

The packet predicted "the update path opens even though the read predicate
survives". It does not, and the reason is Postgres's documented behaviour: for an
`UPDATE` policy with no `WITH CHECK`, the `USING` expression is used for the
new-row check as well. So the boundary did not move, and a correct suite is
green. **Measured, not asserted:**

```
$ ./tests/rls.sh --serve --empty          # mutated file applied
  select policyname || '  using=' || qual || '  check=' || with_check
    from pg_policies where policyname='publishers_publisher_update';
  publishers_publisher_update  using=(id = (SELECT pantry.current_publisher_id()))  check=<NONE>

  -- U1: my own row, non-key column
  UPDATE 1
  -- U2: move my own row to a free publisher id
  ERROR:  42501: new row violates row-level security policy for table "publishers"
```

`check=<NONE>` proves the clause is genuinely gone from the catalog, and `U2` is
still refused by a *new-row* check — with no `WITH CHECK` in the catalog, the only
thing that can be doing that check is the `USING` fallback. **B2 as specified is
behaviourally indistinguishable from master**, which is exactly why zero checks
moved.

**A second, independent reason it could not have gone red** — found while
confirming the first, and reported because it is the more interesting fact.
Replacing the clause with `with check (true)` — the version that genuinely widens —
does **not** open the key-column path either:

```
publishers_publisher_update  using=(id = (SELECT current_publisher_id()))  check=true
  U3: update my own row set verified = false      →  UPDATE 1     (permitted, as intended)
  U2: update my own row set id = <free id>        →  ERROR 42501 new row violates row-level security policy
```

So `with check (true)` widens the *column* boundary (U3 now allowed) but the
*identity* boundary on `publishers.id` is held by something that is not this
`WITH CHECK`. I did not identify the mechanism and I am **not** claiming it as a
result. What I can state is measured: **`publishers`' update path cannot be
widened by editing this policy's `with check` alone.** Worth a separate look;
worth being careful about, because it means a `publishers` UPDATE check that
asserts a refusal has a second barrier behind it that nobody named.

### B3 — `current_publisher_id()` stubbed. The nastiest class, and it reached the public read path.

```diff
-   raw := current_setting('pantry.publisher_id', true);
+   raw := '00000000-0000-4000-8000-0000000000a1';   -- fixture alpha
```

Every policy is untouched, syntactically valid, and still reads
`id = (select pantry.current_publisher_id())`. **6 reds:**

| check | verdict | what it saw |
|---|---|---|
| **`D6`** | `ABSENT-OBSERVATION` | with no identity a publisher read **2** rows instead of 0 |
| `F2` | `ABSENT-OBSERVATION` | the owner, with no identity, read **2** instead of 0 |
| `I4` | `ABSENT-OBSERVATION` | `set session authorization pantry` read the catalog instead of nothing |
| `C1` | `ABSENT-WIDENED` | `pantry_public`'s view gained `alpha-draft` |
| `C3` | `ABSENT-WIDENED` | `pantry_public` read a draft |
| `C4` | `ABSENT-WIDENED` | the graph gained the edge into that draft |

**Written for `D6`, and `D6` is the D-tier check that went red.** Barrier: none —
nothing refused anywhere; three reads returned rows they must not and two
returned the wrong count.

**The part worth flagging: it reached the *public* read path, not just the
publisher's.** `service_is_visible` compares `publisher_id` against the same
function, so a stubbed identity also satisfies its third clause for `alpha`'s
rows in any state — and `alpha-draft` became publicly visible. A single-line
change in a helper function, with `pg_policies` byte-for-byte unchanged,
published a draft. The C tier caught that; `D6` caught the publisher half.

### B4 — `force row level security` removed. Nine reds, and **none** in the D tier.

```diff
- alter table pantry.services force  row level security;
```

**9 reds: `F1`, `F2`, `F3`, `F5`, `F7`, `F8`, `F9`, `I4`, `J5`. Zero in the D
tier.** Two of them are the strongest kind of failure in the whole packet:

```
ABSENT-ALLOWED  F5  service role: pantry may NOT insert a FIRST-PARTY service
ABSENT-ALLOWED  F9  service role: pantry may NOT set trust = first_party either
```

Both are `deny` checks asserting `row-level security`, and with `FORCE` gone
**the statements were permitted**. Not refused by the wrong barrier — not
refused at all. `F7` compounds it: the owner's `DELETE` of its own service, which
`F8` then confirms really happened, so the row is gone.

**The packet predicted this would be green or nearly so, and it was green in the
tier it went looking in. This is a finding about the suite, and it is
structural, not accidental:** `FORCE` is about the **owner's** exemption, and
`pantry_publisher` is not the owner. **Nothing in the publisher-isolation tier can
see `FORCE` come off, ever** — not because a check is missing, but because the D
tier's role is not subject to the exemption in the first place. Nine checks catch
it, in F, I and J. So this is not an uncovered property; it is a property covered
somewhere other than where one would look, and **a reader citing the D tier as
pantry's isolation evidence is citing a tier structurally blind to `FORCE`.**

### B5 — `noinherit` dropped. One red, and it is a catalog read.

**B5 produced exactly one red: `C15`, "every pantry role is NOINHERIT".** And `C15`
notices by *reading `pg_roles.rolinherit`* — it does not execute a statement
against the boundary at all. Verdict `ABSENT-OBSERVATION`: the count went 0 → 1.

**The first thing B5 found is that `noinherit` is set twice in
`00001_roles.sql`**, and a single-site mutation of it is inert:

```sql
    create role pantry_publisher nologin noinherit;   -- site 1, inside do $$ $$
…
alter role pantry_publisher noinherit;                -- site 2, the unconditional re-assert
```

- Remove only site 2 → **inert.** The role does not exist yet on a fresh cluster,
  so it is *created* noinherit by site 1.
- Remove only site 1 → **inert.** The re-assert four lines later puts it back.

Both had to go, and both are grep-confirmed in the recipe. **Either single-site
mutation of this fact is a green that reads as a pass** — which is the sharpest
available argument for the recipe's "exactly one match, or abort" rule.

**The coverage finding, stated plainly: no check demonstrates that inheritance
actually widens what `pantry_publisher` can do, and none can.** The suite
connects as a superuser and takes roles with `set role`, so the `INHERIT` bit is
never on the path — exactly the failure `C13` documents for its own first attempt
at an escalation check. `C15` asserts an attribute; it does not assert a
consequence. Dropping `NOINHERIT` is caught by a catalog fact and by nothing
else.

### B6 — the control. One red, named.

```diff
- "alpha-api,alpha-draft"
+ "alpha-api,alpha-draft,bravo-api"
```

**1 red: `D1`, and only `D1`.** Verdict `ABSENT-NARROWED` — the observation is
smaller than the expectation, because the control makes the expectation larger
than reality on purpose.

This is what proves `B1`–`B5` were not red for some ambient reason. If the
harness were red in general, `B6` would come out red too and the other five would
mean nothing. It also proves this recipe's replacement machinery can turn a check
red **at all**, which is the half of a mutation test that is easiest to assume.

**On mutating a check.** The rule is "never mutate pantry's checks to make a
breakage red". `B6` does the opposite — it breaks a currently-green check on
purpose and restores it. Nothing is added, nothing weakened, `B6` is reverted from
master, and the tree is verified byte-identical afterwards. The same applies to
`M4`, which is the same control the earlier recipe used.

---

## 5. Part 2 — the four recorded mutations, against 89

Re-run verbatim in intent from `REPORT-registry-pantry-schema-02.md` §4.

| # | mutation | grep confirmed | recorded (of 77) | measured now (of 89) | still red |
|---|---|---|---|---|---|
| M1 | drop `, pantry` from the schema USAGE grant | ×1 | 34 | **36** | yes |
| M2 | re-add `revoke all on schema pantry from pantry_public` | ×1 | 18 | **20** | yes |
| M3 | delete the `pantry_admin` table grant | ×0 remaining | 29 | **31** | yes |
| M4 | flip `C1`'s expected row set **[control]** | ×1 | 1 | **1** — `C1` | yes |

The suite grew by 12 named checks since that report — `H5` and `I1`–`I7` from
`reg-roles-02`'s `00007`, `J1`–`J5` from `registry-migdown-04` — which is the
whole of the `+2 / +2 / +2 / 0`. So the earlier report's numbers were not wrong,
they were about a suite that no longer exists.

### The finding is not the numbers. It is where the D tier went.

`M1` and `M3` each make **eight** D-tier checks red. Eight.

```
M1  D1=ABSENT-NARROWED  D2=ABSENT-NARROWED  D3=ABSENT-NARROWED  D4=ABSENT-NARROWED
    D7=GRANT  D8=ABSENT-OBSERVATION  D11=ABSENT-ALLOWED  D14=ABSENT-OBSERVATION
M3  D1=ABSENT-NARROWED  D2=ABSENT-NARROWED  D3=ABSENT-NARROWED  D4=ABSENT-NARROWED
    D7=CONSTRAINT  D8=ABSENT-OBSERVATION  D11=ABSENT-ALLOWED  D14=GRANT
```

**Not one of those sixteen is a policy refusing anything.** `D1`–`D4` narrowed to
nothing because the role could no longer reach the table at all. `D11` was
*permitted* because the grant was gone and the policy never ran. `D14` read an
empty table. `D7`/`F4` failed on `violates foreign key constraint`, because
deleting the admin grant stops `A0` and **the fixtures never land at all** — the
same mechanism the earlier report described in §1.

**So a reader who sees "D2 red under M1" would conclude `D2` tests the grant. It
does not.** `D2` tests the predicate. Under a *grant* mutation it goes red for
want of reachability, which is a different reason entirely — and the whole
question this packet asks is which reason. That is why the recipe refuses to
accept "the D tier went red" as a result on its own.

`M2` is the mirror image and the cleanest illustration: 20 reds, **16 of them
`GRANT`**, and **zero in the D tier**, because removing `pantry_public`'s schema
`USAGE` locks the *public* role out and leaves publisher isolation untouched. A
mutation that is 80% grant-barrier reds produces no publisher-isolation red at
all. That is the packet's thesis in one line.

---

## 6. Part 3 — which barrier each breakage hit

Mandatory, and every breakage in Part 1 is named.

| # | barrier | why |
|---|---|---|
| **B1** | **NONE — nothing refused.** `ABSENT-WIDENED` on `D2`. | A widening cannot produce a refusal. The publisher read all four rows. **Proof about the predicate; no grant involved.** |
| **B2** | **NONE, and none was reachable.** 0 reds. | The clause was genuinely absent (`check=<NONE>`) and the new-row check still refused, so the `USING` fallback held the line. Barrier evidence of a sort — the *policy* refused, via `USING` — but no check in the suite executes that statement, so nothing observed it. |
| **B3** | **NONE — nothing refused.** `ABSENT-OBSERVATION` ×3, `ABSENT-WIDENED` ×3. | Three reads returned rows they must not. **The strongest possible outcome for a widening**, and it includes the public read path. |
| **B4** | **NONE — nothing refused.** `ABSENT-ALLOWED` ×2 (`F5`, `F9`). | The two `deny` checks that assert `row-level security` had their statements **permitted**. `F8` confirms `F7`'s delete really removed the row. The rest are catalog and observation. |
| **B5** | **NONE.** `C15` `ABSENT-OBSERVATION`. | A `pg_roles.rolinherit` count went 0 → 1. No statement was executed against the boundary. |
| **B6** | **NONE — by construction.** `ABSENT-NARROWED`. | The control breaks an expectation, not a boundary. Its job is to prove the other five were not red ambiently. |

**And the number that should stop a reader: `POLICY` verdicts, across all ten
breakages and all 106 red rows — `0`.**

Not one breakage in this packet was caught by a check noticing that a *policy*
refused. The reason is structural and worth stating rather than leaving as a
coincidence: **every policy mutation here is a widening**, so its reds are all of
the "nothing refused" kind. A widening and a refusal are opposites.

Two consequences, and the second is the limitation of this packet:

1. It is **correct and expected** that a widening cannot falsify a check that
   asserts a refusal — it can only falsify a check that asserts a *narrowing*.
   `B1`, `B3`, `B5` all behaved that way. `B4` is the exception that shows the
   mechanism: it falsified `F5` and `F9`, which do assert refusals, by removing
   the barrier altogether.
2. **This packet contains no narrowing mutation.** Every policy breakage here
   widens. So the opposite failure — a policy that refuses *too much*, breaking a
   legitimate publisher — is untested by this packet, and `B2` is the closest
   thing to a probe of it and turned out to be a no-op. That is a real gap and it
   is stated here rather than left to be discovered.

---

## 7. Tallies, kept apart from the passes

Straight from the recipe's own output. Planted breakages and observed reds are
different numbers and are never added together; so are controls and passes.

```
controls run (every one green):        12
breakages planted:                     10
  of which the plant FAILED:           0
breakages reverted byte-for-byte:      10
  of which the restore FAILED:         0
breakages with >= 1 D-tier red:        5
breakages with 0  D-tier red:          5
recipe problems (structural):          0
```

Split by part, because pooling them hides the shape:

| | planted | ≥1 D-tier red | 0 D-tier red | not red at all |
|---|---|---|---|---|
| **Part 1** (policy breakages) | 6 | 3 — `B1`, `B3`, `B6` | 3 — `B2`, `B4`, `B5` | **1 — `B2`** |
| **Part 2** (recorded mutations) | 4 | 2 — `M1`, `M3` | 2 — `M2`, `M4` | 0 |

**If this packet is summarised in one number, the number is "3 of 6".** Not "6 of
6", not "10 of 10". Three of the six policy breakages produced a
publisher-isolation red.

---

## 8. Restore, and the proof

Every breakage reverted from master, `tests/rls.sh` re-run, and green required —
twelve times, all green. At the end of the full run:

```
$ git diff --quiet master -- migrations/ tests/   ->  CLEAN (no differences from master)
$ git diff --quiet -- migrations/ tests/          ->  CLEAN (nothing unstaged)
$ git status --porcelain -- migrations/ tests/    ->  empty (nothing untracked)
$ ./tests/rls.sh
checks: 89 run, 89 passed, 0 failed, 0 skipped
```

**`migrations/` and `tests/` are byte-identical to master.** `migrations/` is not
modified by the committed tree at all; `tests/` is touched only by `B6` and `M4`,
which are the two controls, and both are reverted.

### Two bugs in this recipe, both found by its own output

Recorded because a mutation recipe that hides its own failures is the thing this
packet is arguing against.

1. **`run_suite` deleted the cluster workdir before `classify` read the results
   TSV out of it.** An empty read classified as "nothing failed". `B3` and `B4`
   each had real reds and the first run printed `NOT RED AT ALL` for both. The TSV
   is now copied out before the workdir goes, and a run that keeps no TSV is an
   `ABORT`, not an empty classification.
2. **`grep -c` counts lines, so every multi-line confirmation matched nothing**
   and `B1`/`B2` aborted on their own confirmation rather than planting anything.
   The abort was the right outcome — a mutation that cannot confirm itself should
   not run — but two of six not running is not a result either. Multi-line
   mutations now get a literal-containment check against the file read fresh from
   disk.

Also worth recording, because it cost the most time and the lesson is general:
the `B2` investigation went through four wrong turns before landing — `psql -q`
suppressing the command tags (the mistake `tests/rls.sh`'s own header warns
about), an apostrophe in an `\echo`, `begin_publisher`'s transaction-local GUC
expiring in autocommit, and two order-of-operations bugs in the throwaway
harness. **Every one of them produced a plausible wrong answer rather than an
error.** That is the argument for the recipe's abort-on-anything-unexpected
posture, written down.

---

## 9. What I would do next, in the order I would do it

These are proposals. This packet changed no check and weakened no assertion.

1. **`noinherit` is asserted, never demonstrated** (`B5`). One behavioural check —
   a member of `pantry_publisher` with `rolinherit = true` reading rows it cannot
   read as itself — would make `C15` mean something. This is the cheapest real gap
   here.
2. **The D tier cannot see `FORCE` come off** (`B4`). Not fixable inside D, and it
   should not be: the honest fix is a line in `D`'s header saying what the tier is
   *not* about, so the next reader does not infer it from its name.
3. **`publishers` publisher isolation is one check** (`B1`). `D2` alone holds the
   read boundary on that table.
4. **The key-column refusal on `publishers` has an unnamed second barrier** (§4,
   `B2`). `with check (true)` permitted `verified = false` but not `id = …`.
   Measured, mechanism unidentified, and worth naming before somebody relies on it.
5. **No narrowing mutation exists** (§6). The policy suite can be widened
   indefinitely; nothing here can catch a policy that has become too strict.
6. **The count drift** (§1). Whatever quotes a check count will be quoting a
   number that changes under it. `tests/rls.sh --list` exists; the number in prose
   should stop being a number.

---

## 10. Reproduce

```sh
cd /Users/kaka/Code/any/moon/cafaye/wt-m39-pantry-rls-01

./tests/rls.sh                                     # the control: 89 run, 89 passed
./reports/pantry-rls-mutations-01/mutations.sh --list
./reports/pantry-rls-mutations-01/mutations.sh     # control + part 1 + part 2, ~90s

./reports/pantry-rls-mutations-01/mutations.sh --phase part1
./reports/pantry-rls-mutations-01/mutations.sh --phase part2
./reports/pantry-rls-mutations-01/mutations.sh --phase control
```

One run of the suite is ~5s. The full recipe is 12 suite runs and exits 0 on a
clean tree. **That exit status is a claim about the recipe** — control green,
every mutation applied exactly once, every mutation reverted — and the script says
so in those words, because *"the script exited 0"* must never be readable as
*"every breakage was caught."*

### Decisions I made where the packet was ambiguous, and why

| decision | reason |
|---|---|
| **89, not 84** | Measured, §1. 84 is 5 stale and the delta has a commit. |
| **B3 mutates `00005_functions.sql`, B5 mutates `00001_roles.sql`** | The packet says "Part 1 — six policy breakages against `migrations/00006_rls.sql`" and then names `00001_roles.sql` for #5. `current_publisher_id()` is defined in `00005_functions.sql` and nowhere else; stubbing it in `00006` is not possible. Each breakage was applied where the code actually lives. |
| **B5 removes `noinherit` at both sites** | Removing one site is inert (§4, `B5`). Two `replace_once` calls, each grep-confirmed at exactly one match. |
| **`with check (true)` is reported as a diagnostic, not a seventh breakage** | It is not one of the six. Labelling it separately keeps the "6 planted / 6 reverted" tally honest. |
| **`B6`/`M4` may mutate a check** | The packet forbids mutating a check *to make a breakage red*. These break a green check on purpose and revert it. Same mechanism `M4` already used. |
| **Five verdicts, not two** | The packet names grant vs policy. Measurement needed `CONSTRAINT` (an FK refusing because the fixtures never landed) and the `ABSENT-*` family (nothing refused at all). §2. |
| **The recipe lives in `reports/`, not `tests/`** | `tests/` is not a build directory any more and `bin/prime` counts `tests/*.rs`. Putting it in `reports/` keeps it inert to every tier by construction rather than by argument. |