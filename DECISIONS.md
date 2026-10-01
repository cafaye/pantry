# pantry's open decisions

Manager-owned, like every other decision in the fleet: **the packet drafts it, the
manager rules on it.** Nothing here is settled, and nothing here is decided by a
test. A decision recorded twice and still open is worse than one recorded once,
because the second reader concludes somebody must already have ruled — so each
entry is numbered once, and a later packet that adds a decision takes the next
number rather than re-arguing an old one.

The decision numbers here are **pantry's own** and deliberately not MD numbers:
`/Users/kaka/Code/any/moon/DECISIONS.md` records decisions about the *fleet* and
MD1/MD2 are the two pantry rulings it has already made. What MD1 and MD2 built
is answered in `AGENTS.md` under "Answered decisions, and where they went", and
is not repeated here — this file is the open list, and it exists because the two
pantry packets so far each found at least one thing they had to choose and could
not derive.

Each entry is also marked in the file it affects, as
`> DECISION NEEDED (pantry):` or with the packet's name, because a decision
nobody can find from the code it is about is a decision that gets made twice.

---

## D1 — a client library is registered as a `cli`, and the gem beside it is not — OPEN

**Found** 2026-09-30, registering `cafaye-ts` (pantry-06).

**The question.** `cafaye-ts` ships the fleet's OpenAPI documents — six of them,
one per service that has one, vendored into `specs/` — and serves nothing. Its
manifest deliberately omits `exposes`, so `serves_http`, `publishes_events` and
`subscribes` are all false and no derived row of the `kind` table reaches it.
`check_kind` refuses `worker` and `both` for a surface-less manifest, which
leaves the two curated values. One of them is guard's (`api`: a service that
serves HTTP and has not written its document yet), and saying it here would be a
falsehood a client acts on, because `kind: api` is a routing instruction.

**What pantry-06 did, and why it is a decision rather than a derivation.**
`kind: cli`, `basePath: null`, registered. `cli` is the honest value among those
the schema permits: installed rather than brought up, routed to by nobody, which
is a true description of an npm package. The argument in full is on the row in
`registry/index.yml` and in `README.md`, "kind" — the short version is that
`api` is a falsehood, `worker` and `both` are refused, and a value that is
merely less wrong is still better than a falsehood a client routes on.

**The cost, stated plainly, because this is the part that needs a ruling.**
MD1 added `cli` for a binary: *installed **and run***. `cafaye-ts` has no `bin`
and no entry point; it is imported. So the third row of the curated set is an
overstatement of the second, and the bill arrives immediately:

| repository | shape | recorded as |
| --- | --- | --- |
| `caf` | a binary, no surface | registered, `kind: cli` |
| `cafaye-ts` | an imported package, no surface, vendors six documents | registered, `kind: cli` |
| `cafaye-rb` | a gem, no surface | **excluded**, `blockedBy: library` |
| `docs` | a static site, no surface | **excluded**, `blockedBy: library` |

**Two client libraries, two answers, in one file.** And the answer a caller gets
depends on the language of the service they happen to be writing: "what do I
install?" is answerable for a TypeScript service and unanswerable for a Ruby
one. That is arbitrary, and `kind` is documented as a column a client switches
on.

**Alternatives.**

1. **Read `cli` as "an artifact a person installs, run or imported", and move
   `cafaye-rb` into `registry/services/` as a second `cli`.** The boundary
   becomes *is it published as an installable artifact*, and `docs` stays a
   `library` because a documentation site is installed by nothing. One row, one
   directory, one reworded value; `kind` means one thing again; and the registry
   answers the question its clients actually ask.
2. **Keep `cli` narrow — a repository with an executable entry point — and move
   `cafaye-ts` to `blockedBy: library` beside `cafaye-rb`.** One answer for one
   shape, and the reading of today's vocabulary is the stricter one. The cost is
   a published client being invisible to `?kind=` and to `?language=typescript`
   as a *service entry*, and a registry that cannot tell a TypeScript developer
   what to install.
3. **A core amendment** — an affordance that distinguishes a library from a
   binary, so the value is derived rather than curated. This is the gap caf's row
   already names, and a fourth repository is now asking for the word. It is a
   core packet; core is read-only from here.

**Recommended: (1), then (3).** (1) is small, it removes a real inconsistency
rather than documenting it, and it is the only option under which the registry
answers the only question a TypeScript or Ruby developer has about it. (3) is
the durable fix and is not this repository's to make.

**Why pantry-06 did not take (1).** It moves a row a different packet wrote, on
a question the packet was not asked, and the instruction for that packet was to
register rather than exclude. Changing `cafaye-rb`'s curation is the manager's
call, so the inconsistency is recorded in four places — this file, the
`cafaye-ts` row, `README.md`, and `CHANGELOG.md` — instead of being resolved
quietly in either direction.

**Cost of flipping to (2):** move one directory, delete one row, `?kind=cli`
returns `[caf]` again, and the two libraries are consistent. **Cost of flipping
to (1):** one directory, one index row, and `cafaye-rb`'s `library` tripwire in
`tests/schema.rs` stops applying to it — correctly, because a registered entry is
checked by four drift tests instead of one, including byte-equality against the
checkout and the `repository.url` origin comparison. It also means the
`workspace-drift` job can clone and check it, which it already lists in
`CAFAYE_UNREADABLE` only because the repository is private.

---

## D2 — what `blockedBy` says about a directory that is not a repository yet — OPEN

**Found** 2026-09-30, the same packet.

**The question.** `moon/cafaye/cafaye-py/` exists, is empty, and is not a git
repository. MD6 (`DECISIONS.md`, the fleet file) ruled that the Python client is
**hand-written and not generated** — hey-api's Python generator is v0.0.24 and
emits parameterless methods with unsubstituted path templates — so the directory
is a placeholder for planned work with a decided shape.

pantry-06's brief offered three answers: remove it, leave it with an explained
exclusion, or something else justifiable. **What it did: left it, and gave it an
`excluded` row with `blockedBy: no-manifest`**, and strengthened the workspace
walk so the directory cannot be invisible again.

**The cost, stated plainly.** `no-manifest` is documented as *"the repository
carries no `cafaye.yml` on master yet"* — and there is no repository. The row's
own value is therefore a slight overstatement of the fact, in the same way
`cafaye-ts`'s `cli` is a slight overstatement of its fact in D1, and for the same
underlying reason: **the vocabulary was built for repositories, and the workspace
now contains things that are not repositories yet.**

There is a second cost, and it is a real coverage gap that a green badge would
hide. In CI the `workspace-drift` job clones repositories; `cafaye-py` is not one,
so the directory is absent and `every_exclusion_reason_is_still_true`'s
`no-manifest` arm — "the file is not there" — is satisfied by a directory that
does not exist. That is a **vacuous pass**, and it is named as one in the row and
in `README.md` rather than being left to be inferred.

**This packet got the second half wrong first, and the tripwire corrected it.**
The row was written with `cafaye-py` deliberately absent from
`CAFAYE_UNREADABLE`, on the reasoning that the list is a claim about what the
runner cannot *read* and a repository nobody has written cannot be read by
anyone. Then `the_drift_job_clones_every_repository_pantry_curates` refused the
whole registration: a curated name the job neither clones nor declares unreadable
is a claim no drift test can check, and cloning it would fail the job's clone step
on a 404 — the same 404 as `cafaye-rb`'s, for the opposite reason. So it is now
declared, with the difference spelled out beside the list.

That correction is the **second half of the argument for a `planned` value**, and
it is stronger than the first half. `CAFAYE_UNREADABLE` is now carrying two
different claims — *the runner has no credential for this* and *there is nothing
here to clone* — under one name, and its own comment has to explain which is
which for each entry. A list whose entries mean different things is a list whose
entries eventually mean neither. `blockedBy` already has the same problem one
level up: `no-manifest` on a directory that is not yet a repository.

**Alternatives.**

1. **What pantry-06 did:** `blockedBy: no-manifest` on a directory that is not
   yet a repository, plus a workspace walk that sees every non-hidden,
   non-worktree directory. The row is a live tripwire — the day a
   `cafaye.yml` appears there, `tests/schema.rs` fails with *"now carries a
   cafaye.yml — register it or change this row's blockedBy and say why it is
   still held back"* — and the walk means no future version of the test can
   step over it.
2. **Remove the directory.** It is empty, so nothing is lost — and nothing is
   *recorded*, which is the problem. An empty directory is not tracked by git, so
   the deletion is unreviewable: a reviewer sees a commit whose entire content is
   a CHANGELOG sentence about something git cannot show them. It also removes
   MD6's Python decision from the filesystem, and `AGENTS.md` says not to touch
   anything outside the worktree besides reading it.
3. **A fifth `blockedBy` value** — `planned`, say: a known cafaye repository that
   does not exist yet, with the decision that shapes it named in the reason. This
   is the honest vocabulary, and it is the same move MD2 made with `library`
   rather than teaching the drift test that documentation sites are exempt. It
   costs a fifth value for one row, and a fifth value is a precedent — but it
   would also give `CAFAYE_UNREADABLE` its honest name back, because a `planned`
   row would not be a coverage claim the runner has to answer for.

**Recommended: (1) now, (3) when a second planned repository appears.** (1) is
correct for a single row and its tripwire is real. The argument for (3) is the
count: `planned` is a vocabulary gap that MD6's own decisions will keep opening —
Go and Rust clients are also unstarted, and a third empty directory is a matter
of time — and the CI correction above has now shown the gap costs something
outside the registry as well. One value per decision, and the first value to
arrive is not this row.

**Cost of flipping to (2):** one `rmdir` outside this worktree, one CHANGELOG
line, and a tripwire that no longer exists. **Cost of flipping to (3):** one
variant in `src/registry.rs`, one arm in `every_exclusion_reason_is_still_true`,
the `blockedBy` table in `registry/index.yml`, the README section that explains
it, and the value's meaning in `openapi/v1.yaml` if it is ever published — which
it is not today, because `blockedBy` is pantry-internal.

---

## D3 — core's `examples/valid/` holds three document kinds, and pantry must classify rather than conflate — RULED (third kind arrived; consumer side closed here, producer side still core's)

**The ruling, 2026-10-01 (manager).** Option **(3) consumers classify**, held as the
interim, is now the standing answer on the consumer side, and this repository has
paid the cost it was holding (3) at: core landed the third kind.

**What arrived, and what it did.** `core-15` added
`tenancy.account-scoped.yml` and `tenancy.honest-zero.yml` — the tenancy
declaration format, governed by its own `tenant-isolation.schema.json`. Pantry's
gate went red naming both files:

```text
2 file(s) in core/examples/valid/ match neither rule and neither row of NON_MANIFEST_EXAMPLES:
  tenancy.account-scoped.yml
  tenancy.honest-zero.yml
```

That is the mechanism **working**. D3 was opened precisely because a third kind
would break a consumer, and this is the break it predicted, arriving with its
file names in the message instead of as `"owner" is a required property`.

**The part that would have been silently wrong.** The table inferred
`GateDeclaration` for every row, because with two kinds that inference was always
right. With three it is wrong twice. Left alone, the fix that "makes the test
pass" is to add the two filenames to the table — at which point both tenancy
documents are validated against `gate.schema.json`, a schema that governs none of
their fields, and the green is meaningless. So the row now carries the **kind**
(`(&str, DocumentKind, &str)`), and `every_non_manifest_kind_names_the_schema_that_governs_it`
asserts the schema core actually ships for that kind exists at the ref under test.
A row naming a schema that is not there now fails rather than validating nothing.

**Still core's to make: option (1)**, splitting the directory by kind. It remains
the right shape and this ruling does not pretend otherwise — it makes the fleet
correct without waiting for it. What changed is that the cost of (3) is now
measured rather than estimated: one enum variant, two table rows, one test that
had to learn a third case. Every future consumer pays it too.

**How this was found, which is the part worth keeping.** Not by the worker whose
packet it blocked — that worker's gate was **green**. `pin::published_head` reads
`refs/remotes/origin/master` of a *sibling clone*, so whether this suite is green
depends on when someone last fetched core in that clone. See **D26**.

---

**The finding, measured.** `core/examples/valid/` today:

| file | kind | governing schema |
|---|---|---|
| `go-api.cafaye.yml`, `muse.cafaye.yml`, `ruby-api.cafaye.yml`, `worker.cafaye.yml`, `worker-only.cafaye.yml` | service manifest | `cafaye.manifest.schema.json` |
| `gate.external.yml`, `gate.self-contained.yml` | **gate declaration** — `version`, `name`, `gate.command`, `gate.proof`, `external`, `ci` | `gate.schema.json` |

`pantry`'s `the_schema_accepts_core_s_own_valid_examples` validated every `*.yml`
in that directory against the **manifest** schema. So when `core-09` added the two
gate declarations, the gate went red in another repository with:

```text
"owner" is a required property
unknown field "ci"; a cafaye.yml may declare only name, description, language, core, …
```

— a manifest rule quoting a document that was never a manifest. Reported as
**pantry broken**, three times.

**The judgement this packet made, and what it is not.** MD15 ruled the *ref*
question (consumers resolve at their recorded pin, never the working tree). It did
not rule this one, and the brief was right to flag it as the sharpest judgement in
the packet: **a consumer validating "core's valid examples" against the manifest
schema was always going to break the moment a second kind of document appeared in
that directory.** No pinning fixes that. Pinning makes the break happen at a
chosen moment instead of an arbitrary one; it does not stop it.

So pantry now **classifies, and requires the classification to be total**:
`tests/core_pin.rs` holds a table naming every non-manifest example and the
schema that governs it, and two tests hold it in both directions — a file nobody
classified is a FAIL naming the file, and a table row whose file no longer exists
is a FAIL naming the row. Pantry does **not** vendor `gate.schema.json`, and does
not validate gate declarations; it says which schema governs them and leaves that
to core.

**The three options, and what each costs.**

1. **Split the directory by kind** — `examples/valid/manifests/` and
   `examples/valid/gates/`. Then "core's valid examples" has one answer, every
   consumer's `*.yml` walk keeps working, and no consumer needs a table at all.
   Cost: a directory move in core, and every consumer that reads that path
   (pantry, and anything modeled on pantry's test). It is the shape the directory
   should have.
2. **Each example declares which schema validates it** — a `schema:` key, or a
   sidecar, or a subdirectory as a kind marker. Cost: a *cafaye* key in
   non-manifest documents, which is the vocabulary question `AGENTS.md` rule 1
   says not to answer privately — and it makes every consumer parse a declaration
   to do the obvious thing. It also does not compose: the third kind needs a third
   answer, and the declaration is per-file rather than per-kind.
3. **Consumers classify by convention** — pantry's filename rule, `*.cafaye.yml` is
   a manifest. Cost: the convention is implicit, so a third kind is a silent skip
   unless every consumer independently invents the same rule. This is what pantry
   does today *as a stopgap*, and it is why pantry keeps a table and asserts it is
   total: the table is what turns an implicit convention into a checked one.

**Recommended: (1), in core.** It is the only option where the answer does not have
to be re-derived per consumer, and this packet is evidence of why: the same
directory has now broken a gate in another repository once. Pantry cannot make
this change — it is core's tree, and `AGENTS.md` says this repository reads core
and does not own it — so it is recorded here as a request, with (3) held in pantry
as the interim so the fleet is not broken while core decides.

**Cost of flipping pantry to (1) or (2):** one table and two tests in
`tests/core_pin.rs`. There is deliberately **no `ci:` key added to the vendored
manifest schema**, and adding one would be wrong: `cafaye.schema.json` describes
a *service manifest* and a `gate.yml` is a different document with its own schema
(`gate.schema.json`). Widening the manifest schema to accept gate keys would make
every consumer accept a document it has no business accepting.

---

## D4 — a registry copy is a copy *of a recorded commit*, and staleness is a report — OPEN

**What changed.** `registry/services/<name>/cafaye.yml` are nine copies of other
repositories' files. The check that kept them honest compared them to the sibling
**working tree**, so the claim was "this copy is what the service says *right
now*" — a claim about somebody else's repository. `pantry-07` found the
consequence:

```text
identity  copy 11591 bytes, real 13302 bytes — COMMENT-ONLY drift
muse      copy  2930 bytes, real  6280 bytes — a YAML FIELD MOVED — WRONG
          muse: dependencies[0].required — copy says false, real says true
```

`muse-06` made `identity` a **required** dependency (every token is verified
against identity's JWKS, so muse without identity is 503 on every request), and
the registry was publishing `required: false`. All of it reported as *pantry*
being broken. Pantry was not broken; two other repositories had merged.

**The decision.** Every index row now carries `recordedAt`, the commit of *that
service's own repository* the copy was taken from, and
`tests/recorded_copy.rs` compares against that commit. So:

- a merge in `identity` no longer turns **this** gate red;
- a copy edited here, or a `recordedAt` bumped without re-copying, still does, and
  both are defects in this repository;
- how far behind `recordedAt` is has become a **report** —
  `the_registry_says_how_far_behind_each_copy_is_and_names_the_fix`, printed on
  every run, with a stated 9-commit budget above which it fails.

**Why a report and not a gate.** `kit/tests/staleness.py` says the same thing in
its own docstring: *"a stale copy is LEGAL — it is a copy that has not been bumped
yet — and a scheduled report that is red every week is a report that gets muted."*
And MD15's rule 3: making the frequent case a coordinated wave is a rule that gets
skipped the third time it is inconvenient. Nine commits is roughly a working day
of this fleet's merge rate — a person refreshes from the report; a copy past the
budget says the refresh has been missed long enough to stop for.

**The alternative that was considered and rejected: resolve at a ref, no copy.**
Preferred in principle — a registry with no copies could resolve each service at a
ref and there would be nothing to go stale. It does not survive contact with
`AGENTS.md`: pantry has **no database**, nothing persistent, no plugin loader, and
`Registry::load` runs at startup inside a container with no sibling checkouts and
no network. Serving the registry requires the bytes. So the copy stays, and the
decision is to make its origin explicit rather than to pretend it does not exist.

**Who refreshes, and when.** A developer or whoever merges the packet that
changes a service's manifest, in the same commit, with the `recordedAt` bump beside
the `cp`. The report on every gate run is the reminder; the budget is the backstop.
`AGENTS.md` "Registering or changing a service" carries the procedure.

**Open, and honestly so:** `recordedAt` is `Option<String>`, not required, so a
row without one still loads. A metadata gap should not be an outage — a registry
that refuses to start because a comment field is missing is worse than one that
serves. The gap is not silent: three tests fail with the command that records it.
If the fleet would rather a missing `recordedAt` be a load failure, that is a
one-line change to `src/registry.rs` and it should be ruled rather than assumed.

**Cost of flipping:** one field's type in `src/registry.rs`, and one `unwrap_or_else`
in each of three tests. The CI clone must stay non-shallow either way — asserted by
`the_drift_job_clones_deep_enough_to_reach_a_recorded_ref`, which is why `--depth 1`
was removed from `workspace-drift`: a shallow clone would make every recorded-ref
check SKIP, naming the ref it could not read, which is honest but would mean CI
verified nothing about any registry copy while looking green.

---

## D26 — this repository's gate result depends on a ref in a sibling clone, and nothing in it says so — OPEN (found while landing pantry-09)

**The finding.** `pantry::pin::published_head` resolves, in order,
`refs/remotes/origin/master`, `refs/remotes/origin/main`, `master`, `HEAD` of
whatever checkout `core_checkout()` found — which is `$PANTRY_CAFAYE_ROOT/core`, or
`CARGO_MANIFEST_DIR/../core`, i.e. **a sibling clone on the same disk**. So the
suite's green/red is a function of *when someone last ran `git fetch` in that
clone*, not of this repository's tree.

**How it was caught, which is the only interesting part.** Landing
`pantry-09-isolation`, its worker reported exit 0 on a full `cargo test`. My gate
on the same branch — minutes later, same tree — failed
`every_example_in_core_s_valid_examples_is_classified_by_this_table`. Same
commits, same machine. The difference was that something had fetched core in
between. The worker's green was not wrong about its own work (its two new targets
were 25/25); it was **wrong about the suite**, and would have reported a passing
gate for a repository that does not pass.

**Why this is worse than an ordinary flaky test.** A flaky test fails
intermittently and the failure is visible. This one produces a **confident,
reproducible-looking green that is stale**, and it is stale in the direction that
matters most: it hides a *new* document kind in a dependency's directory — exactly
the D3 break. The worker's report even said "not 62 runs of flake"; the honest
version is "green against a ref that was 2 commits behind."

**What would close it**, in decreasing order of cost:

1. `cargo test` fetches the core clone it is about to read, or fails naming the
   ref it read and that it did not verify. Cheap, and makes the number in the
   failure message attributable.
2. The gate runs with `PANTRY_CAFAYE_ROOT` unset and CI supplies core by URL+sha,
   so there is no ambient sibling at all. Correct, and more setup.
3. `published_head` stops falling back to `master`/`HEAD` and only reads an
   explicit ref. Reduces the ambiguity; does not remove it.

**Not done here.** This is a change to `src/pin.rs` and to how the suite is
invoked — a packet, not a drive-by, and the manager's rule is that the manager does
not author. Recorded so it is a decision rather than a surprise. **Recommended (1)**:
it is the smallest change that makes "green" mean "green at the ref I just read."

---

## D27 — six tests here asserted which service sat on which core pin, and said nothing about the thing they were named for — RULED (manager, 2026-10-01)

**The finding.** `tests/api.rs` and `tests/filters.rs` hardcoded the fleet's core
pins: `^0.1.0` for courier, guard and identity, `^0.2.0` for the other six. Six
tests went red when core's version standard (core-17) raised every service to
`^0.2.0`, and **not one of them was about pins.** What they were named for:

| test | name promises | what it asserted |
|---|---|---|
| `a_service_object_carries_exactly_the_documented_keys` | the key set | that identity is on `^0.1.0` |
| `a_document_at_the_repository_root_is_served_verbatim` | `openapi.yaml` is not normalised | that courier is on `^0.1.0` |
| `every_filter_narrows_the_list` | filters narrow | which six services are on `^0.2.0` |
| `a_filter_that_matches_nothing_is_an_empty_list` | empty is empty | two coincidences of placement |
| `contract_filter_matches_by_range_intersection` | range intersection | a partition of the fleet |
| `two_filters_are_both_applied` | the conjunction applies | two hardcoded disjoint pairs |

**Why it stayed hidden, which is the worse half.** The registry copies were
stale. `registry/services/*/cafaye.yml` still held the *old* manifests, so the
served pins were `^0.1.0` and the hardcoded expectations matched them. The tests
were passing **because the data was out of date**. Refreshing the copies — the
ordinary, prescribed maintenance after any merge — turned six green tests red
with nothing in them having changed. A suite that is only correct while its
fixtures are stale is not a suite.

**The ruling.** A test may assert a fact about *this build*. It may not assert a
fact about *another repository's decisions*. Core's version standard is core's
to move, and pantry has no vote in it, so no test here names a version again:

* the two single-entry tests now compare the served `core` against the
  registry copy **read off disk** — a real claim, because the value has been
  through YAML parse, projection and JSON serialisation on the way out;
* `every_filter_narrows_the_list` keeps its `kind` and `language` rows (those
  are facts about the vocabulary) and the contract cases moved to a derived
  test: every pin selects exactly its own members, and **the pins partition the
  fleet**;
* the two "matches nothing" cases are now **computed** — a language exactly one
  service carries, paired with a pin one minor below it, which a caret on a
  `0.x` makes disjoint by arithmetic. `checked > 0` is asserted, because a test
  that proved nothing should fail rather than pass quietly.

**What would have been cheaper and wrong.** Editing `^0.1.0` to `^0.2.0` and
`^0.2.0` to `^0.3.0` in six places. It turns green, it costs four minutes, and it
is the same six tests red again at the next core release — with a fleet that has
now moved twice in a month.

**Still open, and this ruling does not fix it.** `pantry::pin::published_head`
reads a mutable ref in a sibling clone, so a green run is not attributable to a
commit — **D26**. This ruling made the *tests* independent of the fleet; D26 is
what makes the *ref* independent of whoever last ran `git fetch`. They are the
same class of defect and were found the same afternoon, which is the argument for
doing D26 next rather than later.

---

## D28 — two workers answered the observability packet and only one can land — RULED (manager, 2026-10-01)

**What happened.** The observability packet was dispatched twice. Both workers
returned research reports with the same headline and neither implemented
anything:

| branch | base | ships | report |
|---|---|---|---|
| `worker/pantry-07` (`d5608ee`) | `63f0b83` | `bin/fleet-telemetry` (240 lines) | 941 lines |
| `worker/pantry-07-observability` (`e0c83e7`) | `92ca41e` | `scripts/redaction_canary.py` (263 lines) | 1019 lines |

Neither is an ancestor of the other, they share a filename for the report, and
both answer the same three questions. This is the shape D16 is about one level
up: two independently-authored packets, one subject, no collision signal from
git because neither touches the other's files under different names.

**The ruling: land `worker/pantry-07`.**

1. **The measurement is unique to it.** `bin/fleet-telemetry` answers *which of
   the registered services emit telemetry today*, read from `registry/` rather
   than hardcoded. Run on the merged tree it reports **1 of 9 registered
   services emits** — `muse` — while `identity` links the OTel SDK and uses
   nothing, which the script calls out by name as "the failure mode that reads
   as coverage in a dependency audit and is not coverage at all." Nothing else
   in the fleet answers that question, and it is the question a self-hoster asks
   first.
2. **Its second script is repeatable and it says what it did not measure.** It
   prints its unmeasured surface — no docker needed for the inventory, ~1 GB
   pulled only under `--footprint` — and it states outright that it is not a
   gate. A research packet whose conclusion is a number keeps the number; this
   one does.
3. **Its other half is already answered, better, somewhere that owns it.**
   `kit-16` landed `tests/canary_test.sh` — 662 lines proving the redaction
   boundary against a **real collector**, with a capturing exporter, polling
   rather than a race, and an assertion that removal is *observed* rather than
   inferred. `scripts/redaction_canary.py` proves the same property against a
   live Tempo and Loki. kit distributes the stack, so kit should own the
   stack's proof, and `canary_test.sh`'s own header says it: *"a second copy of
   the redaction config is a second answer to the question."* Landing a second
   implementation in a repository that does not ship the stack would be exactly
   that.

**What is kept from the branch that did not land, because it is not in the
other report and it matters more than the report it arrived in.** From §5.4 of
`REPORT-pantry-07-observability.md` at `e0c83e7`:

> A single OTLP/HTTP POST carrying 4000 spans returned **HTTP 200** with a body
> of `{"partialSuccess":{}}` — an explicit *no rejections*.
> `otelcol_receiver_accepted_spans` did not move. Nothing appeared in Tempo, and
> Loki's `labels` endpoint had no series for those records.

The worker's own conclusion is the part worth keeping, and it is stated against
its own uncertainty: *"I do not know why, and I am not going to guess."* The
finding that survives that uncertainty is that **the only way to learn the
pipeline dropped everything is to ask the store at the far end.** A self-hoster
has no reason to ask the store, and a health check that only asserts the
collector returned 200 would be green in exactly this situation.

So the obligation this creates is concrete and belongs to whoever implements the
stack for real: **the boot check must be a round trip, not a receipt.** Assert
that a canary span written through the collector is queryable in Tempo, and that
a canary log line is queryable in Loki. The second half of it is the shape of
`scripts/redaction_canary.py`, and the argument for it is the paragraph above —
so the *script* was worth writing even though the branch did not land. Preserved
on the remote at `keep/pantry-07-observability` (`e0c83e7`), not deleted.

**Note for the next dispatch, because it will happen again.** This was one
packet, two workers, because a dispatch was repeated after a death and the
replacement was given the same name. The ledger shows the original at 18:21 and
the replacement eight hours later. **A re-dispatch into the same packet name
should check for a surviving branch first**, and if one exists, the replacement
should be told what it is replacing rather than being left to redo it.
