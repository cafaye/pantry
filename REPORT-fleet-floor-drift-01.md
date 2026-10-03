# REPORT — fleet-floor-drift-01

**The floors in this fleet, measured against the trees they sit in.**

Branch: `worker/pantry-fleet-floor-drift-01`
Worktree: `wt-m39-pantry-fleet-floor-drift-01`
Date: 2026-10-03

---

## 1. The defect, and why it is invisible

Every repository in this fleet declares floors: `minimum:` in `gate.yml`,
`BASELINE_*` in CI. Each is a claim about a measured quantity — *this tree runs
at least N tests, inspects at least M files.* The house style, and it is a good
one, is that those numbers are measured rather than predicted.

The claim is unverifiable after the fact, because **a floor below the truth does
not fail.** It passes everything and blocks nothing. `gate_check.py` compares a
proof's count against its floor and reports `gate.floor` when the count is
*under* it — which catches a floor that is too *high* and is structurally unable
to catch one that is too *low*. The drift this packet hunted has no check
anywhere in the fleet.

`billing` shipped one: a declaration reading `1077` over a tree running 1078,
described in its own commit message as measured. It was found by accident during
an unrelated merge. On `master` today `billing` reads `1078`, and this sweep
measured `1078` — so that particular instance has already been fixed. The class
has not.

## 2. The set of floors, derived rather than taken from the brief

The brief says ten services. **Sixteen.** `cafaye/` holds sixteen git
repositories; fifteen carry a `gate.yml`; `kit` has none and is the template
repository. My first draft of this sentence said fifteen and was wrong in the
other direction — `kit` has no `gate.yml`, so fifteen declarations, not
sixteen. The correction matters because a sweep that reports fifteen floors and
finds fifteen repos has not told you which one it skipped.

Derived with `harness/floors_check.py --declare-only`, which walks the fleet
directory for `gate.yml` and reads each one. **71 floor declarations** across
the workspace, which includes every live `wt-m39-*` worktree — 32 distinct
floors in the fifteen checkouts, each appearing twice because a worktree carries
its repository's `gate.yml` verbatim. Plus **3 `BASELINE_*` claims** in
workflows:

| repository | baseline | value |
| --- | --- | --- |
| billing | `BASELINE_RUNS` | 1078 |
| billing | `BASELINE_ASSERTIONS` | 3082 |
| docs | `BASELINE_TESTS` | 16 |

**Where this differs from the brief's survey:**

1. Sixteen repositories, not ten.
2. `guard`, `muse`, `caf`, `courier`, `pantry`, `darkroom` each appear with
   floors the brief does not mention.
3. **`identity` declares eight proofs and not one floor.** It is the only
   repository in the fleet where the gate is declared and no quantity is
   asserted. It is also the repository whose `bin/prime` does not migrate its
   database — the 1430-tests-against-an-empty-schema defect `core/docs/gate.md`
   records. Those two facts together are worth a reader's attention and are in
   §7.
4. Three `gate.yml` files cannot be read by a stock YAML parser —
   `cafaye-rb`, `cafaye-ts`, `identity` — because an `unmet:` value contains an
   unquoted `": "`. They parse under core's own reader and fail under Psych and
   PyYAML. Detail in §7.

## 3. The table

Measured by running each repository's own declared gate, sequentially, one at a
time, and reading the number off the output with the floor's own pattern.
`delta` is `measured - declared`.

| repository / proof | declared | measured | delta | verdict |
| --- | ---: | ---: | ---: | --- |
| **billing** / lint | 127 | 127 | +0 | **exact** |
| **billing** / suite | 1078 | 1078 | +0 | **exact** |
| **caf** / packages | 15 | 15 | +0 | **exact** |
| **caf** / suite | 714 | 714 | +0 | **exact** |
| **caf** / subtests | 875 | 876 | +1 | below *(margin)* |
| **cafaye-py** / suite | 580 | 948 | +368 | below *(documented)* |
| **cafaye-py** / no-skip | 948 | 948 | +0 | **exact** |
| **cafaye-rb** / suite | 220 | 228 | +8 | below *(margin)* |
| **cafaye-rb** / lint | 44 | 44 | +0 | **exact** |
| **cafaye-ts** / suite-pass | 208 | — | — | **not measured** |
| **core** / suite | 280 | 280 | +0 | **exact** |
| **courier** / suite | 1276 | 1282 | +6 | below *(margin)* |
| **darkroom** / suite | 7 | — | — | **not measured** |
| **docs** / offline-suite | 16 | 16 | +0 | **exact** |
| **docs** / contract-tier-ran | 6 | 7 | +1 | **below — FIXED** |
| **docs** / manifests-validated-by-caf | 2 | 2 | +0 | **exact** |
| **docs** / event-claims-checked | 8 | 9 | +1 | **below — FIXED** |
| **docs** / span-names-checked | 6 | 6 | +0 | **exact** |
| **guard** / pass | 423 | 467 | +44 | **below — FIXED** |
| **guard** / files | 19 | 21 | +2 | **below — FIXED** |
| **muse** / suite | 890 | 968 | +78 | **below — REPORTED** |
| **muse** / core-parity | 968 | — | — | **not measured** |
| **pantry** / suite | 7 | — | — | **not measured** |
| **pantry** / total | 144 | 143 | −1 | **not measured (gate red)** |
| **pantry** / skip-count | 0 | — | — | **not measured** |
| **parlor** / suite | 662 | 665 | +3 | **below — FIXED** |
| **parlor** / ci-shape-checks | 41 | 46 | +5 | **not measured (gate red)** |
| **parlor** / check-self-test | 42 | — | — | **not measured** |
| **site** / suite | 301 | 367 | +66 | **below — FIXED** |
| **site** / ci-shape-checks | 48 | 62 | +14 | **not measured (gate red)** |
| **site** / check-self-test | 47 | — | — | **not measured** |
| **identity** | — | — | — | **declares no floor** |

**23 floors measured. 10 exact. 6 fixed. 4 below with a documented margin. 1
below and reported. 9 not measured, each with a reason in §5.**

Plus, unmeasured because it is not a gate floor but the same kind of claim:
`billing/BASELINE_RUNS` 1078 and `BASELINE_ASSERTIONS` 3082, both consistent
with the measured 1078 runs. `docs/BASELINE_TESTS` 16, consistent with the
measured 16. **All three CI baselines agree with their trees.** The
gate.yml/CI split that `billing/gate.yml` records as a past defect has not
recurred.

### The `delta` column is the finding, not the `below` count

Eight floors read `below` on a mechanical `measured > declared` test. **Four of
them are documented margins**, stated in the same file, applied without
exception:

- `caf/subtests` 875 against 876 — "moved it to 875 on a measurement of 876,
  same one-lower rule", after 831→862→865→875, each one lower than measured.
- `cafaye-rb/suite` 220 against 228 — "220 is 228 less a deliberate margin of
  eight."
- `courier/suite` 1276 against 1282 — "1282 whole-suite tests, so 1276 with the
  same margin of 6", the eighth consecutive value under the same rule.
- `cafaye-py/suite` 580 against 948 — the one that is *not* a margin. Its own
  comment says so: "580 is now **368 below it**, which is no longer a margin but
  a floor that would sit 368 tests under the suite", left in place because
  `no-skip` at 948 carries the exact count and two floors at the same number
  would be ambiguous about which one does the work.

**A mechanical reading calls all five the same word, and four of them are
correct.** So the verdict alone is not the answer — which is why `floors_check.py`
prints `delta` next to it. The defect is not "measured exceeds declared". The
defect is **measured exceeds declared by more than the margin the file
documents**, or by more than any margin at all.

`muse/suite` at +78 is the only floor in this fleet with no margin rule behind
it. §6.

## 4. What was fixed, with before and after

Three floors in two repositories, all of them "the number is not what the tree
prints" and nothing more. Every one was re-measured after the edit.

### guard — the drifted pair

`worker/guard-fleet-floor-drift-01` → `b54a002`

    before:  declared pass 423, files 19
             measured 467 pass / 15 skip / 0 fail
                      Ran 482 tests across 21 files
             guard's own comment: "423, against a measured 428"
    after:   declared pass 462, files 21
             measured 467 pass (+5 margin), 21 files (+0) — exact on both

`pass` goes to **462, not 467.** guard's margin is the house rule, argued for in
the comment it sits under — muse holds 890 for 895, courier 530 for 535 — and a
floor at exactly today's number is a floor nobody can move without a deliberate
act. 467 − 5 = 462 keeps the rule and closes the gap. Raising it to 467 would
have been a different decision and is not mine to make silently.

`files` goes to **21 with no margin**, which is the *different* rule the same
file already argues for: a file count does not move when a test is added, so a
margin there buys nothing and costs the only proof that catches a deleted test
file. The comment records that deleting `test/noSkips.test.ts` took the suite
from 428 to 423 — exactly the old `pass` floor — so that proof stayed green
through a deleted file and only `files` caught it.

The comment read "against a measured 428" over a tree printing 467. True when
written, stale by 39, never re-derived. Two test files landed and 19 did not
move: `test/dockerStage.test.ts` and `test/tenantEntryPoints.test.ts`, both named
in AGENTS.md, both in the Dockerfile's test stage — so the `image` CI job's `find`
diff already agreed with 21 and not with the floor.

### site — the biggest single drift in the fleet

`worker/site-fleet-floor-drift-01` → `d621b70`

    before:  declared suite 301
             measured Test Files 29 passed (29)
                     Tests  367 passed (367)
             the comment beside it: "Measured on this tree on 2026-10-02:
             npm test -> Tests  301 passed (301), 16 files (was 23)"
    after:   367 — exact

Sixty-six tests landed and the floor did not move. The comment above this floor
already named the mechanism in advance: it is "the only place in site that writes
down how many tests there are, which is deliberate", there is no ratchet test
keeping it in step, and "raising this number when the suite grows is a thing a
human has to remember until MD12's machinery lands". **A file that predicts its
own drift and has no mechanism to stop it.** Nothing went red: a floor below the
truth is green by construction.

No margin, because this suite has been consolidated before (23 files became 16),
so headroom would accept the next consolidation without anybody deciding to.

### parlor — the smallest, and the most likely to survive

`worker/parlor-fleet-floor-drift-01` → `1e44ce2`

    before:  declared suite 662
             measured Test Files  29 passed (29)
                     Tests  665 passed (665)
    after:   665 — exact

**Three tests.** That is the most likely size of drift to survive several
landings unremarked, because three under a floor looks like a rounding error
rather than a defect. It is the same mechanism as site's, and `parlor`'s comment
names it identically: "it is the only place in parlor that writes down how many
tests there are" and "raising this number when the suite grows is a thing a human
has to remember".

### docs — the two that moved together

`worker/docs-fleet-floor-drift-01` → `b2718ad`

    before:  declared contract-tier-ran 6, event-claims-checked 8
             measured # pass 7
                      event-type claims checked against core's catalog: 9
                      (core publishes 32)
    after:   7 and 9 — both exact, re-measured

Both drifted by one, in the same direction, and **neither went red**, because the
contract tier is a flag away from the default gate: a test added to it does not
touch `# pass 16` and does not touch `BASELINE_TESTS`. The offline tier is held
by equality in CI, which is precisely why the contract tier needs a floor.

**The parenthetical in `event-claims-checked`'s pattern was supposed to catch
this** and did not. The pattern carries `(core publishes N)` so a count checked
against the wrong document cannot be green. Both halves had drifted — 8→9 claims
and 30→32 types — *in the same direction*, and a parenthetical cannot catch two
numbers moving together. That limit is now written next to the pattern.

`offline-suite` (16), `manifests-validated-by-caf` (2) and `span-names-checked` (6)
measured exact and were not touched.

### courier — reverted, deliberately

I drafted a comment for `courier/gate.yml` recording that its floor was measured
correct and calling it the control the drifted floors are measured against. Then I
reverted it: **a note about a floor that was already right is not worth a
reviewer's slot in the log.** It is in this report instead, where it costs
nothing.

## 5. What could not be measured here, and why

Fourteen floors. Every row above carries its reason; this is the grouping.

**Gate red before it prints a number (6 floors).** `parlor`'s and `site`'s
`bin/prime` both exit 1, at `npm ci` or at the CI-shape self-test, so their full
gates produce no measurement. Their `suite` floors were measured directly instead
— `mise x -- npx vitest run`, which is the command `npm test` runs — and those
measurements are what §4 commits. **`npm ci` needed the pinned node: the ambient
node here is v22.12.0 against a `22.22.2` pin, and `npm ci` refuses with
`EBADENGINE`.** Running under `mise x` resolved it. Recorded because the first
attempt failed for a reason that had nothing to do with floors, and a
`not-measured` row whose cause is "wrong node" is a different row from one whose
cause is "wrong number".

**Gate red before it prints a number (2 more).** A gate that exits nonzero has
not produced a measurement, and treating its partial output as one would be
exactly the false green this packet is about. `pantry` (3 floors) and `cafaye-ts`
(1 floor) — named individually in §7.

**A proof whose pattern never appeared (1 floor).** `muse/core-parity` — see §6.
`muse/suite` read `968` from the same run, so that one *is* measured; its
neighbour is not, because its pattern is a negative lookahead over a summary
line that carried `3 skipped`.

**`parlor`/`site` `check-self-test`, and a bound I did not widen (2 floors).**
With Docker on PATH, `validate-ci.sh --self-test` exits 1 before printing a
breakage count — §7.5. With Docker removed from PATH it gets past that and prints
`45 passed, 0 failed, 1 skipped` for parlor, but the breakage loop did not finish
inside a 115s bound. **`check-self-test` is not measurable here, and the reason is
a bound, not a number.** Raising the bound until the loop finished would have
produced a count I could quote without knowing whether the loop is bounded at all,
so it is reported unmeasured.

**A driver bug in my own first pass, which is worth recording.** The first sweep
wrapped each `floors_check` in `timeout 60` while asking the checker for a 300s
cap, so `darkroom` and `pantry` were killed at 60s and reported as
`sweep exit=124`. A `124` says nothing whatever about a floor. The second pass
fixed the bound and re-ran both. Recorded because a `124` in the middle of a
table of measurements is a row about the harness that made it, not about a
repository, and leaving it in would have been the more comfortable mistake.

**What I did not do.** I did not widen any bound to make a suite finish. Where a
suite would not complete, it is reported as not measurable here, with the reason.

## 6. muse — the one real below-floor, reported not fixed

`muse/suite` declares **890**; the tree prints **968**. Delta +78, no margin rule
anywhere in the file — the comment says "890, deliberately below today's 904",
and 904 was already stale when written.

**Not fixed, and here is the reasoning rather than a shrug.** Raising it means
deciding what `muse`'s suite floor should be, and that decision is entangled with
the far larger finding below: `muse`'s `core-parity` proof *cannot be satisfied by
`bin/prime` at all*. Raising `suite` to 968 would put two floors 78 apart, one of
which never fires, and would look like a repair of something that is actually a
different defect wearing the same number.

### The finding: a proof that can never appear

`muse/gate.yml`'s `core-parity` proof is `^=+ (?!.*skipped)([0-9]+) passed` — a
summary line with **no skip on it**. `bin/prime` runs `uv run pytest` without
`MUSE_CORE_SCHEMAS`, so three tests skip every time:

    SKIPPED [1] tests/test_contracts.py:225: MUSE_CORE_SCHEMAS is not set; skipping parity with core
    SKIPPED [1] tests/test_error_vocabulary.py:312: MUSE_CORE_SCHEMAS is not set; skipping parity with core
    SKIPPED [1] tests/test_openapi.py:714: MUSE_CORE_SCHEMAS is not set; skipping validation against core
    968 passed, 3 skipped in 27.85s

`gate_check --prove` reports `gate.proof-missing` on every single run of
`muse/bin/prime`. **This is deliberate and documented** — the file's own header
says "muse's gate is the one gate in this fleet that is green while part of its
suite has not run" and states the alternative is "one line: delete the
`core-parity` proof".

So `muse` has made the opposite choice from every other repository here: rather
than a floor that drifts silently downward, a proof that is permanently red
locally and green in CI (which sets `MUSE_CORE_SCHEMAS` at `ci.yml:163`). **That
is a defensible position and I am not arguing with it.** It is also the reason
`muse/suite` is a judgement call rather than a correction: the floor and the
proof are two halves of one decision, and the decision is about what a local gate
is for.

What I will say is that the numbers inside the decision have gone stale — 890 for
968, and "today's 904" for 968 — and whoever revisits that decision should do it
with the current measurement rather than the one in the comment.

## 7. Findings outside the packet, named

These were not the assignment. They are handed over rather than expanded into.

**1. `cafaye-ts`'s gate is red on `master`, by design.** `bin/prime` exits 1:
`# tests 210 / # pass 207 / # fail 3`. The merge commit says so — "THIS MERGE
LANDS A RED GATE ON PURPOSE", and the three failures are two
`customer-capability.test.mjs` cases stating that the client cannot onboard a
tenant, plus the suite wrapper. **Not a floor problem** — the floor `208` is
unmeasurable because the tree is red. Anyone auditing floors needs to know a red
tree is a legitimate state in this fleet, or they will file it as drift.

**2. Three `gate.yml` files are unreadable by a stock YAML parser.**
`cafaye-rb:117`, `cafaye-ts:193`, `identity:320`. Each is an `unmet:` value
containing an unquoted `": "`:

    unmet: bin/prime: ruby not found on PATH — run 'mise install' in this worktree (exit 127)

core's own reader accepts all three (`cafaye_contract.read_yaml` folds a plain
scalar). **Psych and PyYAML both refuse.** Found because I extracted floors with
Ruby first, got three `Psych::SyntaxError`s, and checked whether that was a
Ruby problem before re-extracting through core's reader. Cheap to fix (quote the
value) and it would let any standard tool read a fleet declaration.

**3. `pantry` has two floors that may be drifting and cannot be told.**
`bin/prime` exits 101: `suite: 143 passed across 13 test binaries` against a floor
of **144**. That is one *below* the floor — the loud direction, the one that
blocks merges. But three tests are also failing, all of them about `core` being
58 commits ahead of `pantry`'s recorded pin, and the suite is therefore not
settled: fixing the staleness could move the count either way. **The `144` may be
correct, may be one high, or may be masking three failures.** I could not settle
it and did not guess. If `pantry`'s floor were genuinely above the tree, that is
the same bug as everything else in this report wearing a louder hat.

**4. `identity` declares no floor at all.** Eight proofs, zero `minimum:`. It is
the one repository in the fleet whose gate is fully declared and asserts no
quantity — and per `core/docs/gate.md` it is the repository whose `bin/prime`
does not migrate its database, so 1430 tests ran green against an empty schema.
The two facts are not connected by any mechanism today. A floor would not have
caught that particular defect, and I am not claiming it would; but a repository
with no quantity assertions is a repository where the next quiet drift is
invisible by construction, and it is the only one where that is true.

**5. `parlor` and `site` cannot run their own self-test on this machine, and the
reason is in the tree.** Both print their pristine-copy baseline as green and then
`self_test: the pristine copy does not pass; the breakages below prove nothing`,
exit 1. `parlor`'s `validate-ci.sh` in the sandbox fails one check —
`the provenance verifier refuses a foreign or inherited stamp` — because the
sandbox has no Docker and `validate-ci.sh:1284` gates that check on
`docker info`. On the real tree it passes. **The `check-self-test` floors (`42`
and `47`) are unmeasurable on any machine with Docker, which is to say on every
machine that matters.** Re-running with Docker removed from PATH gets the
self-test to run and prints a real breakage count — I have that number for
`parlor` but not for `site`, and it is not in the table because the tier it
belongs to is not the tier that runs in CI.

## 8. How the next one is caught instead of found by accident

The audit is a snapshot; the drift is a process. The repository already has the
idiom — `wavecheck.sh`'s "the roster is discovered, not maintained", with its own
comment recording three earlier wrong versions — so the guard is derived the same
way.

**`core/harness/floors_check.py` and `core/harness/bin/floors-check`,** committed
as `ffefb3b`. Derived scope: the repositories come from walking a directory for
`gate.yml`, the floors from reading each one, so a repository added tomorrow is
covered on the day it lands. No roster to go stale.

Four verdicts, not three: `exact`, `below`, `above`, and **`not-measured`** for a
floor this machine could not settle. A skipped row is printed with its reason,
counted in the summary, and never a pass. The exit code is about *measured* drift
only — a laptop missing a toolchain gets a named row, not a red sweep that gets
switched off.

It reuses `gate_check.py`'s own `strip_ansi` and `_compile` rather than
re-implementing them, so the two checkers cannot read a number differently on the
day one of them is edited.

**Should the gate itself assert its own floors? No, and here is why.** `core`'s
suite already does this for one repository: `test_the_gate_floor_is_not_below_the_suite_core_claims`
is named in `core/gate.yml` as the ratchet that stops the floor being left behind.
That works for `core` because `core` is one repository whose own suite can count
its own tests. **It does not generalise**: `billing`'s count is its own, and
nothing inside `billing` can see `gate.yml`'s number without running the gate, and
the gate is the thing the floor is about. A fleet-level sweep is the only place
the comparison can happen, which is what this file is.

**Where it should run.** Not in every service's CI — a repository cannot see its
siblings, and the comparison is inherently cross-repository. It belongs in the
manager's dispatch, as a bounded sweep, for the same reason `wavecheck.sh` lives
there rather than in a service. **What it must not be is per-commit**: eleven
repositories and ten toolchains on every commit is a check that gets switched off
within a fortnight, which is the failure mode `gate_check.py`'s own `--explain`
argues against at length.

**The gap I could not close, and it is the real one.** A sweep run on demand is
the same shape as the audit that produced this report: it catches drift when
someone remembers to look. What is missing is a **static** floor kind — a quantity
derivable from the tree with no toolchain — so that the cheap majority of floors
are checked on every commit and only suite floors need a machine. I looked for one
and did not find it. `lint` floors look derivable and are not: `127 files
inspected` is rubocop's count over the files rubocop's config includes, which is
not a glob a checker can evaluate honestly without reimplementing rubocop's
config resolution. That is why `floors_check.py` has no `static` kind today, and
why the docstring says so rather than leaving the impression that the expensive
kind is the only one that was considered.

## 9. What was committed, and where

| repository | branch | commit | what |
| --- | --- | --- | --- |
| core | `worker/core-fleet-floor-drift-01` | `ffefb3b` | `harness/floors_check.py` + `harness/bin/floors-check` |
| guard | `worker/guard-fleet-floor-drift-01` | `b54a002` | both floors raised, 423→462 and 19→21 |
| docs | `worker/docs-fleet-floor-drift-01` | `b2718ad` | both floors raised, 6→7 and 8→9 |
| site | `worker/site-fleet-floor-drift-01` | `d621b70` | suite floor raised, 301→367 |
| parlor | `worker/parlor-fleet-floor-drift-01` | `1e44ce2` | suite floor raised, 662→665 |
| pantry | `worker/pantry-fleet-floor-drift-01` | this file | the audit |

Not pushed. Not merged. Not tagged. `core`'s own gate was run and is green at
280/280 after the new file landed — worth noting, because `core` has a test that
refuses a harness module importing anything outside a fixed stdlib allowlist, and
the first draft of `floors_check.py` imported `tempfile`. The import is gone and
the log directory is derived from `TMPDIR` by hand.

**Leftover state.** Six `wt-m39-*-fleet-floor-drift-01` worktrees remain — the
six carrying a commit. The other nine were measured and produced nothing worth a
branch, so their worktrees and empty branches are removed; their measurements are
in the table above and that is where they live. Nothing running, no container,
image or volume started, no other session's worktree touched
(`wt-m39-pantry-27` and the `searxng-*` containers were left alone throughout).
Every repository's `master` is unmoved.

## 10. What I would do next

1. **Settle `pantry`'s 144.** Its gate is red for a reason unrelated to the
   floor, which means the floor has never been measured on a green tree. Fix the
   `core` staleness first, then measure. Until then it is the one floor in this
   fleet that might be too *high*.

   `pantry`'s `suite` floor of 7 is **not** a defect and I checked before saying
   so. It reads the LAST `test result: ok` line, which is one binary
   (`tests/schema.rs`, 7 tests), and the file says exactly that in capitals:
   "READ IT AS A FLOOR ON ONE BINARY, NOT ON THE SUITE … the `total` proof below
   is the one that carries the suite-wide claim." It is a presence floor on the
   last binary and was never pretending to be more. Recorded here because a
   reader scanning the table sees `7` next to `144` and reasonably suspects the
   first is vacuous; it is documented, and it is not the ratchet.
2. **Decide `muse`,** with the current numbers. Both halves of the decision
   (§6), not one.
3. **Quote the three `unmet:` values** so a stock YAML tool can read a gate
   declaration. Ten minutes, three lines.
4. **Give `identity` a floor,** or record why it has none. It is the only
   repository where a quiet drift would be invisible by construction.
5. **Find the `static` floor kind** (§8). It is what turns this from an audit
   into a mechanism, and I could not find it in the time I had.