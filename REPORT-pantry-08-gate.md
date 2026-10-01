# REPORT-pantry-08-gate — declaring the gate, and the false green it was hiding

**Packet:** `pantry-08-gate` · **Branch:** `worker/pantry-08-gate` · **Base:** `92ca41e`
**Not pushed.** The manager merges and pushes after the gate is green.

## The one-paragraph version

pantry had no `gate.yml`, so `bin/prime` could be replaced with `exit 0` and
this repository would have stayed green. Writing the declaration meant measuring
what the gate actually does, and the measurement found something worth more than
the file: **a skip in this suite is invisible.** Every environment-gated tier
prints `SKIP …` and returns early from the test body, which libtest scores as a
PASS, and libtest then discards the stderr that said so. On a real clone with no
cafaye sibling — which is what a pantry-only clone is, and what CI's `build` job
sees — `bin/prime` exited 0 reporting `114 passed; 0 failed; 0 ignored` with the
entire drift tier skipped and not one word about it. That is the `cafaye-rb`
defect (1430 tests green against an empty schema) wearing a Rust hat. It is
fixed: `bin/prime` runs with `--nocapture`, counts the skips, and prints
`skips: N` on every run.

## 1. The two commands, measured

The brief asks for the developer-facing command and the CI command, because they
are often not the same. **Here they are not the same, and that is the second
finding.**

| | Command | Source |
|---|---|---|
| Developer-facing | `bin/prime` | AGENTS.md "Order of work" step 5; the script header |
| Fleet spelling | `mise run prime` → `./bin/prime` | `mise.toml` `[tasks.prime]`, read not assumed |
| CI, one job | `./bin/prime` | `.github/workflows/ci.yml:266`, `workspace-drift` |
| CI, the other job | **four separate `cargo` steps, not `bin/prime`** | `ci.yml:25-51`, `build` |

`bin/prime` and `mise run prime` are one gate — `mise.toml`'s task resolves to
the same file, and the checker enforces that, so they cannot drift apart.

**Finding 1 — the `build` job does not run this gate.** It re-spells the four
cargo steps as four `run:` lines. So a step added to `bin/prime` is not run by
`build`, and nothing in the workflow or the suite notices. This is exactly the
local-gate/CI-gate drift `core/docs/gate.md` says the format exists to prevent,
sitting in the repository that has the declaration.

**Not fixed here, deliberately.** Adding a `run: ./bin/prime` step to that job is
a change to how CI verifies this repository, which is the manager's call, and
doing it in this packet would have meant the declaration going green over a
workflow the same packet rewrote. Recorded in `gate.yml`'s `ci` block and here.

`gate.yml`'s `ci.invokes` is `[./bin/prime]` — true as written, because that argv
does appear in the `workspace-drift` job's `run:` body. `check_ci` reads one
workflow's run bodies and nothing else, so it cannot see that the other job
bypasses the gate. That is a limit of the checker, not a mistake in the
declaration, and it is stated in the declaration rather than left implied.

## 2. What was measured, before anything was declared

All runs on a saturated 8-core arm64 host (load average >100), rustc 1.95.0,
cargo 1.95.0, mise 2026.8.4, Go 1.26.1.

**A run with the workspace reachable** (`PANTRY_CAFAYE_ROOT` → the cafaye
workspace):

| Measure | Value |
|---|---|
| Tests passed | **114** |
| Failed / ignored | 0 / 0 |
| Integration test binaries | **11** (api, ci, contract, core_pin, drift, filters, kind, manifest, pin, recorded_copy, schema) |
| `test result:` lines | 14 (11 integration + lib + bin + `Doc-tests pantry`) |
| Wall clock, warm | ~4m17s cold-warm; ~26s fully warm |

Per binary: api 24, ci 7, contract 9, core_pin 7, drift 12, filters 13, kind 7,
manifest 13, pin 11, recorded_copy 4, schema 7. Lib, bin and Doc-tests are 0.

**A run with no cafaye workspace** — a `git clone` in a directory with no
`core/schemas/cafaye.manifest.schema.json` anywhere up the tree:

| Measure | Value |
|---|---|
| Exit code | **0** |
| Tests passed | **114 — identical** |
| Failed / ignored | 0 / 0 |
| `SKIP` lines in the output | **0** |

That table is the finding. Not one number in it moved.

## 3. Finding 2 — a skip in this suite is invisible, and it is fixed

**Measured, not reasoned about.**

The mechanism, in three steps:

1. **A skip here is a `return`, not an `#[ignore]`.** There is not one
   `#[ignore]` in `tests/`. The gated tiers call `require_workspace!` /
   `skip_without_workspace`, which `eprintln!` and return. `tests/drift.rs` has
   8 such tests, `tests/schema.rs` 4, `tests/core_pin.rs` 5,
   `tests/recorded_copy.rs` 1 — 18 tests whose entire verification can evaporate.
2. **libtest scores an early return as a PASS.** So `drift.rs` reports
   `test result: ok. 12 passed` with all 12 skipped.
3. **libtest captures a passing test's output and discards it.** So the `SKIP`
   lines never reached the terminal either.

Verified directly rather than inferred. On the bare clone:

```
$ cargo test --test drift                 # 8 SKIP lines: 0
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test --test drift -- --nocapture  # 8 SKIP lines: 8
SKIP base path: no cafaye workspace found next to this repository. Set PANTRY_CAFAYE_ROOT …
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Same count both ways. Only the flag differs.

And confirmed on the library itself, so it is not something specific to pantry's
tests: a two-line crate whose one passing test does nothing but `eprintln!`
prints nothing under plain `cargo test`.

**Why this is worse than a skip.** `kit`'s AGENTS.md rule, and this repository's
own `tests/drift.rs` header, both say *"A skip is reported, never hidden."* That
was true as an intention and false as a fact: the report was being written to a
stream the harness throws away. A repository whose stated rule is "a skip is
never hidden", where a skip is hidden, is the exact shape of the incident this
project keeps rediscovering.

**The fix, and what it does not do.** `bin/prime` now runs
`cargo test --no-fail-fast -- --nocapture`, counts lines beginning `SKIP `, and
prints `skips: N` on **every** run — not only when non-zero, because a report
that only appears when something is wrong is one nobody learns to read. When N is
not zero it names `PANTRY_CAFAYE_ROOT` and states that those tests counted as
passes. It also prints `suite: N passed across 11 test binaries`, which gives the
declaration a floor that moves when any test is lost.

**Nothing was weakened.** No assertion loosened, no skip added, no retry raised,
no test changed. The skip still happens; the suite still passes; the gate still
exits 0. What changed is that it now says so. The measurement is re-runnable:
clone this repository somewhere isolated and run `bin/prime` before and after.

## 4. Finding 3 — the toolchain pin was declared in three files and enforced in none

`mise.toml` pins `rust = "1.95"`, `Cargo.toml` declares `rust-version = "1.95"`,
and `docker/Dockerfile` has `ARG RUST_VERSION=1.95`. There is no
`rust-toolchain.toml`, and `bin/prime` ran bare `cargo` — so whatever rustc
answered on PATH formatted, built, linted and tested the gate, and a mismatched
toolchain reported itself as a wall of cargo errors about features and resolver
behaviour that reads as a defect in the code under test.

This is the cafaye-rb lesson in a different language: there, `bin/prime` picked
up macOS system Ruby 2.6.10 while the declaration's own `external` requirement
named the pinned 4.0.1, and every failure was a gem error about a language
feature the project had legitimately stopped supporting.

`bin/prime` now asserts the pin before anything else runs, reading it out of
`mise.toml` — the file that owns it — rather than hardcoding a fourth copy. A
different minor version exits 127 naming `mise install`; a patch release of the
pinned toolchain passes, because that is the same toolchain. Both directions were
tested: a simulated 1.94.2 answering exits 127, a simulated 1.95.3 does not.

## 5. The declaration

`gate.yml`, written against `core/schemas/gate.schema.json` and checked by
`core/harness/gate_check.py`. `version: 1`, `name: pantry`,
`command: [bin/prime]`, `miseTask: prime`, `entrypoint: bin/prime`,
`timeoutSeconds: 3600`.

**Eight proofs, three with floors** — the schema's `maxItems` is 8 and the
checker refused a ninth with `gate.schema: gate/proof: minItems 1, maxItems 8`.
The `fmt` banner was cut to fit and `gate.yml` says why; it is the weakest of the
three step markers because a formatting failure is caught by the exit code either
way. Raising the cap is a core change and therefore not this packet's call.

| id | shape | floor | what it catches |
|---|---|---|---|
| `toolchain` | `==> toolchain: rustc 1.95.x (mise.toml pins 1.95)` | — | the pin check was deleted |
| `suite` | `test result: ok. N passed; 0 failed; 0 ignored; …` | 7 | a suite that ran to the end |
| `total` | `suite: N passed across 11 test binaries` | 112 | **any** test lost |
| `targets` | `Running tests/<file>.rs (…)` | — | the suite stopped launching test binaries |
| `skip-count` | `skips: N` | 0 | the skip report stopped being printed |
| `clippy` | `==> cargo clippy -D warnings` | — | the gate quietly stopped linting |
| `lint` | `==> the manifest validates against core's schema` | — | the manifest step was deleted |
| `ok` | `==> ok` | — | the gate completed |

### Three floors, and why `suite`'s is only 7

`minimum` is read from the **last** match. `cargo test` prints one
`test result:` line per binary and the last is always `Doc-tests pantry`
reporting **0 passed**. A floor written the obvious way — `([0-9]+)` — therefore
reads 0 on every run in this repository, and any positive floor under it is a
gate that can never go green. The leading `[1-9]` is the correction, and
darkroom's declaration made the same correction for the same measured reason.

With that, the last matching line is `tests/schema.rs` at 7, so `suite`'s floor is
7. That is honest but narrow, and it is why `total` exists.

### Two attempts at a floor on the test-FILE count, and what the checker said

guard's `files` floor is the detector that catches a deletion too small to move
the suite count. Both attempts here were refused, and both refusals are the check
working on the declaration that declares the checks:

1. **No capture group + a `minimum`** → `gate.proof-invalid`: *"its pattern has
   0 capture group(s); a floor is read from exactly one."*
2. **Capture the filename + a `minimum`** → `gate.proof-invalid`: *"sets a minimum
   but its capture group is not a number."* `gate_check.py:1237` collects only
   matches whose group 1 `.isdigit()`, so a filename is not a way to smuggle a
   count past a floor — it is refused precisely because there is no number in it.

**So a floor on the test-file count is not expressible in this format**: the
checker reads floors out of the output, and cargo prints no total of test files
anywhere. `targets` is therefore a presence proof, and `gate.yml` states the
consequence rather than dressing a pattern up as a count.

## 6. Finding 4 — the near-miss, caught by measuring file sizes

The `total` floor was first set to **110**, a margin of five, copying
guard/muse/courier. That margin was wrong for this repository, and the reason is
a measurement rather than a preference:

```
4 tests/recorded_copy.rs      ← smallest file
7 tests/ci.rs
7 tests/core_pin.rs
7 tests/kind.rs
7 tests/schema.rs
```

Deleting `recorded_copy.rs` took the total from 114 to **110**, which *satisfies*
a floor of 110. **The smallest test file in `tests/` was invisible.** That is
guard's near-miss reproduced, in a different repository, by copying a margin
without measuring the thing it was protecting.

The floor is now **112** — a margin of two, smaller than the smallest file — and
`bin/gate-self-test` asserts the 4-test deletion goes red as `gate.floor`, with
the 7-test deletion beside it.

The general rule this produced, worth carrying: **a floor's margin must be
smaller than the smallest unit of coverage it is meant to detect.** The other
adopters' five-test margin is right for repositories whose smallest unit is
larger. Copying a margin across repositories without counting is copying the
defect along with the convention.

## 7. `bin/gate-self-test` — the declaration proved able to fail

36 breakages, 6 warning cases, 0 skipped. Each asserts the copy goes red **and
names the finding it expects** — naming being the half that decays silently,
since a check written for one specific defect can be dead code forever while
everything stays green.

A **pre-flight** runs `bin/prime` once, before any case, and prints its verdict
in the reader's eye. That is not tidiness: pantry reads sibling checkouts, so a
merge in `core` can turn this repository's suite red for reasons unrelated to any
declaration, and every "must stay green" case below is then unjudgeable. The run
distinguishes the two by `gate.nonzero` and reports the second as a named SKIP,
never as a pass and never as a failure of `gate.yml`.

Controls run first in both phases; the proving control runs the real `bin/prime`.
Not part of `bin/prime`: the checker is core's and is not vendored here, and a
self-test inside every gate invocation is a second gate that can disagree with
the first.

**Three ways the gate can fail, and one that must stay green:**

1. **A stand-down tier.** Deleting `--nocapture` and the reporting block →
   `gate.proof-missing`. Nothing in the declaration is wrong; the gate exits 0
   having verified nothing.
2. **A gate that exits zero without running anything** — both `exit 0` written
   over `bin/prime` and the script deleted → `gate.proof-missing` /
   `gate.entrypoint-missing`. This is the defect the format exists for.
3. **A deleted test file** → `gate.floor`, asserted against **both** the 4-test
   and the 7-test files, plus the near-miss asserted directly: with only `suite`
   left, a 7-test deletion is **green**, which is what makes `total`
   load-bearing rather than decorative.
4. Plus a step deleted (`clippy`, the toolchain check, the manifest step, the
   suite-total report), a floor raised past reality, a floor with no capture group,
   a floor with two, a pattern matching nothing, a gate that prints everything
   and then exits 1, a timeout, and the CI/mise/external blocks disagreeing with
   the tree.

**One case is expected to stay GREEN, and asserting that is the point:**
`selfContained: true` with an empty requirements list is a valid declaration of a
false thing. The checker reads the declaration, not the machine; no breakage will
ever turn it red. `external` is load-bearing in one direction only.

### The self-test caught seven bugs in my own work

Each was invisible to the static phase, which is the argument for the controls:

| What | Caught by |
|---|---|
| `targets` had a floor with no capture group | the **proving control** |
| `can_prove` inverted, so every proving case silently skipped | reading the summary line |
| the "only `gate.nonzero`" test — false, because a red gate also trips floors | the run against a red repository |
| a breakage recipe quoting backticked prose | `edit`'s fail-loudly check |
| the `two-capture-groups` case firing on the wrong proof | `expect_red`'s finding assertion |
| the `skip-report-deleted` case removing only half the defect | `expect_red` reporting green |
| the `total` floor's margin hiding the smallest test file | counting `#[test]` per file |

The first and third are the ones worth keeping. `gate.proof-invalid` and
`gate.nonzero` do not exist in the static phase, so a self-test whose control is
static-only calls all of that green; and a discriminator written from intuition
instead of from a captured report is a discriminator waiting to be wrong.

## 8. What I could not verify, and did not claim

- **A floor on the test-file count.** Not expressible; `targets` is a presence
  proof and `gate.yml` says so. Both refusals are quoted above.
- **That the drift tier ran.** The declaration makes the count visible and the
  gate fail if the reporting stops; no floor here fails when the tier *skips*.
  `minimum` is a lower bound and the failure that matters is the count going UP.
  What enforces the tier is `tests/ci.rs`, which predates this packet:
  `the_drift_job_is_enabled` fails if the job is disabled or deleted, and
  `the_drift_job_points_the_suite_at_the_checkouts` fails if it stops running
  `./bin/prime` or sets `PANTRY_CAFAYE_ROOT` to anything but `${{ github.workspace }}`.
- **That a requirement is satisfied.** `gate.requirement-unproven` is a warning
  by design; three appear on every run and none moves the exit code.
- **The `build` job.** Reported in §1, not fixed.
- **Whether `--nocapture` is the right long-term shape.** It makes every test's
  stdout visible, which is noisier than before. The alternative — a separate
  assertion that no test skipped — is a different packet, and `bin/gate-self-test`
  records the current behaviour in both directions so a future change has to
  notice it.

## 9. pantry's gate is RED right now, and it is not this packet's doing

**Read this before merging.** As of this run `bin/prime` in pantry **exits 101**
with two failures, and both are defects in `core`, not in pantry:

```
    every_example_in_core_s_valid_examples_is_classified_by_this_table
    this_repository_says_how_far_behind_core_it_is
```

1. **`core` added examples pantry has not classified.** `core` merged
   `examples/valid/tenancy.account-scoped.yml` and `tenancy.honest-zero.yml`
   (core-15-tenancy). `tests/core_pin.rs` holds a table of non-manifest examples
   with the schema that governs each, asserts the classification is total in both
   directions, and a file nobody classified fails **by name** — the design
   working. pantry is correct to be red: two new documents exist in core that
   pantry has not decided what they are.
2. **pantry's core pin is 18 commits stale; the budget is 9.**
   `vendir.lock.yml` records `71d01fd`; `core` is at `9fac31e`. The
   `this_repository_says_how_far_behind_core_it_is` test prints the distance on
   every run and fails past 9 — a report red every week gets muted, so it is
   budgeted rather than exact.

**Proven pre-existing, not asserted.** I checked out this repository's own base
commit `92ca41e` — no `gate.yml`, no `bin/prime` change, no self-test — and ran
`cargo test` against the same sibling checkouts:

```
$ git archive 92ca41e | tar x -C baseline && cd baseline
$ CARGO_TARGET_DIR=<warm> cargo test --no-fail-fast
    every_example_in_core_s_valid_examples_is_classified_by_this_table
    this_repository_says_how_far_behind_core_it_is
```

The same two failures, at the base commit, before anything in this packet
existed. **They are not mine, and fixing them is not this packet's work.** The
first needs a decision about what a `tenancy.*` example is and which schema
governs it — a spec question. The second is a re-vendor: bump the sha in
`vendir.lock.yml` and `cp` the schema, in one commit.

**What this means for the merge.** pantry's gate is red for a reason that has
nothing to do with declaring it, so "the gate is green" is not a claim this
packet can make today. What it can say:

- the two failures are identical before and after this packet, verified at the
  base commit;
- all eight proofs are satisfied by a real green run of this gate, captured
  while it was green, and re-checked against the declaration;
- `bin/gate-self-test`'s **static** half is green in full and needs no gate run
  at all — 22 breakages, 3 warning cases, 0 failures;
- its **proving** half is blocked by the same two cross-repository failures,
  because a sandbox copy of pantry inherits them from the sibling `core`.

Fixing either is one commit in pantry and neither touches a gate declaration. My
recommendation is to do them before merging, so the merge is not the first moment
someone reads this repository's own red as being about this packet.

### A note on `core` being mid-merge

For part of this run `core/harness/cafaye_contract.py` carried **unresolved merge
conflict markers**, which made the gate checker itself unimportable — every
adopting repository's self-test would have failed at once, for a reason that has
nothing to do with any of them. I waited and re-ran against a clean `core`
rather than touching another worker's file. Worth the manager's attention: a
conflicted `core` is a fleet-wide outage of the gate tooling, and it is invisible
from any single repository.

The pinned-ref measurement (`core` at `71d01fd`, the sha in `vendir.lock.yml`)
was taken separately, and it is what §2's "with the workspace reachable" row
means — pantry reads core at a recorded ref, never at a working tree.

## 10. Open for the manager

- **`core`'s schema caps `gate.proof` at 8 and pantry wants a 9th.** The missing
  one is a floor on the test-file count, which needs a number in the output — one
  line of counting in `bin/prime`, then the proof. A `maxItems` bump is a core
  change.
- **`core`'s `external` cannot express "the skip count must be 0".** `minimum` is
  a lower bound, so the one number that matters here cannot be floored in the
  direction that matters. Worth a core decision.
- **The `build` CI job should call `bin/prime`.** A one-line change, and it is
  the local/CI drift this packet documents.
- **The floor ratchet is owed.** core enforces "raise the floor when you add a
  test" with `test_the_gate_floor_is_not_below_the_suite_core_claims`; pantry has
  no equivalent, so raising `total`'s 112 is a thing a human has to remember.

## The checks that were run

```
gate-check .                                  0 failures, 3 warnings
bin/prime                                     green, skips: 0, suite: 114
bin/gate-self-test                            35 breakages, 6 warnings, 0 failures, 0 skipped
```