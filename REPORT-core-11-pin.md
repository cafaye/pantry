# REPORT — core-11-pin

**Packet:** `core-11-pin` · **Branch:** `worker/core-11-pin` · **Date:** 2026-09-30
**Base:** `63f0b83` · **Commit:** `4188f97` · **Gate:** 14 suites, **114 passed, 0
failed, 0 skipped**, exit 0.

---

## 0. Finding first

**The gate said "pantry is broken" three times. Pantry was never broken.** Each red
was a commit landing in a repository nobody was working on, and nothing in the
failure said whose. That is one lesson wearing three coats, and the addendum is
right that there were three causes:

| # | cause | reported as | really was |
|---|---|---|---|
| 1 | `core-09` put two **gate declarations** into `core/examples/valid/`, and a test validated every `*.yml` there against the **manifest** schema | `"owner" is a required property` | a manifest rule quoting a document that was never a manifest |
| 2 | `identity-09` merged; `registry/services/identity/` was not refreshed | *pantry's copy is corrupt* | a comment-only staleness (11591 → 13302 bytes) |
| 3 | `muse-06` merged and made `identity` a **required** dependency; the copy said `required: false` | *pantry's copy is wrong* | the registry publishing a falsehood: a muse without identity is 503 on every request |

Causes 2 and 3 are the same shape: `registry/` is a copy of nine services'
manifests, checked against the sibling **working tree**. Cause 1 is the same shape
one repository over: pantry's schema test read core's **working tree**. So:

> **A copy whose test is "is the copy still current" can only ever fail for reasons
> outside the repository holding it.**

Everything below follows from taking that seriously.

### The red, before anything was changed

Run at clean `63f0b83`, nothing in the tree:

```
test result: FAILED. 12 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
test every_registered_entry_is_a_verbatim_copy_of_the_services_own_bytes ... FAILED
test every_registered_entry_matches_the_real_service_on_disk ... FAILED

2 registry copies are not byte-identical to the service they were copied from.

  identity
    copy   registry/services/identity/cafaye.yml (11591 bytes)
    real    identity/cafaye.yml (13302 bytes) — first differs at line 216
    every YAML field already matches — a COMMENT-ONLY drift.
  muse
    copy   registry/services/muse/cafaye.yml (2930 bytes)
    real    muse/cafaye.yml (6280 bytes) — first differs at line 49
    a YAML field moved too — this entry is not merely stale, it is WRONG.

assertion `left == right` failed: muse does not match …/muse/cafaye.yml
  dependencies[0].required — copy says false, real says true

test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
test the_schema_accepts_core_s_own_valid_examples ... FAILED

/…/core/examples/valid/gate.external.yml: does not satisfy cafaye/core's manifest schema:
"language" is a required property
"core" is a required property
"repository" is a required property
"owner" is a required property
unknown field "ci"; a cafaye.yml may declare only name, description, language, core,
exposes, consumes, dependencies, repository, owner

error: 2 targets failed:
    `--test drift`
    `--test schema`
PRIME_EXIT=101
```

Note the timestamps in the addendum are what settle the attribution and I did not
need to re-derive it: `pantry-07` ran at **18:21**, `core-09` authored at **18:13**
and merged to core's master at **20:01**. Pantry's copy drift was therefore
already red before `core-09` existed.

---

## 1. The three fixes

### 1.1 A consumer resolves at the ref it has vendored — MD15

**`src/pin.rs`** (new). Reads the two shapes a core pin really takes in this fleet:

- a `vendir.lock.yml` sha, or
- a `CORE_REF: <sha>` in `.github/workflows/*.yml`,

preferring the lockfile, in the same order and for the same reason as
`kit/tests/staleness.py`.

**Rust, not a shell-out to `staleness.py`.** I read that script and reused its file
shapes and its preference order rather than its code, for three reasons, and I am
naming them because "reuse that resolver, do not write a second one" is the
instruction and I did not follow it literally:

1. **CI cannot run it.** `workspace-drift` clones the fleet and not `kit`. A runtime
   dependency on a script in a repository this one does not clone would make the
   check unrunnable in CI — absent, not merely unverified.
2. **A report is not three functions.** What is needed here is *read a pin, read a
   blob at a ref, count commits between two refs*. `staleness.py` also does
   `--repos-dir` and GitHub-API discovery, and its docstring is explicit that it
   exits non-zero only for *unreadable* — a policy a test cannot inherit, since a
   test needs a missing pin to name what it searched for.
3. **The two must agree, and that is testable rather than assumed.** `tests/pin.rs`
   asserts the lockfile-preference and the disagreement semantics independently, so
   if `staleness.py` changes them the failure is visible in *one* of the two, not
   silent in both.

Three outcomes, all named, none silent:

| state | behaviour |
|---|---|
| a 40-hex commit | resolves, and says **which file shape** it came from |
| two pins, different commits | `Disagreeing` — a reported state, not a silent winner |
| no pin | `Missing`, carrying **every path searched** |

And `muse`'s real `CORE_REF: 'master'` — which exists in this fleet right now — is
**refused as a pin** and reported as a ref, not a commit. A branch is a question
that changes answer over time, which is the entire property this module removes.

**A working-tree fallback is deliberately absent.** Not "absent by oversight" —
absent because reintroducing it as a degraded mode is the one failure mode the
brief names explicitly, and it would make the green mean nothing while appearing to
fix the thing.

`vendir.lock.yml` is new and records this repository's own vendored schema at
core's `71d01fd`, with the byte-identity of that copy asserted on every run rather
than asserted by a comment.

### 1.2 Two document kinds — the decision, and where it is written

`core/examples/valid/` today:

| files | kind | governing schema |
|---|---|---|
| `*.cafaye.yml` (5) | service manifest | `cafaye.manifest.schema.json` |
| `gate.external.yml`, `gate.self-contained.yml` | **gate declaration** — `version`, `name`, `gate.command`, `gate.proof`, `external`, `ci` | `gate.schema.json` |

**The decision: consumers classify, and the classification must be total.**
`tests/core_pin.rs` holds a table naming every non-manifest example *and the schema
that governs it*, and two tests hold the table in both directions:

- a file that matches neither rule is a **FAIL naming the file** — so a third kind
  cannot be added without a decision;
- a table row whose file no longer exists is a **FAIL naming the row** — so core
  moving a file cannot leave a stale entry describing a document that is gone.

This is a *stopgap held honestly*, not the end state, and `DECISIONS.md` **D3** says
so: the recommendation to core is to **split the directory by kind**
(`examples/valid/manifests/` and `examples/valid/gates/`), because that is the only
option where the answer does not have to be re-derived per consumer. I cannot make
that change — it is core's tree, and `AGENTS.md` says this repository reads core
and does not own it — so it is recorded as a request with its reasoning and costs.

**The tempting one-line fix was checked and rejected, as instructed.** `ci` was
**not** added to `schemas/cafaye.manifest.schema.json`. `cafaye.schema.json`
describes a *service manifest*; a `gate.yml` is a different document with its own
schema. Widening the manifest schema to accept gate keys would make **every
consumer** accept a document it has no business accepting, and the error message
would then be a lie in the other direction — a gate declaration satisfying
`cafaye.manifest.schema.json` would be as wrong as the reverse.

pantry does not vendor `gate.schema.json` and does not validate gate declarations.
It states which schema governs them.

### 1.3 Registry copies record what they are a copy of

Every row in `registry/index.yml` gains `recordedAt`, and `tests/recorded_copy.rs`
checks each copy against **that commit** rather than the sibling working tree. The
claim changes from *"this copy is what the service says right now"* to *"this copy
is what service S said at ref R"* — which is true or false for a reason **inside
this repository**.

- a merge in `identity` no longer turns this gate red;
- a copy edited here, or a `recordedAt` bumped without re-copying, still does, and
  both are defects here.

**The copies were refreshed *and* staleness made visible**, per the addendum:

- `identity` and `muse` refreshed (the two `pantry-07` found);
- three `recordedAt` values bumped to a newer head whose `cafaye.yml` bytes are
  identical to the recorded ones (`core-10` merged into `cafaye-ts`, `darkroom`,
  `muse` while this packet was open — verified byte-equal before bumping);
- `the_registry_says_how_far_behind_each_copy_is_and_names_the_fix` prints every
  service's distance on **every run** and fails only past a **9-commit budget**, with
  the `cp` that fixes it.

Current: **9 current, 0 behind, 0 unmeasured.**

**Why a report and not a gate.** `kit/tests/staleness.py` says it in its own
docstring: *"a stale copy is LEGAL — it is a copy that has not been bumped yet —
and a scheduled report that is red every week is a report that gets muted."* MD15's
reason 3 agrees: making the frequent case a coordinated wave is a rule that gets
skipped the third time it is inconvenient. Nine commits is roughly a working day of
this fleet's merge rate — a person refreshes from the report; a copy past the budget
says the refresh has been missed long enough to stop for.

**A copy is still needed, and that is now a stated decision.** pantry has no
database, nothing persistent, no plugin loader, and `Registry::load` runs at
startup inside a container with no sibling checkouts and no network — serving the
registry requires the bytes. "Prefer resolving at a ref over copying" does not
survive contact with that, so `DECISIONS.md` **D4** says so rather than pretending
otherwise, and names who refreshes (whoever merges the change to a service's
manifest, same commit, with the `recordedAt` bump beside the `cp`) and when.

---

## 2. The proof each one is a fix and not a suppression

Every new check was broken deliberately and watched fail. Method: inject the old
behaviour or a specific fault, run, capture, restore.

| # | what I broke | what failed, and how it said so |
|---|---|---|
| 1 | injected the **old working-tree read** into the resolver | **3 tests** failed: `a_consumer_pinned_to_an_older_ref_validates_that_refs_examples` (`it yielded gate.external.yml, GateDeclaration…`), `a_consumer_with_no_recorded_pin_skips_and_names_the_missing_pin` (**"resolved 7 examples anyway — from where? A fallback to the working tree is the defect"**), and `resolving_at_a_pin_is_not_the_same_as_reading_the_working_tree` |
| 2 | injected the **original** rule — every `*.yml` against the manifest schema | `gate.external.yml: … "owner" is a required property / unknown field "ci"` — **the exact original failure, reproduced through the new harness** |
| 3 | `recordedAt: 35c2576…` for muse (an unreachable sha) | both copy tests: *"muse has no commit … locally. Either the recorded ref is wrong or the clone is shallow — `git -C …/muse fetch --unshallow`"* |
| 4 | flipped `required: true` → `false` in muse's **copy** | `dependencies[identity]: copy says … required: Some(false), recorded says … required: Some(true)` — the exact falsehood that shipped |
| 5 | dropped `gate.external.yml` from the table (**forward**) | `1 file(s) in core/examples/valid/ match neither rule … gate.external.yml` |
| 6 | added `slo.yaml` to the table (**reverse**) | `NON_MANIFEST_EXAMPLES declares "slo.yaml" but core/examples/valid/ … does not carry it` |
| 7 | `PRE_GATE_EXAMPLES` → core HEAD | `the_ref_under_test_really_is_a_ref_before_the_gate_examples`: *"was expected to predate the gate declarations"* — the fixture pin cannot silently stop meaning what it says |
| 8 | edited `schemas/cafaye.manifest.schema.json` | `is not core's schema at 71d01fd… (from vendir.lock.yml)` |
| 9 | a **shallow** fleet clone | 4 of 9 copies fail naming the exact `--unshallow` — which is why `--depth 1` had to go |

Break 1 is the important one and I will not shorten it: **the no-pin test caught the
fallback itself.** A resolver that quietly degraded to the working tree would have
passed every other test in the file.

### The stale-copy test, skipped rather than weakened

`a_consumer_with_no_recorded_pin_skips_and_names_the_missing_pin` uses a fixture
consumer with vendored bytes and no recorded origin — the real state of `caf` and
`pantry` when this started, so the normal path, not an edge case. It asserts the
**message**, not a shape:

```
no core pin is recorded in …/pantry-pin-no-pin-…. Searched …/vendir.lock.yml and
…/.github/workflows/*.yml. Vendored bytes with no recorded origin are not a pass:
nothing below describes the schema this repository was actually built against.
```

It also asserts the skip does **not** claim to have read examples it did not read
(no `core/examples/valid` in the message) — so a skip cannot grow into a lie about
what it checked.

### After

```
PRIME_EXIT=0
14 suites, 114 passed, 0 failed
skips: 0
```

**Pass and skip counts, separately, and measured rather than estimated.** A
pantry-only clone prints the **same 114 passing and the same exit 0** with **21
`SKIP` lines**, each naming the directory that would make it run. The README's
previous numbers were 89 passing and 11 skips; both were stale, and both are now
measured with the reproduction command included in the README rather than carried
forward. The stale count is why the count itself is part of the claim: `cargo test`
prints skips on stderr, which a terminal shows and a captured pipeline does not.

---

## 3. Things I changed that are not the obvious ones

- **`workspace-drift` no longer clones `--depth 1`.** `git show <sha>:<path>` cannot
  reach a commit a shallow clone does not have, so every recorded-ref check would
  become a skip — an honest one naming the ref it could not read, but a CI run that
  verified nothing about any registry copy while looking green. Measured, not
  assumed (break 9 above). `tests/ci.rs` holds this in both directions, and its
  existing `the_drift_job_clones_anonymously_and_needs_no_credential` **required**
  `--depth 1`, so that test was inverted in the same commit — a test that pins the
  defect had to change with it.
- **`tests/ci.rs` needed a comment-stripping helper.** The workflow's explanatory
  comments necessarily *name* the flag they argue against, so a substring check
  over the whole script can never pass. Both the check and the comment about why
  are in the file.
- **`manifest::validate_schema` keeps its signature.** `/readyz` must keep
  validating offline against bytes compiled into the container — a validator that
  resolved a ref at startup would make the readiness probe depend on a checkout the
  deployment does not have. `validate_against` is the new seam, and the two callers
  make two different claims; the module comment says so.
- **`tests/drift.rs` keeps a tombstone, not silence.** A reader who greps for
  `every_registered_entry_is_a_verbatim_copy` should find out where it went and why.
- **Three `recordedAt` bumps for commits that did not change the manifest.** `core-10`
  merged into three services while this packet was open. Their `cafaye.yml` bytes are
  identical, verified by hash before bumping — otherwise the copies would have been
  *behind* their own recorded refs, which is the failure mode this packet exists to
  remove.

---

## 4. What I could not verify

**Mandatory, and it is not empty.**

1. **I never ran `workspace-drift` on a GitHub runner.** There is no CI here and I
   did not create one. The `--depth 1` removal and the recorded-ref checks are
   verified against **local** clones — including a deliberately shallow one, which is
   the mechanism that mattered — but not against `actions/checkout` on
   `ubuntu-latest`, and I cannot confirm the full-history clones of nine repositories
   fit a hosted runner's time budget. **Treat the CI timing as unverified.**

2. **I could not run against the *real* GitHub remotes.** All my verification used
   local `file://` and path clones of `moon/cafaye/*`. Anonymous-HTTPS cloning from a
   hosted runner is asserted by an existing test, not by me.

3. **`cafaye-rb` is private and I could not read it.** Its exclusion row's reason is
   therefore still verified only by a developer's run, and my change does not alter
   that. I did not touch `CAFAYE_UNREADABLE`.

4. **`cafaye-py` has no repository.** Its row's check is vacuous in CI. `recordedAt`
   does not apply (it is excluded, not registered), but the vacuous-pass problem in
   `DECISIONS.md` D2 is untouched by this packet.

5. **I did not verify the pinned-ref path against a *shallow* `core`.** I verified it
   for the services (break 9) and reasoned that `core` behaves identically since it
   is the same code path — but `core`'s recorded ref happens to be HEAD in this
   checkout, so the specific combination was not exercised.

6. **The 9-commit staleness budget is a judgement, not a measurement.** I picked it as
   roughly a working day of this fleet's merge rate. I did not gather merge-rate data
   to justify it. If the fleet's rate differs materially, the number should move —
   it is one constant, named, in `tests/recorded_copy.rs`.

7. **Two document kinds is what exists *today*.** My classification is total against
   core HEAD as of this commit. If core's `examples/valid/` gains a third kind, the
   test fails with the filename (proven, break 5) — but I have not predicted the
   shape of that third kind or argued for how it should be classified.

8. **I did not measure whether the fleet's other consumers have the same defect.**
   `docs` and `muse` read core through `CORE_REF`; whether their checks resolve at
   that ref or at the working tree is a question about *those* repositories, and I
   did not open them. `kit`'s `staleness.py` reads the pins correctly, but a reporter
   is not a consumer.

9. **The `vendir.lock.yml` I wrote is hand-maintained, not vendir-written.** It is
   the honest name for the record and `pantry::pin` only requires a 40-hex sha, but
   nothing in CI proves it is updated when core moves — a human does it, with the
   `cp` and the `rev-parse` both written in the file's own header. A re-vendor job
   would be a better answer and is not in this packet.

10. **I did not run the gate under a non-`bash` shell.** `bin/prime` is `#!/usr/bin/env
    bash` and the packet's discipline is `set -o pipefail` reading `${PIPESTATUS[0]}`
    — which I did. I did not test zsh, and I know from the fleet's history that a
    pipe's exit code is exactly how this once got a false green.

11. **Two stale numbers in `README.md` were corrected, but I have not audited the
    whole file for others.** I measured the pass and skip counts and fixed those. A
    claim in the README I did not re-measure may still be stale.

12. **`target/` is 1.1 GB** (up from ~880 MiB) because the new test binaries were
    built. I have left the worktree clean of tracked artifacts; I have **not** deleted
    `target/`, since doing so means the next `./bin/prime` pays a full rebuild and
    deleting it is a judgement for the manager. Flagging rather than deciding.
