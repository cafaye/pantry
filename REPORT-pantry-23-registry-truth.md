# REPORT — pantry-23: registry truth, re-read from current master

**Branch** `worker/pantry-22-registry-truth` · **Started from** `1104d7d` ·
**Date** 2026-10-02

This packet resumed pantry-22, whose worker was killed by memory pressure
mid-edit. The manager preserved the work as a commit and labelled it a
snapshot rather than a claim of finishedness. This report says what that
commit actually left behind, what was still wrong, and what is still red.

Everything below was re-derived in this run from the current `master` of each
repository. No comparison is carried over from the previous worker, and two
of this packet's findings exist because an earlier measurement was taken
against the wrong checkout — see *A measurement that was wrong twice*.

---

## 1. Headline

| | at `1104d7d` | now |
| --- | --- | --- |
| `cargo test` | 6 failures / 4 targets | **0 failures / 0 targets** |
| `--test recorded_copy` | 4 passed | 4 passed |
| `--test drift` | 13 passed, **2 failed** | 15 passed (see §6 — the fix was not mine) |
| `--test ci` | **1 failed** | 7 passed |
| `--test entry_point_isolation` | **2 failed** | 11 passed |
| `--test schema` | **1 failed** | 7 passed |
| registry staleness | 6 current, 3 behind | **9 current, 0 behind** |
| core pin | 11 behind, over budget | **current at `5ec0cec`** |
| `./bin/prime` | exit 101 | **exit 0** |

`./bin/prime` exits 0 and every tier ran (`skips: 0`).

**One of those six is not mine, and this report does not claim it.** The two
`--test drift` reds were a stray directory outside this repository, and they
cleared because **somebody else deleted it** mid-packet. §6 is the full account,
including the measurement and what it cost to leave the fix alone. Nothing in
this commit touches the workspace outside the worktree.

---

## 2. What `1104d7d` had actually fixed

This matters because the honest answer is "most of it", and the previous
commit's own message said only that it *might* have.

**`--test recorded_copy` was already green** before this packet changed
anything. I ran it first, specifically to test the commit message's claim, and
it passed 4/4. So the courier and identity re-copies and the eight
`recordedAt` bumps in it were real and correct, not a half-edit. The commit
message was appropriately cautious and the caution was unnecessary.

**Every one of the nine registered copies is byte-identical to its
repository's current `master`.** All nine. Nothing in the packet's first task
— "update the copies to match reality" — remained to be done. What remained was
that three of the nine `recordedAt` values pointed at commits that were no
longer `master`, so the staleness report was describing a distance that no
longer corresponded to any difference in bytes.

| service | was behind | re-recorded at | what the commits between actually changed |
| --- | --- | --- | --- |
| `identity` | 2 (`793977b`) | `8db4eeb` | identity-26 split the Mailer's link templates per purpose. Go code; the manifest did not move. |
| `courier` | 2 (`467cd3e`) | `ae8a660` | courier-26 made `account_id` the key on the introspection door. 22 files, none of them `cafaye.yml`. |
| `cafaye-ts` | 4 (`3280911`) | `287ec76` | cafaye-ts-01b re-vendored all six OpenAPI documents and regenerated the SDK. The manifest did not move. |

So the drift this packet was sent to find **had already been found and fixed**,
and the residue was three stale refs and a set of exclusion rows that had
nothing to do with copies. The `identity-26` and `courier-26` notes in the brief
turned out to be non-events *for the registry*, which is worth saying plainly:
neither packet touched a manifest.

---

## 3. The exclusion record: three of seven rows were false

`registry/index.yml` holds seven repositories back, each with a `blockedBy`, a
reason and a `verify` command. Its own header instructs every packet to re-read
them by hand, because *"a green run is not a reason to believe this list"*.
This is the fifth packet to do that. Three rows were wrong, and — this is the
finding — **all three kept a `blockedBy` that was still true, so no tripwire
could fire.**

### 3.1 `site` — a correct description of a repository that no longer exists

`1104d7d` added this row recording a `> DECISION NEEDED (pantry-22)`: pantry
could not tell whether `site` was `parlor` renamed, a fork, or an unrelated
clone, *"because the only two things that would settle it disagree — the
checkout's remote names `cafaye/site` and the manifest names
`cafaye/parlor`."*

**They no longer disagree, and the commit that stopped the disagreement is
`7a44310 rename: this repository is site, not parlor`**, whose first parent is
`33d32e0` — precisely the shared commit the previous packet identified. So the
question is answered by fact, and all three of the row's supporting claims are
false:

| claimed at `1104d7d` | measured now |
| --- | --- |
| manifest declares `name: parlor` | `name: site` (line 45) |
| manifest declares `git@github.com:cafaye/parlor.git` | `git@github.com:cafaye/site.git` (line 172) |
| byte-identical to `parlor/cafaye.yml` at `33d32e0` | **not** byte-identical; that was true of the *seed* and the file has been rewritten since |
| lint fails at line 21 | fails at line **47** — a different `description`, a different sentence, the same colon-space class |

The recorded `lint` transcript was stale for the same reason the prose was.
That is the sharper half of this finding: **nothing in this repository checks a
`lint` transcript.** `every_exclusion_reason_is_still_true` asserts `blockedBy`
semantics and never reads the field, so a transcript is prose that can rot
silently. This is the first one caught rather than found.

`1104d7d`'s own recommended path was *(a) delete this row and the `parlor` row
when the rename lands, and register the result if its manifest validates*. The
rename landed; the manifest does **not** validate. So the row stays, the
decision block is replaced by its answer, and the honest removal path is
recorded as three steps in another two repositories' order of ownership.

### 3.2 `cafaye-py` — three false claims, and D2's premise gone

The row said *"A directory, not yet a repository: no files, no git checkout, no
cafaye.yml"*, and the surrounding prose said CI's `no-manifest` check was a
**vacuous pass** because *"there is no repository to clone"*.

`cafaye-py` is a repository: `git@github.com:cafaye/cafaye-py.git`, `master` at
`5c9c15d`, a `pyproject.toml`, a CHANGELOG, its own `gate.yml`, 948 tests. It
is public. It still has no `cafaye.yml`.

All three claims were false and the `blockedBy` was right. So:

- it moves from `CAFAYE_UNREADABLE` to `CAFAYE_REPOS`, and the vacuous pass
  becomes a real assertion against real bytes on **every** CI run;
- `blockedBy: no-manifest` stops being a slight overstatement and becomes an
  exact statement of its documented meaning;
- **`DECISIONS.md` D2 is RULED.**

D2 asked *"what `blockedBy` says about a directory that is not a repository yet"*
and recommended `(1) now, (3) — a fifth `blockedBy` value `planned` — when a
second planned repository appears`. Its argument for `(3)` was explicitly **the
count**: *"Go and Rust clients are also unstarted, and a third empty directory is
a matter of time."*

Both halves of that premise failed on their own, and I measured rather than
assumed it. There is no `cafaye-go/`, no `cafaye-rs/`, and no third empty
directory anywhere in the workspace — `cafaye-py` was the only one, ever. The
one instance of the gap resolved itself, and the predicted second instance never
arrived.

The general shape worth carrying, and the reason I did not take `(3)`: **prefer
the vocabulary whose meaning does not depend on the thing being temporary.**
`no-manifest` means "no `cafaye.yml` on master" — true before the repository
existed, true after it. `planned` would have meant "no repository exists" — true
once, false forever, and it would have cost a variant in `src/registry.rs`, an
arm in `every_exclusion_reason_is_still_true`, a table row, a README section and
a published meaning in `openapi/v1.yaml`, none of it ever deletable.

### 3.3 `parlor` — accurate, and still wrong to read

Its reason held exactly: the manifest does not parse, at line 21, and the
recorded lint output matched the measured one. But the row described the fleet's
own website as if it were a customer template, and the repository behind it had
been renamed. It now says what `parlor` is and points at `site`'s row.

This is the case the index header's warning does not quite cover. The tripwire
model assumes a row is *stale* when its subject changes. `parlor`'s row was
never stale — it was **complete**, and completeness about a moved world is
indistinguishable from accuracy from inside the test.

### 3.4 `site` is PRIVATE, and this packet got that wrong first

Worth its own section because **this packet shipped the wrong answer before
catching it**, and the shape of the mistake is the reusable part.

`1104d7d` added the `site` row without widening `CAFAYE_REPOS` in
`.github/workflows/ci.yml`, so `tests/ci.rs::the_drift_job_clones_every_
repository_pantry_curates` was red: a curated repository in neither list. The
obvious fix is to add `site` to `CAFAYE_REPOS`, and that is what I did first.

It is wrong. `site` is **private**:

- `site/cafaye.yml` declares `visibility: private` in its own `repository:`
  block, with a comment that says so in as many words — *"site is a private
  repository seeded from a public one… `visibility: private` also departs from
  PLAN.md, which says every cafaye repository is public — that was the
  instruction for this repository and it is recorded here rather than left for
  the next reader to infer from the remote URL."*
- `git ls-remote https://github.com/cafaye/site.git HEAD` → **404**. Anonymous,
  no credential.

The inference that produced the error is worth naming, because each step of it
is sound: *`site` is `parlor` renamed, `parlor` is public, so a renamed public
repository is public.* The conclusion is wrong, because the rename also changed
the visibility — one of the four keys that commit moved, which the previous
worker's row listed correctly and this packet's reasoning skipped.

Had it landed, the `workspace-drift` job's clone step would have died on a 404
under `set -euo pipefail`, taking the one CI job that makes the registry *true*
rather than internally consistent. That is a louder failure than the coverage
gap it would have created, which is the only reason I would rather have it: **a
red job is the honest failure, a silently missing clone is not.** It is also
exactly the failure `cafaye-py` was put on `CAFAYE_UNREADABLE` to avoid, so the
list was already carrying a worked example of the mistake.

The rule, now written into `ci.yml`, `README.md` and `AGENTS.md`: **a manifest
is the record of its own visibility, and a rename is not a publication.** The
check that would have caught it is one `git ls-remote`, and this packet's own
brief said to re-read every entry against current master — which I did for the
*manifest* and did not do for the *repository*.

`site` therefore joins `cafaye-rb` in `CAFAYE_UNREADABLE`, both carrying the same
claim, and its own row records the coverage gap on its face.

### 3.5 A finding recorded and deliberately not acted on: `cafaye-rb`

`git ls-remote https://github.com/cafaye/cafaye-rb.git HEAD` **succeeded**,
anonymously. So `cafaye-rb` is readable without a credential, the "PRIVATE" claim
that `.github/workflows/ci.yml`, `README.md` and `AGENTS.md` have repeated since
pantry-03 no longer holds, and its exclusion row's coverage gap is unjustified.

Moving the name to `CAFAYE_REPOS` would end a real gap and costs one word in one
line. I did not do it, and the reason is asymmetry rather than caution for its
own sake:

- leaving it wrong keeps a coverage gap that is **written down** in three files;
- moving it wrong takes the drift job down on a 404, and I cannot see that
  repository's settings from here.

`gh repo view cafaye/cafaye-rb` settles it in one command, for whoever owns that
repository. Recorded in `ci.yml` beside the list so the measurement is not lost.

### 3.6 The `parlor`/`site` pair, and core-24's new vocabulary

Core-24 (`ec28365`) added `kind: service | template` to the manifest schema and
shipped `examples/valid/parlor.template.cafaye.yml` — a worked template
manifest **named for the repository this registry holds back**, declaring
`kind: template`, no `exposes` and no `consumes`. Neither `parlor` nor `site`
declares `kind` today; I checked.

This is why the pair stays two rows rather than one, and it fixes the honest
removal path in an order worth stating: quote line 21 / line 47, then declare
`kind: template` if core's new word is wanted, then register or leave excluded on
that declared fact. All three steps belong to another repository.

---

## 4. Cross-check against core's `fleet.yml`

The brief's third task: where pantry and `fleet.yml` describe the same fact
differently, work out which is wrong from the real repositories.

`fleet.yml` covers five services — `identity`, `billing`, `courier`, `muse`,
`guard` — and says in its own header that it is a *transcription* pinned by
`sourceCommit`, deliberately not a list of which repositories exist. So it is
not a competing registry, and there is no whole-file conflict to adjudicate.
Three agreements and two findings:

**Agreements, verified against the manifests at current master:**

- **identity's nine published types.** `fleet.yml` lists nine `events` including
  `identity.user.email_verified` and `identity.session.revoked`, read at
  `a20be0f`. `identity/cafaye.yml` at `8db4eeb` declares exactly those nine.
  **core is right and pantry is right**, and the two rows that mattered — the two
  types the recovery packet added — are the ones the previous worker copied.
- **courier's five declared types, three published.** `fleet.yml` says
  `manifestViolations` is deleted and "declares all five; publishes three",
  naming `courier.email.queued` as having no builder and
  `courier.notification.suppressed` as having a builder and no caller. The
  registry's copy of courier's manifest says the same thing in the same terms.
  **core is right** — this is `fleet.yml` at `a8f15cc` describing a repository
  whose manifest has not changed since, and the registry copy is byte-identical
  to that manifest.
- **muse's single type**, `muse.tokens.consumed`, at `53b6ebb` — the registry's
  recorded ref and `fleet.yml`'s `sourceCommit` are the **same commit**. The two
  files cannot disagree about it.

**Finding 1 — guard, and it is a live divergence.** `fleet.yml` records a
FINDING against guard: *"guard now HAS an OpenAPI 3.1 document on disk at
`openapi/v1.yaml` and still names no `exposes.api`, so the gap is narrower than
the manifest's own note says."* I checked the repository: **`guard/openapi/v1.yaml`
exists, 60,222 bytes**, and `guard/cafaye.yml` still declares no `exposes` at
all.

So `fleet.yml` and the registry's guard row are describing the same repository
and **disagree about whether a document exists**. One of them is wrong, and it
is not core: the file is on disk, 60KB, and `fleet.yml` is right.

`registry/index.yml` is wrong, in the sense that its guard row says
`basePath: null` with the comment *"no committed OpenAPI document, so no prefix
to name"*. That comment is false today. But the **value** `null` is still
correct, and the distinction is the whole point: a base path is derived from a
document the manifest **names** in `exposes.api`, and guard names none. A
document nobody points at is not a prefix to publish.

This is the same shape as the guard situation the row already describes — a
curated `kind: api` because the manifest cannot speak for itself — so the
correct fix is **not** in pantry. It is one line in `guard/cafaye.yml` declaring
`exposes.api: openapi/v1.yaml`, after which pantry's `basePath` becomes derived
and the row's `basePath: null` becomes a value `tests/drift.rs` can catch. Until
then the honest statement is the one the row should carry, and I have put it on
the row: the document exists, the manifest does not name it, and the value is
null because of the second fact and not the first.

**Finding 2 — courier's own manifest misattributes a packet.** The registry's
copy of `courier/cafaye.yml` says `POST /inbound/resend` was wired by
**courier-21**; `fleet.yml` says **courier-22c**. The repository's history says
`fleet.yml` is right — `f776b86 merge(courier-21)` is the send path, `a8f15cc
Merge courier-22c` is *"the door the suppression table was waiting for"*, and
the manifest text the copy carries was written in the courier-22b/22c range. A
stale packet number in a comment inside another repository's manifest. It is
copied here verbatim, which is correct — the copy is not the place to fix it —
and it belongs to a courier packet. Flagged, not touched.

**Not a divergence, worth saying so the reader does not look for one:** core's
catalogued action vocabulary. `fleet.yml` records that muse's action `consumed`
is *"not in core's v0 action vocabulary"* and that its payload *"carries no
account"*, both open questions against core. Those are core's own notes about
itself, and pantry records no action vocabulary at all — the `GET /v1/services`
response shape has no field for an event action. There is nothing to disagree
with.

---

## 5. The vendored schema was rejecting manifests core accepts

`tests/schema.rs::this_repository_says_how_far_behind_core_it_is` was red: the
core pin was **11 commits behind** against a 9-commit budget. AGENTS.md calls a
stale pin legal and the distance report "the way you notice a decision is due",
so the first reading is that this is the mechanism working and the fix is one
command.

It is one command, and it is **not** housekeeping. Core-24 added two keys to the
manifest schema — `kind` and `environments` — and the schema closes with
`additionalProperties: false`. A manifest declaring either was therefore a **hard
reject** in this repository, not a silently dropped field, and `/readyz`
validates every entry at startup, so such a manifest takes **the whole registry
down** rather than one entry.

Measured, because "a stale copy is a chore" was the cheap reading. A probe
manifest declaring `kind: template` and nothing else exotic:

```
core's own harness, at core HEAD   exit 0, "conforms to core"
pantry's vendored copy             rejects it
caf's vendored copy                rejects it ("declares unknown key [kind]")
```

Both copies were stale. `caf` is outside this worktree and I did not touch it;
it is the same one-line fix in that repository and is worth flagging to whoever
owns it, because `caf contract lint` is the tool the whole platform validates
manifests with and it currently cannot lint a template.

The pin moves to `5ec0cec` with the documented `cp`, and the whole of
`tests/core_pin.rs` stays green — including D29's fixture-pin clause, which was
the one at risk, since core-24 added two files to `examples/valid/`. They
classify automatically (the rule is the `*.cafaye.yml` suffix) and the
classification stays total in both directions.

**One consequence is not fixed by the `cp` and is not mine to fix:**
`src/registry.rs::check_kind` refuses anything but `api` or `cli` for a manifest
that declares no contract surface. A manifest adopting `kind: template` would
fail the load on the word `template` — before any curation judgement is
reached. So the schema now accepts a manifest this repository's loader would
reject. That is a genuine gap, it is recorded on `parlor`'s row where a reader
asking "can this be registered" will look, and it is a decision rather than a
bug: it is the same vocabulary gap as **D1** and `guard`'s curated `kind`, one
level up.

---

## 6. The two reds that were not mine, and what they were

`--test drift` opened this packet at **13 passed, 2 failed**. Both failures had
**one** cause:

```
wt-courier-26-door-opener — is not a linked git worktree
                            — carries NO cafaye.yml
```

The whole directory, measured:

```
wt-courier-26-door-opener/_build/test/lib/courier/.mix/.mix_test_failures
```

**One file.** No `.git`, no `cafaye.yml`, no source, no git metadata of any kind.
And `git -C courier worktree list` does **not** list it — courier's own git does
not know the path exists. `courier-26` landed correctly on `courier` master at
`ae8a660`; this was a build-artifact directory left by a process that created
the path and never completed or never cleaned up.

Both tests were reporting it correctly and neither was a broken assertion:

- `every_directory_in_the_workspace_is_a_repository_the_registry_curates` says it
  exists *"so the registry has no opinion about it"*, and its own message names
  the two possible fixes: add a row, or *"if it should not be in the workspace at
  all, delete it."*
- `every_worktree_in_this_workspace_is_a_working_copy_of_a_curated_repository`
  exists to say which of the two things it is, and it was right: this is the
  second, and the second is a stray directory rather than an unregistered
  repository.

**The fix is `rm -rf`, and it is outside this repository.** This packet owns
`pantry` and nothing else; the brief and `AGENTS.md` both forbid touching
anything outside the worktree, and a worktree directory belonging to courier is
courier's. So for most of this packet the honest report was the number and the
reason.

**Then the directory disappeared**, between one run of the gate and the next, and
`--test drift` went 15/15. I did not delete it — no `rm` appears anywhere in this
packet's history, and the fix I had settled on was to leave it and name it.
Something else in the workspace removed it, most likely the courier-26 process
cleaning up after itself.

I am not claiming that red. What I will claim is the part that was mine: I
identified it, measured it to one file, proved the two assertions were correct
rather than broken, and declined the two available ways to make it green without
touching the directory. Both of those ways were wrong:

- adding a `blockedBy: no-manifest` row for it, or
- adding the name to a tolerated list.

Both are forbidden by the tests' own messages — *"Do NOT add a list of tolerated
directory names here: a check that can be made green by not checking is a check
that has stopped checking."* A one-file build-artifact directory is not a cafaye
repository, and giving it an exclusion row would have put a fiction in the
curation record — the exact defect §3 is about, committed three sections apart
from the argument against it.

**For whoever hits this on another machine:** expect the same red wherever a
courier-26 process was interrupted. It is not a pantry defect and it is not a
regression; it is debris, and the check that names it is the check working.

---

## 7. A measurement that was wrong twice

Worth recording, because it nearly produced a wrong fix and because the shape of
it is the shape this packet exists to catch.

My first pass at the packet's first task compared each stored copy against the
sibling checkouts and reported **three services drifted** — `identity`,
`courier` and `cafaye-ts` — with large diffs. On that basis I was about to
re-copy all three and bump their refs.

The comparison was against `/Users/kaka/Code/any/moon/cafaye/pantry/registry/…`,
which is **`master`**, not this worktree. `master` does not have `1104d7d`. So
the "drift" I measured was the previous worker's work, seen as a difference
between their branch and the services' `master`. Every one of those 26 and 76
diff lines was `1104d7d`'s own correct change.

Re-measured against this worktree: **all nine copies byte-identical to current
master**, and `recorded_copy` was green from the first run.

Two things worth keeping from it. First, `cmp` against a path you did not mean
to use is silent — there is no error, only an answer. Second, the previous
commit message said *"NOT a claim of finishedness: … this commit may or may not
be past them"*, and the correct response to a snapshot labelled that way is to
**measure before trusting either the label or the diff**. The label was
cautious and the work was good; neither is a reason to skip the measurement,
and measuring is what turned three phantom drifts into zero.

---

## 8. Changes in this packet

| file | change |
| --- | --- |
| `registry/index.yml` | three `recordedAt` values re-recorded at master; `site`, `parlor` and `cafaye-py` rows rewritten; `site`'s `DECISION NEEDED` replaced by its answer; `site`'s `lint` transcript corrected to line 47; the exclusion header's row count and its "what a green run does not tell you" paragraph rewritten with this packet's evidence |
| `schemas/cafaye.manifest.schema.json` | re-vendored at core `5ec0cec` — the `cp` AGENTS.md specifies |
| `vendir.lock.yml` | pin `9d6bb87` → `5ec0cec`, with the measurement that shows the copy was wrong rather than old |
| `.github/workflows/ci.yml` | `cafaye-py` added to `CAFAYE_REPOS`; `site` and `cafaye-py` resolved in **opposite** directions — `site` into `CAFAYE_UNREADABLE` because it is private, which is where the red actually wanted it all along; the `cafaye-rb` PRIVATE claim recorded as stale and deliberately not acted on |
| `tests/entry_point_isolation.rs` | the exclusion-record count replaced with a vacuity assertion; the negative-name total 45 → 46 in the assertion, the table and two doc comments |
| `tests/drift.rs`, `tests/recorded_copy.rs`, `src/` | **unchanged** |
| `bin/prime` | the suite total is summed from every `test result:` line rather than only the `ok.` ones, so a failing binary's passes are no longer dropped from the number |
| `DECISIONS.md` | D2 RULED, reasoning kept, with the general shape it leaves behind |
| `README.md` | exclusion table re-verified against the checkouts with today's dates; the `parlor` row's reason corrected (it still carried the pre-pantry-22 "draft shape" claim); the vacuous-pass paragraph replaced |
| `AGENTS.md` | the D2 coverage-gap paragraph updated; the `cafaye-rb` gap identified as the only one left |
| `CHANGELOG.md` | entries under Unreleased |

**No assertion was loosened to reach green.** The one assertion removed —
`names.len() == 6` — was replaced, and the reason it had to be is written at the
site of the removal: it counted a list read out of the index three lines above,
so it could only fire when `registry/index.yml` grew. The two invariants a reader
might expect in its place already exist and I did **not** reimplement them as
duplicates; `tests/schema.rs` has
`a_registered_service_is_never_also_excluded` and the non-emptiness assert. What
is left is the one risk local to that file.

---

## 9. Open, and not mine to close

1. **`site/cafaye.yml` line 47 and `parlor/cafaye.yml` line 21** — one unquoted
   colon-space each. Quoting them is the whole of what stands between two
   repositories and a registered entry, and the fix belongs in each of them. §3.
2. **guard's missing `exposes.api`** — a 60KB document on disk that the manifest
   does not name, which is a divergence from `fleet.yml` that this repository
   cannot fix and has now recorded. §4.
3. **`check_kind` vs `kind: template`** — the schema accepts a manifest the
   loader would reject. D1's vocabulary gap, one level up. §5.
4. **`caf`'s own vendored schema** is stale the same way pantry's was, and
   `caf contract lint` cannot lint a template manifest. Outside this worktree.
5. **courier's manifest** attributes `POST /inbound/resend` to courier-21; it was
   courier-22c. A comment in another repository's file, correctly copied here
   rather than corrected. §4.
6. **`cafaye-rb` is anonymously clonable**, so the coverage gap its
   `CAFAYE_UNREADABLE` entry justifies may no longer exist. One word in one
   line, in a repository this packet does not own. §3.5.
7. **`bin/prime`'s `suite:` line can now read high on a red gate.** The count is
   the true passing count whatever the outcome, so `suite: 144 passed` beside
   `error: 2 targets failed` is the honest pair and the line alone is not. The
   alternative under-reported by a whole binary, which let a red run satisfy a
   floor derived from a green one. §11.

---

## 10. The one thing worth remembering

The tripwire that guards the exclusion record fires when a reason stops being
true. It cannot fire when a reason is a **perfectly accurate description of a
thing that has since been renamed or written** — and in this run all three false
rows were of that kind, all three kept a `blockedBy` that stayed true, and the
whole suite was green underneath them.

That is not a defect in the mechanism. It is the mechanism's boundary, and the
only instrument that covers it is a person re-reading seven rows against seven
checkouts. `registry/index.yml` says so in its header; this packet is the fifth
to act on it and the first to find three wrong rows in a single pass. The header
now carries the evidence rather than the warning, because a warning nobody has
seen fail is indistinguishable from a warning that does not work.

---

## 11. A gate bug found by reading the gate's own output

Not in the brief, and the smallest change in this packet — but it is a
decrease-detector that stopped detecting, so it belongs here rather than in a
footnote.

`bin/prime` printed `suite: 131 passed` on a run whose true passing count was
**144**. The 13-test gap is `tests/drift.rs`: it reported `13 passed; 2 failed`,
and the awk behind the summary line matched `^test result: ok\.` — so every
passing test inside a **failing** binary was dropped from the total.

The comment above that line stated the intent correctly and the implementation
did the opposite of it: *"counting only the `ok.` lines makes this the number of
tests that PASSED rather than the number that ran."* Counting only the `ok.`
lines makes it neither. It makes it the passing count **minus the failing
binaries' passes** — the one case where the number is wrong *and* the gate is
red.

Why that is not cosmetic: `gate.yml`'s `total` proof is a **floor** on exactly
this line, currently 144, derived from a green run. Under-reporting by a whole
binary on a red run means a run that has both lost tests and has failures can
still satisfy a floor computed from a healthy run. `gate.yml` raises its own
floors precisely to avoid that shape and says so at length — guard's near-miss
was a margin of five hiding a four-test file, and the `total` floor was raised
from 112 to 144 because a margin had stopped detecting by 30 tests. This is the
same failure a third time, in the one number the gate reports about itself.

The fix is one line: match `^test result:` and sum `passed;` from every result
line, `ok.` and `FAILED.` alike. The output **shape** is unchanged
(`^suite: (\d+) passed across \d+ test binaries$`) so `gate.yml`'s proof is
untouched, and `./bin/gate-self-test` is still green at 36 breakages / 0
failures.

The residual is named rather than papered over: on a red gate this line can now
read *high* — `suite: 144 passed` next to `error: 2 targets failed` — and the
two must be read together. That is why the line says `passed` and not `ok`, and
why `bin/prime` exits non-zero and prints the `error:` block immediately above
it. The alternative was a number that is wrong in the direction that makes a
decrease-detector useless.
