# REPORT-pantry-publisher-rewrite-01 — a publisher could rewrite its own identity

**Branch** `worker/pantry-publisher-rewrite-01` · **base** `master` @ `3ccc679`
**Suite** 89 → **99 checks**, 99 passed, 0 failed, 0 skipped
**PostgreSQL** 18.4 (Homebrew), measured on every claim below

---

## 0. The gap, reproduced first

The packet told me not to believe it and to run the probe. I took
`reports/pantry-narrowing-01/probe.sh` verbatim from `worker/pantry-narrowing-01`
(sha1 `f3e305f3`), ran it in this worktree, and reproduced WORLD 5 exactly:

```
github_login -> 'alpha-renamed' (free)   -> UPDATE 1
github_id    -> 999 (free, unclaimed)     -> UPDATE 1
github_id    -> 202 (bravo's, collides)   -> ERROR 23505 publishers_github_id_key
is_first_party -> true                   -> UPDATE 1
```

It also reproduced something the packet did not mention and I had not found:
`verified` and `claimed_at` are equally writable — `UPDATE 1` for both. So the
hole is **five columns wide**, not three. That is why §2 pins five.

The probe is not in this branch's diff. It was borrowed to measure the gap and
run; it is `worker/pantry-narrowing-01`'s artefact, not mine.

`migrations/00006_rls.sql:104` said the `WITH CHECK` *"is what stops a publisher
from moving a row OUT of its own account by updating `github_id`/`github_login`."*
It said `id = current_publisher_id()`. That pins the primary key. `id` was already
pinned by the `USING` clause on the SELECT policy — **the `WITH CHECK` was
redundant with a barrier that already existed, and the two columns it claimed to
protect were open.**

---

## 1. Decision: `with check (false)`, and why the packet's first option is impossible

The packet listed four candidates. Here is what each one measured as, on this
PostgreSQL.

### 1.1 "A `WITH CHECK` that pins the key columns" — cannot be written in PostgreSQL

This is the interesting result, and it disposes of the obvious fix. The natural
repair is a check comparing the new `github_id` to the old one, which means the
policy references its own relation. **A PostgreSQL policy may not do that.**

The trap is *when* it fails. `CREATE POLICY` **accepts** such a policy — exit 0,
tag `CREATE POLICY` — and the error arrives on the first statement that uses the
table:

```
ERROR:  42P17: infinite recursion detected in policy for relation "publishers"
LOCATION:  fireRIRrules, rewriteHandler.c:2272
```

Two corrections I had to make to my own work here, both because I wrote down a
plausible mechanism instead of the measured one:

- I first wrote "the planner refuses it before it reaches the executor" and cited
  `build_withcheck, preppolicy.c:1049`. **Neither was observed.** The rewriter
  raises it. Corrected in `00008`'s header from a re-measurement.
- I first concluded, from a psql command tag in a filtered stream, that
  `CREATE POLICY` was *refused*. It was accepted; I had mistaken the tag for an
  error. That is the same class of mistake the suite's own header warns about —
  a plausible wrong answer read off a display rather than a check.

Because the rewriter raises it, the failure is **total**: while such a policy
exists the role cannot write the table at all, and every statement answers 42P17.
The reference can be laundered through a function, since a function call is opaque
to the rewriter — but then the rule stops being visible in `pg_policies` and
becomes a `stable` function body a reader must go and find, which defeats the
entire argument for putting the rule in a policy.

**A `create policy` that "worked" is not evidence that a policy works.** `00008`'s
`Down` and the J-tier round trip are what distinguish the two, and that is why the
new migration's rollback is an executable check rather than a claim.

### 1.2 A `BEFORE UPDATE` trigger

Works, and is boring. Rejected because it is not a policy: the refusal would be a
trigger's error naming a trigger function, the rule would live in
`pg_trigger` rather than `pg_policies`, and it would be a third mechanism in a
schema whose stated position (D17) is that the *grant* and the *policy* are the
two boundaries worth having.

### 1.3 Column-level privileges

Technically the finest-grained tool available, and rejected for two reasons. The
refusal is `permission denied for column github_id` — SQLSTATE 42501, but the
**grant** boundary, and the packet asked for a policy. And column grants only bite
if the table-level `UPDATE` is revoked, which moves the rule from the policy
catalog into the grant list. D17's own reasoning says why that is a downgrade:
"a role with no privilege never reaches a policy", so `pg_policies` would show a
publisher update that does not exist and the grant list would carry a rule about
`github_id` that no policy mentions. The audit trail moves somewhere less
legible to answer the question it exists for.

### 1.4 What I did: `with check (false)`

```sql
create policy publishers_publisher_update
  on pantry.publishers
  for update
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id()))
  with check (false);
```

Every publisher write now fails with `42501 new row violates row-level security
policy for table "publishers"` — a **policy** refusal, attributable to a clause
in a file, rather than unique-index arithmetic about somebody else's row.

The `USING` clause stays, and it is not vestigial: it is what makes the policy
"the publisher may write to **its own** row" rather than "refused", and it is
what scopes `pantry`'s maintenance reads to one publisher. `false` alone would be
an equally strong statement and a less informative one.

### 1.5 It pins five columns where the question was about three

This looks like over-refusal so here is the whole justification, and none of it is
a guess:

**There is no publisher-shaped write on `publishers` to refuse.** DECISIONS.md D31's
role map says it in the row for `pantry_publisher`: *"a publisher's own rows —
submit a service, add a version, add an edge, withdraw … Phase 1 has no
self-registration so `publishers` rows come from a migration, a fixture or an
admin."* And nothing in `internal/` writes to `publishers` —
`internal/pantrydb/queries/` is the whole data path and it is reads. The `update`
verb in `00006`'s grant is **unused**.

**`is_first_party`: no publisher flow exists.** `00002_publishers.sql:18-23` gives
the reason — *"a first-party service is a fact about the cafaye fleet's own
manifests, decided by a reviewed commit to `registry/index.yml`, and never by
anybody who registers an account."* Phase 1 is official-only by decision, so no
registration webhook exists and there is no account to register.

**`verified`: pinned because a policy branches on it.** This is the one beyond the
packet's three and it deserves the most argument, because it is
`00002`'s **own sentence** rather than a new opinion:

> "a policy that branches on a column the publisher can set is a policy the
> publisher controls."

`publishers_public_read` is `using (not is_first_party and verified)`. So a
publisher that could write `verified` on its own row **decided its own public
visibility** — measured: `update … set verified = false` by `pantry_publisher`
answered `UPDATE 1`. `00002` correctly noted `verified` is not an *authorisation*
input; but it is a *public-visibility* input, which is the same failure with a
smaller blast radius, and the file's rule does not have an exception for that.

**`claimed_at`: pinned because it is the only evidence about the account.**
*"the one value in this table a later row can be checked against after the login
has been renamed"* — editable by the account it is evidence about.

### 1.6 If a flow ever needs one of them, it must widen this on purpose

That is the tripwire, and it is why the check is `false` rather than a list. A
list invites an edit that adds one column and reads as a decision. `false`
refuses everything until somebody writes down which flow needs which column.

### 1.7 The alternative I chose *not* to take, and M5 explains why

The obvious cheaper change is to **delete** `publishers_publisher_update` and drop
the useless `update` from the grant. It closes the hole too. It is worse, and M5
measured why — see §4.

---

## 2. What was corrected, and what was deliberately left

### 2.1 `00006_rls.sql:104` — the required correction

Corrected **in place**, with the measurement in the comment and a pointer to
`00008`. Comment-only, so no deployed database desyncs: goose records versions,
not checksums. `00006` deliberately keeps the write half open so `goose down` from
`00008` returns to exactly the state `00006` describes.

The replacement names the mistake rather than erasing it, because the shape is
worth keeping: *a barrier named in a comment above the code is not a barrier, and
this one was believed by the reader who wrote it.*

### 2.2 `00002_publishers.sql` — same defect class, and my change would have made it worse

Not in the packet's scope, but **I could not leave these**:

| claimed | actual |
|---|---|
| "`00005_rls.sql`'s insert policy refuses the write" | there is no `00005_rls.sql`; the file is `00006`. There is **no insert policy** on `publishers` — `pantry_publisher` holds no INSERT privilege at all (D17). |
| `is_first_party` is "the one field on this table that no publisher may set" | wrong before (four others were settable) and wrong after (none are). |
| `github_id` — "Immutable" | a claim with nothing behind it until `00008`; now true. |
| `verified` — "a publisher role writes its own rows whether or not this is true" | `00008` makes it false, and it condemned the very column it vouched for. |

Leaving any of these would mean my own change had created a fresh false comment.
That is the standard the packet sets: a comment naming a protection that does not
exist is worse than no comment.

### 2.3 Found, reported, NOT fixed — same defect, out of scope

`00001_roles.sql:8,39,44` and `00003_services.sql:165` all say `00005_rls.sql`
where the file is `00006`. Pre-existing, unrelated to my change, and fixing them
would widen the diff into files this packet does not otherwise touch. Listed here
rather than silently repaired.

`migrations/README.md` said the suite had **77** assertions. It had 89. I corrected
it to 99 because I had just corrected the row above it, and leaving a contradicting
number 130 lines down is the same defect as a false comment.

### 2.4 An observation, not acted on

`tests/rls_checks.sh` prints four `command substitution: syntax error` lines on
every run, from backticks inside `$( )` in the seed function's comments (line 83).
Pre-existing, cosmetic — all 99 checks still run and pass. Not mine to fix here.

---

## 3. The checks, and where they belong

**89 → 99.** Ten new checks, in three tiers, because they are three different
claims and one tier cannot carry all three.

### 3.1 D18–D23 — publisher isolation → **D**

D is right. Every other `pantry_publisher` write refusal is already there (D9–D17),
and the packet's FORCE warning does not apply: D's harness impersonates with
`set role`, and **D1 already asserts that impersonation is subject to RLS**. The
packet's rule — *a check about policy scoping is D's business* — is exactly this.

| check | statement | why separate |
|---|---|---|
| **D18** | `github_id = 999` (unclaimed) | **the case no constraint can close.** The same write aimed at another publisher is refused 23505. D18 is the check that says a *policy* is there. |
| **D19** | `github_login = 'alpha-renamed'` | the rename |
| **D20** | `is_first_party = true` | the trust claim |
| **D21** | `verified = true` on **`a4`** | `a4`, not `a1`: the only write that changes what the *public* can read |
| **D22** | `claimed_at = 1999` | the evidence restamp; documents that the pin is wider than three |
| **D23** | the identity tuples, after all five | the one check that reads a **row** |

Six rather than one: a single "may not change its identity" statement touching all
five columns is satisfied by a barrier that pins four of the five. One column per
check, each with its own name and its own hole.

D23 exists because five `deny` checks cannot tell *refused* from *silently did
nothing*. M5 shows exactly that (§4).

### 3.2 F10 — the owner's exemption → **F**, not D

The packet's structural warning, made into a number.

`publishers_publisher_update` names **both** `pantry_publisher` and `pantry`, so a
barrier added for the publisher reaches the owner for free — and **nothing in D
would notice if it stopped.** In SHAPE1 `pantry` *owns* all four tables
(`tests/rls.sh:455` hands them over), so it holds UPDATE implicitly and is inside
the policy's reach. Without F10, un-`FORCE`ing the table would leave the suite
fully green.

Also: `pantry` holds **no table grant** on `publishers` in SHAPE1, so *before* the
ownership handover this statement dies at `aclcheck_error` — the grant-boundary
refusal D13 asserts, reached for a different reason and meaning something weaker.
Owning the table is what puts it inside the policy.

### 3.3 E13–E15 — the flows the pin must not break → **E**

Green before the fix and green after it. That is the job: **a fix that stops the
attack and also stops the fleet's legitimate writes is green on the checks that
assert the attack and red in production.** A security fix is not finished when the
attack fails; only E can say the second half.

| check | the flow, and where it is named |
|---|---|
| **E13** | rename a `github_login` — `00002` keeps the column precisely because "a login can be renamed"; a registry that cannot follow a rename is wrong |
| **E14** | set `verified` — "whether the OAuth flow proved control"; the callback is privileged server-side, so it is `pantry_admin`, never the account holder |
| **E15** | set `is_first_party` — "decided by a reviewed commit to `registry/index.yml`" |

One row each, so no single value carries two checks and a reader can tell which
flow broke from which row. M4 proves they have teeth.

---

## 4. The mutation table

`reports/pantry-publisher-rewrite-01/mutations.sh`. Control green, five breakages,
each grep-confirmed to apply **exactly once** before the run, each reverted from
**HEAD** (never master — that would delete the fix) and followed by a full green
re-run.

| | breakage | fails | of the ten, red | predicted |
|---|---|---|---|---|
| **M1** | `with check (false)` → `(true)` | 8 | D18–D23, F10 | D,F ✓ |
| **M2** | → `00006`'s `id = current_publisher_id()` | 8 | D18–D23, F10 | D,F ✓ |
| **M3** | `no force row level security` on `publishers` | 4 | **F10 only** | F ✓ |
| **M4** | `publishers_admin_update` `(true)` → `(false)` | 3 | **E13–E15 only** | E ✓ |
| **M5** | `00008`'s `create policy` deleted | 6 | D18–D22, F10 | D,F ✓ |

**7 green controls · 5 reverts verified · 0 mismatches · 0 problems.**

Three of these carry the argument:

**M2 is the load-bearing one.** `00006`'s clause *is* a barrier — it refuses every
statement these checks make — and it is what actually shipped. M2 shows it is not
enough. A suite asserting only "a policy exists" would have been green over the
thing it was supposed to be about.

**M3 is the FORCE proof, measured.** Change nothing but `force`, and D18–D23 stay
**green** while F10 goes red. The packet said *D is structurally blind to FORCE*;
this is the number.

**M5 reversed a conclusion, and the reversal is the most useful thing here.** I
wrote it expecting the security to break. It does not — nothing becomes writable.
What breaks is the **attribution**:

```
set role pantry_publisher; select pantry.begin_publisher('…a1');
update pantry.publishers set github_id = 999 where id = '…a1';
  UPDATE 0
psql exit code: 0
```

RLS with no matching UPDATE policy **filters rather than raises**, so the write is
refused *silently* and no rule is named. D18–D22 and F10 are `deny` checks, so
they go red — correctly, because they assert the **mechanism** and not only the
outcome, and `UPDATE 0` is indistinguishable from "your WHERE matched nothing".

**So the real argument for an explicit `with check (false)` over deleting the
policy is not security.** Both close the hole. Only one turns a silent filter into
a 42501 that names `publishers`. A caller that cannot tell "refused" from "matched
nothing" will eventually retry, log success, or file a bug against the wrong thing.

And **D23 stays green under M5** — it reads the row, the row is untouched, and it
cannot tell either. Six checks, five mechanism-visible.

### 4.1 Collateral reds, reported and kept out of the verdict

| mutation | knock-on | what it means |
|---|---|---|
| M1, M2 | **E2** | an existing check nobody touched went red: with the hole open, a publisher's rename lands and `pantry_admin` then reads `alpha-renamed`. **The hole does not stay inside the row** — the clearest single piece of evidence in the packet. |
| M3 | F1, F6, J5 | the other FORCE checks, all agreeing with F10 |

These are named but excluded from the verdict, because the verdict is "did the ten
new checks catch this", and a pre-existing check going red for a knock-on reason
neither strengthens nor weakens that answer.

### 4.2 Three harness bugs, all mine

All three were found by the discipline, not by reading:

- **`$frag` where `read` had assigned `_frag`**, under `set -u`. It died four
  lines after M1's verdict printed and **before** its restore, leaving `00008`
  carrying `with check (true)` in the working tree. Only `git status` caught it.
  → the exit trap, which reverts and says so. Tested by planting a mutation and
  tripping it deliberately.
- **`tree_matches_head` counted untracked files.** `git checkout` cannot remove
  them, so an editor's `.bak` made the restore check report FAILURE *after a
  revert that had succeeded* — it said "AND THE REVERT FAILED" about a file it
  had correctly restored. Tracked-content and untracked-listing are now separate
  functions; untracked files are named, never `git clean`ed.
- **`--phase part5` called a function that did not exist.** The shell said
  `do_part5: command not found` on stderr and the summary — which never reads
  stderr — satisfied `REDS -eq PLANTED` at `0 == 0` and printed **`ok`, having
  planted nothing**. `PLANTED -gt 0` is now a condition. A pass manufactured by
  nothing having been measured is the defect, not the result.

---

## 5. Fix vs mutation, made verifiable rather than asserted

The distinction is executable. `intended_diff()` asserts the committed diff against
master is **exactly** these five files:

```
migrations/00002_publishers.sql                       (comments only)
migrations/00006_rls.sql                              (comments only)
migrations/00008_publisher_identity_immutable.sql      (the fix)
migrations/README.md                                  (the row for 00008, the count)
tests/rls_checks.sh                                   (the ten checks)
```

A mutation that was not reverted shows up as an **extra** file; a fix reverted by
mistake shows up as a **missing** one. `restore()` checks out `HEAD`, never
`master` — the same line in `REPORT-pantry-rls-mutations-01`'s recipe is
`git checkout master`, and copying it here would have deleted this packet's fix.

`migrations/` and `tests/` are byte-identical to HEAD, and `git status` is empty.

---

## 6. Commits

| | commit | what |
|---|---|---|
| 1 | `0e77f36` | **the red check, alone.** 99 checks, 8 failed, before any fix existed |
| 2 | `7dd92d7` | `with check (false)`, `00008`, and the corrected comments |
| 3 | `2c842f7` | the five breakages |

The red was committed first and on its own, so it survives independently:

```
D18 the statement was allowed. output: SET UPDATE 1
D19 the statement was allowed. output: SET UPDATE 1
D20 the statement was allowed. output: SET UPDATE 1
D21 the statement was allowed. output: SET UPDATE 1
D22 the statement was allowed. output: SET UPDATE 1
D23 wanted: 101|alpha|false|true,404|unverified|false|false
     got:    404|unverified|false|true,999|alpha-renamed|true|true
E2  COLLATERAL: wanted alpha,bravo,cafaye,unverified
     got:    alpha-renamed,bravo,cafaye,unverified
F10 the statement was allowed. output: SET UPDATE 1
```

Writing the red found a bug of mine: E15 used `$A2`, which was never defined, and
`set -u` caught it. `A2` added beside `A1`/`A4`.

---

## 7. The rollback is loud

`00008`'s `Down` restores `00006`'s policy byte for byte — a true inverse. Rolling
back a security tightening *does* grant the hole back, and that is the correct
inverse rather than a bug: the refusal to be quiet about it is that the checks
fail loudly. On a database where `00008` has been rolled back, `tests/rls.sh`
reports ten failures naming themselves rather than accepting the write.

J1–J5 confirm the round trip: `00008` adds no table, view, function, trigger or
new policy name, so the down chain leaves nothing behind (`J1`), the re-applied
table set is identical (`J3`), the policy **count** is unchanged (`J4`), and FORCE
survived (`J5`).

The count claim was verified against `master` rather than by arithmetic, because
arithmetic was wrong twice in this packet. `00008` *contains* two `create policy`
statements — one in the Up, one in the Down — and grepping for them suggests the
count rose by two. It did not: the Down's does not run during `up`.

```
master   baseline: 35 policies over 3 tables   checks: 89 run, 89 passed, 0 failed
branch   baseline: 35 policies over 3 tables   checks: 99 run, 99 passed, 0 failed
```

35 → 35. The fix replaces a policy rather than adding one. That also makes `J4`
the check that would catch a future migration which adds a policy without a
matching `down`.

`J4` is a count, and a count is satisfied by policies that are all `using (false)`
— a database that refuses everything. It is D18–D23 that stop that reading, which
is the same argument M2 makes.

---

## 8. Open, for the manager

1. **`pantry_publisher` still holds `update` on `publishers`,** which nothing uses.
   The policy now refuses every use of it. Revoking the verb would be a second,
   independent barrier — and D17's own argument is that two barriers are the shape
   to prefer. I did **not** do it: it would move the refusal from the policy
   boundary to the grant boundary, and the packet asked for a policy. Recorded,
   not decided.
2. **`00001`/`00003`'s `00005_rls.sql` misreferences** (§2.3). One-line fixes, out
   of scope here.
3. **`DECISIONS.md` has no D18**, though the packet refers to "the D18 cross-tenant
   question". The number appears in `CHANGELOG.md:338` as *"pantry 0 cross-tenant
   negative-test packet"*, where the `pantry` prefix suggests it was a sequence
   number rather than a decision number, and DECISIONS.md's own decisions are
   numbered `D1`–`D33` with no `D18` among the RLS work. If D18 is meant to be a
   decision it needs writing; if it was a typo for something else, the packet should
   say which. **I did not invent a `D18` entry** — AGENTS.md: one number per
   decision, ever.

---

## 9. Reproduce

```sh
cd /Users/kaka/Code/any/moon/cafaye/wt-m39-pantry-rewrite-01
PANTRY_PG_BIN=/opt/homebrew/opt/postgresql@18/bin ./tests/rls.sh
./reports/pantry-publisher-rewrite-01/mutations.sh
```

Both are green: **99 checks, 99 passed, 0 failed, 0 skipped**; **5 mutations, 5
verdicts as predicted, 7 green controls, 5 reverts verified, 0 problems.**

### 9.1 Diff this branch with THREE dots

This branch was cut from `3ccc679`, and `master` has since advanced to `e7ac3b3`,
which merged `worker/pantry-narrowing-01`. So:

```
git diff master worker/pantry-publisher-rewrite-01        # TWO dots — MISLEADING
```

reports `REPORT-pantry-narrowing-01.md` and both files under
`reports/pantry-narrowing-01/` as **deleted**. They are not. My branch is one
merge behind and simply does not contain them; nothing in this branch removes
them. A two-dot diff between a branch and a master it does not include always
reads that way.

```
git diff master...worker/pantry-publisher-rewrite-01       # THREE dots — correct
```

is the change set, and it is the seven files in §5. The merge itself is clean and
verified with `git merge-tree`: `narrowing-01` **added** three files and touched
neither `migrations/` nor `tests/`, so there was nothing for this branch to
conflict with. Merging loses nothing of it.