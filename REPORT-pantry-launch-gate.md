# pantry launch gate — the re-pin cannot be done, and there are two more reds

**Bottom line.** The fmt failure is fixed and committed. The clippy failure — which
the brief did not list and which sits *before* the tests in the gate — is fixed and
committed. The `core_pin` failure is **not** fixed, because the ref the brief asks
me to find **does not exist**, and inventing one is the exact failure mode this
repository is built to avoid. I stopped on that question as instructed. `./bin/prime`
exits **101**, for three independent reasons, two of which are not in this
repository's favour to fix silently.

Two commits, two facts, no re-pin:

```
203fcd3 clippy: the gate was red here too, and the brief did not say so
dc74708 fmt: the tree was not rustfmt-clean, in three test files
```

`gate.yml` is untouched. I added no tests, so no floor moved.

---

## 1. `cargo fmt` — what changed, and the proof it was only formatting

`cargo fmt --check` exited 1 on `tests/api.rs`, `tests/core_pin.rs`,
`tests/filters.rs`. Ran `cargo fmt`, then read the whole diff. It is line-wrapping
(four constructs collapsed onto one line, three chains broken across lines), one
comment re-indented into the block it belongs to, and **two trailing commas
removed**.

The trailing commas are the only non-whitespace characters in the diff, and they
deserve a sentence because they are not a detail: `tests/api.rs:444` and
`tests/filters.rs:210` each lost a `,` before a `)`. A trailing comma before a
closer is syntactic sugar that exists so a construct *can* be written across lines;
rustfmt drops it precisely because it has just collapsed the construct to one line.
Same value, same type, same evaluation order.

**The instrument matters here.** `git diff -w` is *not* a valid check for this and
it reports all three files as changed — `-w` ignores whitespace within a line but
not line breaks — so it would have sent a reviewer hunting for a semantic edit that
does not exist. The check that actually proves it is: delete all whitespace, and
delete every trailing comma before a closer, from both the committed content and
`HEAD`'s content, and compare hashes.

```
tests/api.rs      EQUIVALENT modulo (whitespace + trailing-comma)   sha=e2879ebef1b0f532
tests/core_pin.rs EQUIVALENT modulo (whitespace + trailing-comma)   sha=535bba8c61a948c8
tests/filters.rs  EQUIVALENT modulo (whitespace + trailing-comma)   sha=775a2f4d04667c9
```

A caution for the record, because I got it wrong first and it nearly became a false
finding in this report: my first attempt at that comparison was written with BSD
`sed`, and it reported all three files as *still differing* — including
`tests/core_pin.rs`, which the same script had just proven was whitespace-identical.
Two deterministic functions cannot disagree on identical inputs, so the sed
normalization was wrong, not the files. Redone in Python it agrees. If you are
checking my arithmetic, re-run it in Python; do not trust BSD `sed` here.

Measured after the change: `cargo fmt --check` exits **0**; `cargo test --test api
--test filters` is **26 passed** and **13 passed**, 0 failed.

---

## 2. The re-pin: no satisfying ref exists. This is the finding, and I stopped here.

I did not change `PRE_GATE_EXAMPLES`, and I did not touch its header comment.

### The measurement

Core HEAD is `bd62a2e8614b15bbabc283396888cf1250e95945`, and
`schemas/cafaye.manifest.schema.json` at HEAD hashes to
`8d712bd574fd4846c5fa49b3750c89e8b36f800a0272185355603a391854a95d`.

`git log --oneline -- schemas/cafaye.manifest.schema.json` returns exactly three
commits in core's entire history: `a6bbd28`, `a463e7c`, and `ec28365`. Since
`ec28365` is the *last* commit to touch the schema, the set of commits whose schema
is byte-identical to HEAD's is exactly `ec28365` and its descendants.

`git rev-list --all` and `git rev-list HEAD` both return 74, so the walk below covers
every commit reachable from every ref in core — `master`, `origin/master`,
`origin/worker/core-09` and the `v0.2.0` tag are all inside that 74. There is no
unreached commit hiding the answer.

The probe, run in `/Users/kaka/Code/any/moon/cafaye/core`:

```sh
HEADSHA=$(git show 'HEAD:schemas/cafaye.manifest.schema.json' | shasum -a 256 | cut -d' ' -f1)
for c in $(git rev-list --all); do
  s=$(git show "${c}:schemas/cafaye.manifest.schema.json" 2>/dev/null | shasum -a 256 | cut -d' ' -f1)
  [ "$s" = "$HEADSHA" ] || continue
  tpl=$(for f in $(git ls-tree --name-only "$c" examples/valid/ | grep '\.yml$'); do
          git show "${c}:${f}" | grep -qE '^\s*kind:\s*template' && echo "$f"
        done)
  [ -z "$tpl" ] && echo "  SATISFYING REF: $c" || echo "  $c schema=MATCHES but has template: $(echo $tpl)"
done
```

Output, in full:

```
  bd62a2e8614b15bbabc283396888cf1250e95945 schema=MATCHES but has template: examples/valid/parlor.template.cafaye.yml
  a62c9d8520a6acd43d3fcbec025bda605b2f8a73 schema=MATCHES but has template: examples/valid/parlor.template.cafaye.yml
  ec2836503b03948e9e6833cad6b8603950135103 schema=MATCHES but has template: examples/valid/parlor.template.cafaye.yml
```

**No `SATISFYING REF` line. The two sets are disjoint.** Three commits have a
HEAD-identical schema; all three carry a template manifest. The other 71 have no
template manifest; none has a HEAD-identical schema. There is no ref to pin to.

Template detection is by **content** — `kind: template` — not by filename, so this is
not an artifact of a naming convention:

```
examples/valid/parlor.template.cafaye.yml  ->  kind: template  (name: parlor)
examples/valid/environments.selfhost-or-hosted.cafaye.yml  ->  kind: service
```

### Why it is structural, not a scheduling accident

`ec28365` is the commit that added `kind` to the schema **and** the commit that
added `parlor.template.cafaye.yml`, in the same change:

```
$ git show --stat ec28365 -- examples/valid
 .../environments.selfhost-or-hosted.cafaye.yml     | 58 ++++++++++
 examples/valid/parlor.template.cafaye.yml          | 36 ++++++
```

A `kind: template` manifest is not expressible before `kind` exists. So the moment
the schema gained the field that makes a template a valid document, core also
published one, and there was never a window in which the schema was current and the
examples were service-only. The two halves of this contract became mutually
exclusive in a single commit, and no re-pin can restore them. The brief's
step 3 anticipated exactly this: *"It would mean core's schema and core's examples
have never been in the state this test needs."* They have not.

### The test was right and the guard worked

`the_ref_under_test_really_is_a_ref_before_the_gate_examples` is not a bug. It read
its own premise, found the premise false, and refused to certify. That is the
behaviour the file's header says it exists to have, and the fix for it is a decision
about what the test should now assert — not a deletion, a skip, or a loosened
comparison picked by whoever is holding the keyboard.

I did not touch `PRE_GATE_EXAMPLES`, and I did not rewrite the header comment. That
comment currently asserts that `39acaed` is a ref where the schema "is byte-identical
to core HEAD's", and that sentence is now false. It is a stale rationale above a live
assertion, which the brief calls out as worse than no rationale — but the correct
replacement depends entirely on which option below is chosen, and writing one now
would be me pre-committing the manager to a rationale for a decision I was told to
stop on. **Rewriting that comment is in scope for whatever gets decided, and is
part of that work, not a separate errand.**

---

## 3. The test still has teeth — demonstrated, not assumed

Step 5 of the brief asks me to point the pin at a non-conforming ref and watch it go
red. I did that for **each half of the contract separately**, because one red does
not prove two assertions are both live. Temporary edits, reverted; `git status` clean
afterwards.

**Half 1 — "no gate declarations at the pin"**, violated by `94f8d25` (the `core-09`
merge that added them). Red on line 513, naming the offending files:

```
94f8d25 was expected to predate the gate declarations, and it carries
["event-envelope.json", "events", "gate.external.yml", "gate.self-contained.yml", ...]
```

**Half 2 — "schema byte-identical across the two refs"**, violated by `a6bbd28` (the
first commit to add schemas, no gate files at all). Red on line 539:

```
the manifest schema changed between a6bbd28 and core HEAD, so a pass at the older
ref would not be attributable to the examples
```

**Restored to the real pin `39acaed6f1e25a895b0410e0689fe68d523b1b63`**, red on line
539 with the manager's original message.

Two different lines, two different reasons, two different refs. The assertions are
independent and both are load-bearing. Restored to pristine, confirmed by
`git diff --stat` and `git status --porcelain` both empty.

One honest note on step 5: its premise was a *green* starting state, and there isn't
one. The current pin already fails the contract, so the "can it still go red"
demonstration is partly the status quo. The two targeted runs above are what actually
demonstrate the teeth, because they vary one half at a time.

---

## 4. Two more reds the brief did not list

This is the part I would most want a second pair of eyes on.

### 4a. Clippy — a third failure, pre-existing, and it was masking everything

`bin/prime` runs four steps: `fmt`, `build`, `clippy`, `test`. Step three failed:

```
error: writing `&String` instead of `&str` involves a new object where a slice will do
   --> tests/filters.rs:292:16
error: could not compile `pantry` (test "filters") due to 1 previous error
```

`cargo clippy --all-targets -- -D warnings` → exit **101**.

It is **pre-existing and not mine**: I extracted the `fn leak` body from `0410885`
(the commit the manager reproduced from) and from my working tree, and they are
byte-identical. My fmt commit only shifted it from line 290 to 292.

It matters *where* it sits. Clippy is step three of four, so on the original tree
`bin/prime` died before a single test executed. The manager's two reproduction
commands — `cargo fmt --check` and `cargo test --test core_pin` — both bypass
`bin/prime` and both reach past this. Working from the brief's list alone, a worker
would re-pin, re-run those two commands, see them green, and report the gate fixed
while `./bin/prime` still exited non-zero for an unrelated reason. **The brief's
failure list is incomplete, and the omission is load-bearing.**

Fixed in `203fcd3`, one line, and the one subtlety worth flagging: clippy's own
suggestion is to change the parameter to `&str`, and the existing body is
`value.clone()`. On a `&str` receiver, `clone()` returns `&str`, not `String` — so
accepting the suggestion and leaving the body would have produced a `Box<&str>`
behind a `&'static String` return type. The body is `value.to_owned()`, which is
the String-producing clone, so the value reaching `Box::leak` is the same owned
`String` as before.

`ptr_arg` was the **only** lint in the repository: suppressing just it and re-running
`cargo clippy --all-targets -- -D warnings` leaves the whole tree clean, so nothing
was hiding behind it. It now exits **0**.

### 4b. Drift — a fourth failure, caused by the other workers, not by pantry

`tests/drift.rs` fails 2 of 12:

```
6 directories are in the workspace that registry/index.yml curates in neither direction:
  wt-sell-error-reporting  — carries a cafaye.yml
  wt-mailer-launch         — carries a cafaye.yml
  wt-sell-identity         — carries a cafaye.yml
  wt-observe-billing       — carries a cafaye.yml
  wt-sell-observability    — carries a cafaye.yml
  wt-sell-backup           — carries NO cafaye.yml — so it is a directory, not yet a repository
```

Every one is a `wt-*` worktree belonging to another repository, created by the
concurrent wave. I created none of them, and I did not touch one.

**I did not "fix" this, and the test itself forbids the obvious fix**, in as many
words: *"Do NOT add a list of tolerated directory names here: a check that can be
made green by not checking is a check that has stopped checking."* Adding six
worktree names to the registry's `known` list would be fabricating a fleet fact —
declaring a temporary worktree of `courier` a curated repository, on a row that
outlives the directory it describes. Deleting them is not mine to do; four belong to
workers running right now.

Ownership, for whoever cleans this up (read-only inspection; I changed nothing):

| worktree | parent repo | mtime | status |
|---|---|---|---|
| `wt-sell-identity` | `identity` | 12:24 | no worker named in my brief — looks like a finished wave |
| `wt-sell-observability` | `courier` | 14:50 | no worker named in my brief — looks like a finished wave |
| `wt-observe-billing` | `billing` | 13:30 | **live** (named in brief) |
| `wt-sell-backup` | `kit` | 15:46 | **live** (named in brief) |
| `wt-sell-error-reporting` | `courier` | 16:38 | **live** (named in brief) |
| `wt-mailer-launch` | `courier` | 16:38 | **live** (named in brief) |

Whether the first two are safe to remove is the manager's judgement, not mine — I am
flagging the timestamps and owners, not asserting they are abandoned. Note that
`kit` currently has 5 modified files in its own tree; that is the `wt-sell-backup`
worker mid-flight, and it is not something I caused.

---

## 5. The full gate — command and real exit code

Entry point is `./bin/prime` (there is no `package.json`, `Makefile` or `justfile` in
this repository; `.github/workflows/ci.yml` and the `bin/prime` header agree). It
needs the workspace root so the drift tests run rather than skip:

```sh
cd /Users/kaka/Code/any/moon/cafaye/pantry
PANTRY_CAFAYE_ROOT=/Users/kaka/Code/any/moon/cafaye ./bin/prime
# REAL EXIT CODE: 101
```

Steps: `toolchain` ok (rustc 1.95.0, mise.toml pins 1.95) → `cargo fmt --check`
**ok** → `cargo build` **ok** → `cargo clippy -D warnings` **ok** → `cargo test
--no-fail-fast` **101, 2 targets failed: `--test core_pin`, `--test drift`**.

**The gate is red. I am not claiming otherwise, and I did not get it green.** The two
failures that remain are the pin decision (blocked, above) and the concurrent
workers' worktrees (not mine).

### Test count

| | |
|---|---|
| total tests | **138** — 15 `Running` binaries plus `Doc-tests pantry` |
| passed | **135** |
| failed | **3** — 1 in `core_pin`, 2 in `drift` |
| skipped | 0 — "every tier ran" |

Per binary: `api` 26, `contract` 9, `core_pin` 6 pass / 1 fail, `drift` 10 pass /
2 fail, `entry_point_isolation` 11, `filters` 13, `kind` 7, `manifest` 13, `pin` 11,
`recorded_copy` 4, `schema` 7, `scoping` 14, `ci` 7, plus `lib`/`main`/doc at 0.

### Floors — unchanged, and why

I added no tests, so no floor moved, and `gate.yml` is absent from
`git diff --name-only 0410885 HEAD`. The ratchet is satisfied vacuously and I am not
claiming otherwise. Existing floors stand at `minimum: 7` (last binary), `minimum:
112` (suite total), `minimum: 0` (skips), plus the `toolchain` and `ok` markers.

**One observation about the `minimum: 112` floor**, not a change I am making. `prime`
sums only lines matching `test result: ok\. N passed`, so on this run it printed:

```
suite: 122 passed across 11 test binaries
```

against 135 actually passing. The 16 passing siblings of the 3 failures — 6 in
`core_pin`, 10 in `drift` — are excluded, because their binaries' summary lines say
`FAILED` and do not match. The floor of 112 is met either way, so nothing is masked
*today*; but the counter is not a floor that degrades when binaries start failing,
and a reader seeing "122 passed" next to three failures deserves to know why the
number is not 135. Flagging it for a decision, not fixing it.

---

## 6. What I need decided — my recommendation, not my action

The test needs a new instrument, and the options are not equivalent. Ranked by how
much of the test's actual value survives:

**A. Split the assertion, keep both teeth. (Recommended.)** Keep `39acaed` as the
pre-gate ref — it is genuinely the last ref whose `examples/valid/` is service-only,
and every example-isolation property the file exists for still holds there. Replace
the single byte-identity assertion with a claim that is *true and still strict*: the
schema at the pin and the schema at HEAD differ **only** by `kind`, `environments`
and their two `$defs` — asserted as an exact set, so any *other* core schema change
still goes red. This keeps "a pass at the pin is attributable to the examples"
verifiable rather than assumed, which is the whole point of the assertion. It is
strictly *more* informative than byte-identity, which only ever told you "or not".

**B. Narrow byte-identity to a documented subset**, as the brief permits proposing.
Weaker than A: a subset invites the next change to be argued into it, and A already
gives the same protection with a positive claim instead of a carve-out.

**C. Ask core for the ref.** A tag or branch at a service-manifests-only state. Clean,
but the state has not existed since `ec28365`, so this means asking core to *create*
it — a change in core, not a change in pantry, and not mine to request.

**D. Delete or skip the assertion.** Rejected. The brief is right that this destroys
a real check, and §3 above is the evidence: both halves of it demonstrably fire, and
the reason it is red is that the premise moved underneath it.

Whichever is chosen, the header comment must be rewritten in the same change. It
currently describes `39acaed` as satisfying a contract it no longer satisfies.

---

## 7. Housekeeping

- **Did not push.** Both commits are local, on `master` in `pantry`'s own tree,
  which was clean on arrival, so no worktree was needed. The manager merges and
  pushes.
- **Two commits, two facts**, as asked: the tree was not rustfmt-clean; the gate was
  not clippy-clean. The re-pin is deliberately *not* a third commit, because there is
  no ref to point it at.
- **Touched nothing else.** No `wt-*` directory entered, no `git stash` or
  `git checkout` in `courier`, `kit` or `billing`, no cargo run outside `pantry`, no
  docker prune, no volume touched, `moon/refs/` not opened. `core` was read with
  read-only `git` and is still clean (0 changed files). `kit`'s 5 modified files are
  the `wt-sell-backup` worker, not me.
- Every long command was run under `timeout`; every `cargo test` under
  `PANTRY_CAFAYE_ROOT` so the workspace-reading tests ran instead of skipping.
