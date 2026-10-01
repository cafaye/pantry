# Changelog

All notable changes to `pantry` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project is
pre-1.0, so **Minor** carries breaking changes — see the versioning note at the
bottom.

## [Unreleased]

### Added

- **Tenant isolation: the enumeration of every entry point that reaches
  registered data, and a negative case for each.** `tests/scoping.rs` (new, 14
  tests) and `tests/entry_point_isolation.rs` (new, 11 tests), from the
  cross-tenant negative-test packet. No behaviour changes; nothing in `src/`
  moved.

  **The packet's premise does not hold in this repository, and that is the
  finding.** pantry has no accounts, no database and no write surface:
  `account` / `tenant` / `owner` / `org_id` / `user_id` / `customer` across
  `src/` return exactly one hit — `Manifest::owner`, the accountable *team* on a
  `cafaye.yml`, which core's schema defines and which no query filters on;
  `ROUTES` is four `get`s with no `post` / `put` / `patch` / `delete` anywhere
  in `src/`; and the registry is YAML read once at startup. So D18's
  `pantry 0` cross-tenant negative tests is a **correct measurement of a
  property, not a gap in coverage**, and what is worth recording is the
  boundary that *is* real: **the registry, and nothing outside it**.

  Two ways to cross that boundary, both pinned. **Reach** — the `{name}` path
  segment is the only free text a caller controls, and
  `a_service_name_reaches_a_string_comparison_and_never_a_path` requires it to
  reach nothing but a string comparison. **Existence as an oracle** — every
  negative asserts the status is not 401, 403 or 405, and that the response
  *shape* (`status`, `type`, `title`, `code`, content type) is byte-identical to
  the answer for a name that has never existed. A 405 that differed between a
  registered name and an unregistered one would be the same oracle as a 403.

  Counts, each asserted by a named test and checkable against the source:

  | set                                                     | count |
  | ------------------------------------------------------- | ----- |
  | `pub fn` / `pub async fn` in `src/`                     | 58    |
  | HTTP entry points a request can reach (4 routes + 2 fallbacks) | 6 |
  | registry data entry points (`registry.rs` + `filter.rs`) | 23 |
  | of those, ones a request can change                     | 8     |
  | routes registered with a verb other than `get`          | 0     |
  | write primitives in `src/`                              | 0     |
  | 403 / `forbidden` / `unauthorized` in `src/`            | 0     |

  Per operation: **read** 1 entry point / 45 negative names, **list** 1 / 19
  negative parameters plus 6 empty-match filters, **update** **0 entry points**
  / 16 verb-path pairs all 405, **delete** **0** / the same 16. update and
  delete are at zero and that is asserted, not assumed:
  `every_write_verb_on_every_registered_path_is_405_and_never_a_success` sweeps
  POST / PUT / PATCH / DELETE against all four registered paths and requires no
  2xx.

  **The names that carry the weight are the six repositories
  `registry/index.yml` holds back** — `core`, `docs`, `cafaye-rb`, `cafaye-py`,
  `parlor`, `kit`, four of which carry a valid `cafaye.yml`. They are read from
  the index rather than written into the test, so a new exclusion row is a new
  probe. A registry answering for one of them would be publishing a service the
  platform cannot start. Alongside them: eleven **scope-shaped query parameters**
  (`?account_id=`, `?tenant=`, `?org_id=`, `?owner=`, `?scope=`, …) which are
  refused 400 with a shape byte-identical to a mistyped parameter, and eight
  **scope-shaped headers** which leave the list and the read body byte-identical.
  A caller cannot tell a parameter pantry recognises-but-refuses from one it has
  never heard of, and no header can narrow a response.

  Two tripwires for the next packet rather than this one:
  `nothing_reads_an_authorization_header_or_a_credential` and
  `the_only_environment_variables_the_service_reads_are_the_two_named`. A tenant
  boundary cannot arrive in pantry unasked.

  **One of these tests was caught failing rather than asserted.** The mutation
  that proved it: a new `Problem::forbidden` plus an
  `if name.contains("tenant")` guard in `get_service` — the plausible three-line
  version of "add a tenant boundary". `tests/scoping.rs` caught it twice; the
  behavioural file caught **nothing**, because all 36 of its read negatives were
  near misses on real service names and path shapes and not one contained the
  substring the guard looked for. Nine scope-shaped names were added (36 → 45)
  and the same mutation is now red. The lesson is in the file: a negative set
  has to contain the shapes of the **mistake**, not only the shapes of the
  attack.

  A second one, from a failure I caused rather than a mutation: the on-disk
  probe set's size is **floored at 15, and the six held-back names are checked
  for membership**, not pinned to an exact count. Pinning it meant writing this
  packet's own report file turned a reach guard red. The property is "no path in
  this repository is reachable by name", not "this repository has N top-level
  entries" — and a number that goes red because a file was added is a number
  people bump without reading, which is the exemption branch `AGENTS.md` warns
  about.

  **No 403 was found**, so nothing in `src/` changed. The one existence oracle
  pantry does have — a registered name answering 200 and an unregistered one 404
  — is **deliberate and now recorded as such** in
  `the_registry_is_public_and_serving_two_hundred_is_deliberate`: the registry
  is public curated data, every caller is served all of it, and if it ever stops
  being public that test is the line that has to change.

- **`gate.yml`: pantry's gate is now declared rather than guessed.** Written
  against core's `schemas/gate.schema.json` and checked by core's
  `harness/gate_check.py`, it records the command (`bin/prime`), the mise task
  (`mise run prime`, which resolves to the same file), the entrypoint, eight
  proofs with three numeric floors, the four things the gate needs that are not
  in this repository, and the CI job that runs it.

  Every number came from a real run at 92ca41e on a saturated 8-core arm64 host
  (rustc 1.95.0, cargo 1.95.0, mise 2026.8.4): 114 tests across 11 integration
  test binaries, 0 ignored, ~4m17s warm. Nothing here is copied from another
  repository's declaration.

- **`bin/gate-self-test`: the proof that `gate.yml` is able to be wrong.** It
  copies the repository, breaks exactly one thing at a time, and asserts core's
  checker goes red *and names the finding it expects* — naming being the half
  that decays silently, since a check written for one specific defect can be
  dead code forever while everything stays green. A control on the unmodified
  repository runs first in both phases, `edit` fails loudly when a breakage
  recipe stops applying, and pass and skip counts are reported separately. Not
  part of `bin/prime`: the checker is core's and is not vendored here, and a
  self-test inside every gate invocation is a second gate that can disagree with
  the first.

- **`recordedAt`: every registry copy now records the commit it was taken from,
  and the copy is verified against that commit rather than the sibling checkout's
  working tree.** `registry/index.yml` gains `recordedAt` per service, and
  `tests/recorded_copy.rs` (new) checks each `registry/services/<name>/cafaye.yml`
  — byte for byte, and field by field — against `<service>` **at that ref**.

  This is MD15 applied one repository over, and it is the change of *whose
  failure it is*. The old check asserted "this copy is what the service says
  right now", so a merge in `identity` or `muse` turned **this** gate red and was
  reported as *pantry* being broken. It happened three times. Once, muse's copy
  was publishing `required: false` for a dependency `muse-06` had since made
  **required** — a muse without identity is 503 on every request — and nothing in
  the failure named the repository that had moved.

  Now a merge elsewhere does not turn this gate red; how far behind a copy is
  has become a **report** —
  `the_registry_says_how_far_behind_each_copy_is_and_names_the_fix` prints every
  service's distance on every run and fails only past a 9-commit budget, with the
  `cp` that fixes it. A stale copy is legal; a scheduled report that is red every
  week is a report that gets muted. See `DECISIONS.md` D4.

  All nine `recordedAt` values are current at time of writing: **9 current, 0
  behind, 0 unmeasured**. Two copies were refreshed (`identity`, `muse` — the two
  `pantry-07` found) and three `recordedAt` values were bumped to a newer head
  whose `cafaye.yml` bytes are identical to the recorded ones.

### Fixed

- **`bin/prime` now asserts the toolchain pin before it does anything else.** The
  pin lives in three files — `mise.toml`'s `[tools] rust`, `Cargo.toml`'s
  `rust-version`, and `docker/Dockerfile`'s `ARG RUST_VERSION` — and there is no
  `rust-toolchain.toml`, so nothing made cargo enforce it. Whatever rustc
  answered on PATH was what formatted, built, linted and tested the gate, and a
  mismatched toolchain reported itself as a wall of cargo errors about features
  and dependency versions that reads as a defect in the code under test.

  `bin/prime` now reads the pin out of `mise.toml` — the file that owns it, so
  there is no fourth copy to forget — and exits 127 naming `mise install` when
  the answering rustc is a different minor version. A patch release of the
  pinned toolchain (1.95.x) passes, because that is the same toolchain. This is
  the cafaye-rb lesson applied here: a requirement the gate does not check is a
  comment.


- **A stand-down tier is now visible in the gate's output.** This suite has no
  `#[ignore]`. Every environment-gated tier — drift, schema, core-pin,
  recorded-copy — prints `SKIP …` and returns early from the test body, which
  libtest scores as a **PASS**, and libtest captures a passing test's stderr and
  discards it.

  Both halves were measured, on a real `git clone` in a directory with no
  cafaye sibling anywhere up the tree, which is what a pantry-only clone is and
  what CI's `build` job sees: **`bin/prime` exited 0 reporting `114 passed; 0
  failed; 0 ignored` with the entire drift tier skipped, and printed no SKIP line
  anywhere.** A developer could not have told that run from one that compared
  every registry copy against the real services. That is the `cafaye-rb` defect
  — 1430 tests green against an empty schema — wearing a Rust hat, and it was
  live in this repository.

  `bin/prime` now runs the suite with `--nocapture`, counts the `SKIP` lines, and
  prints `skips: N` on **every** run — not only when it is non-zero, because a
  report that only appears when something is wrong is a report nobody learns to
  read. When N is not zero it names `PANTRY_CAFAYE_ROOT` and says plainly that
  those tests counted as passes. It also prints `suite: N passed across 11 test
  binaries`, which gives the declaration a floor that moves when any test is
  lost rather than when the alphabetically-last file is.

  No assertion was weakened and no test was changed: the skip still happens, the
  suite still passes, and the gate still exits 0. What changed is that it now
  says so.

- **`vendir.lock.yml`: this repository records which commit of `core` it
  vendored**, and `schemas/cafaye.manifest.schema.json` is verified against
  **that** commit rather than core's working tree.
  `the_vendored_schema_is_core_s_schema_at_the_ref_this_repository_records`
  replaces `the_vendored_schema_is_byte_identical_to_cores`, and
  `this_repository_says_how_far_behind_core_it_is` reports the distance on the
  same 9-commit budget. Same reasoning as above, one repository over.

- **`pantry::pin` (new module): the resolver both of the above use.** It reads
  the two shapes a core pin really takes in this fleet — a `vendir.lock.yml`
  sha, or a `CORE_REF: <sha>` in a workflow — preferring the lockfile, in the
  same order and for the same reason as `kit/tests/staleness.py`. Three
  outcomes, all of them named:

  - a full 40-hex commit resolves, and says which file it came from;
  - two pins that disagree is a reported state, not a silent winner;
  - **no pin is a skip that names what was searched** — and `muse`'s
    `CORE_REF: 'master'` is refused as a pin, because a branch is a question that
    changes answer over time.

  A fallback to the working tree is deliberately not implemented, in either
  direction: it is the defect, and reintroducing it as a "degraded mode" would
  make the green mean nothing while appearing to fix it.

- **The two-document-kinds classification, and the test that holds it in both
  directions.** `core/examples/valid/` holds service manifests
  (`*.cafaye.yml`, `cafaye.manifest.schema.json`) *and* gate declarations
  (`gate.*.yml`, `gate.schema.json`). `tests/core_pin.rs` (new) keeps a table
  naming every non-manifest example **and the schema that governs it**, and
  asserts the classification is total: a file nobody classified fails with its
  name, and a table row whose file no longer exists fails too. pantry does not
  vendor `gate.schema.json` and does not validate gate declarations. The
  recommendation to core is to split the directory by kind — `DECISIONS.md` D3.


- **`cafaye-ts` is registered — `kind: cli`, `basePath: null`.** The TypeScript
  client, which has declared itself in the platform's own contract language
  (`caf contract lint: OK`) and which the workspace walk found carrying a
  `cafaye.yml` that appeared in no curation list. `registry/services/cafaye-ts/
  cafaye.yml` is a byte-identical copy and the index row carries the two facts a
  manifest cannot state.

  **`kind: cli` and not `api`, which is the judgement worth reviewing.** This
  package vendors the fleet's six OpenAPI documents into `specs/` and serves
  none of them; its own manifest is explicit that `exposes` is absent because it
  "does not serve traffic, does not receive it, and does not publish events", and
  that reaching for `exposes.api: specs/` "would say this package SERVES an API
  described by those documents. It serves nothing." So no row of the `kind` table
  reaches it, `check_kind` refuses `worker` and `both`, and the other curated
  value is guard's — a service serving HTTP whose document is unwritten, which
  would be a falsehood a client acts on, since `kind: api` is a routing
  instruction. `basePath: null` for the same reason: a base path is derived from
  a document a service *publishes*, and these six were published by six other
  repositories and copied at recorded commits.

  **And `cli` is an overstatement, which is why the row carries a
  `DECISION NEEDED` rather than a settled answer.** MD1 added `cli` for a binary
  — installed *and run* — and this is a package with no entry point, imported
  rather than run. The bill lands immediately: `cafaye-rb` is this repository in
  Ruby, same shape, and it is held back as `library`. Two client libraries, two
  answers in one file. **`DECISIONS.md` D1** has the three options and the
  recommendation; nothing in the exclusion record was changed, because resolving
  it means moving a row another packet wrote.

- **`cafaye-py` is recorded, and it is a directory rather than a repository.**
  `moon/cafaye/cafaye-py/` is empty, has no git checkout and no `cafaye.yml`, and
  it is the planned hand-written Python client: MD6 ruled Python hand-written
  rather than generated, because hey-api's Python generator is v0.0.24 and emits
  parameterless methods with unsubstituted path templates, and openapi-generator
  inverts `const` discriminants to `any`.

  **It was not removed**, and the reason is worth more than the removal would
  have been. An empty directory is not tracked by git, so deleting it is
  *unreviewable*: a commit whose whole content is a CHANGELOG sentence about
  something git cannot show a reader. It would also have taken MD6's Python
  decision out of the filesystem, and `AGENTS.md` says not to touch anything
  outside this worktree. So it is left, and the registry has an opinion about
  it — `blockedBy: no-manifest`, which is the closest true value the vocabulary
  has and a slight overstatement of it, since there is no repository yet. That
  overstatement is `DECISIONS.md` **D2**, open, with a fifth value (`planned`)
  recorded as the alternative and a recommendation to wait for a second such
  directory before spending a precedent on one.

  **What the row buys: a tripwire.** `every_exclusion_reason_is_still_true`
  asserts the manifest is absent, so the day someone writes
  `cafaye-py/cafaye.yml` the suite fails with *"now carries a cafaye.yml —
  register it or change this row's blockedBy and say why it is still held back"*.
  An empty directory tolerated by doing nothing is an empty directory forever;
  this one has to be answered.

  **What it does not buy, stated here so a green badge is not over-read:** the
  check is **vacuous in CI**. There is no repository to clone, so "the manifest
  is absent" is satisfied by a directory that is not there. It is verified by a
  developer's run with `PANTRY_CAFAYE_ROOT` set and by nothing else.

  **And the tripwire corrected this packet, which is why it is in the changelog
  at all.** The row was written with `cafaye-py` deliberately absent from
  `CAFAYE_UNREADABLE`, on the reasoning that the list is a claim about what a
  runner cannot read and a repository nobody has written cannot be read by
  anyone. `the_drift_job_clones_every_repository_pantry_curates` then refused the
  whole registration: a curated name the job neither clones nor declares unreadable
  is a claim no drift test can check. Cloning it would fail the job's clone step
  on a 404 — the same 404 as `cafaye-rb`'s, for the opposite reason. So it is
  declared, with the difference written beside the list. That difference is the
  second half of D2's case for a `planned` value: "the runner cannot read this"
  and "there is nothing to read" are two claims, and one list is now carrying
  both.

- **The workspace walk now sees a directory with no `cafaye.yml` in it.**
  `every_directory_in_the_workspace_is_a_repository_the_registry_curates` asks
  about every non-hidden, non-worktree directory, not only the ones carrying a
  manifest. The existing walk asks the right question of the wrong set: a cafaye
  repository that lost its `cafaye.yml` in a merge, and a directory created for a
  repository nobody has written, are both invisible to it, silently.
  `cafaye-py` had been in the workspace through four packets and the tripwire
  could not see it, because the shape it looked for was a file.

  Three exclusions and none of them is an exemption: a hidden directory is not a
  repository (`.git`, and the workspace's own `.github`), a worktree is not a
  repository (they are named `<service>-worker-<packet>`, and a manifest inside
  one is that service's manifest, already checked through its own checkout), and
  anything else in that directory is a repository the registry should have an
  opinion about or a stray worth finding. There is deliberately **no list of
  tolerated names**: a check that can be made green by not checking is a check
  that has stopped checking.

- **A `cli` that has written an OpenAPI document is now a test failure.**
  `a_registered_cli_publishes_no_openapi_document_of_its_own` walks each `cli`'s
  checkout for a document where core's conventions put a published one — a
  directory named `openapi`, or `openapi.{yaml,yml,json}`. cafaye-ts is why the
  check exists and it is the awkward case: it carries six documents, correctly
  under `specs/` rather than in either of those positions, because a vendored
  copy of somebody else's specification is an input and the provenance record for
  an input is not a surface. Without the check, a repository that writes its
  document and has not yet declared `exposes` would sit in the registry on a
  curation that a file in its own tree has already made false.

  That check's **narrowness is pinned by a fixture**,
  `the_cli_document_walker_looks_only_where_core_puts_a_published_document`, over
  a directory holding both sides of the distinction: a vendored
  `specs/identity.yaml` and an `openapi-ts.config.ts` must never be found, and an
  `openapi/` directory and an `openapi.json` must always be. If the rule were ever
  widened to "any file whose name mentions openapi", this list would grow — and
  the tempting fix at that point would be to widen the exemption rather than
  narrow the rule, which is the failure this pins shut.

- **`kind: cli`, and `caf` stops being recorded as an `api`.** MD1. `caf` is a
  binary — its own manifest says "caf is a binary, not a service: it exposes no
  HTTP surface and publishes no events" — and the registry had no way to say so,
  so it said `api`. A client reading that and routing to caf found a command with
  no HTTP surface, which is the false answer the whole `kind` column exists to
  avoid. `ServiceKind` has a fourth value, `?kind=cli` returns `[caf]`, and the
  `DECISION NEEDED` on caf's row is answered and replaced by the next question.
  `openapi/v1.yaml` moves to **1.1.0**: the `kind` enum and the `?kind=`
  vocabulary both gained a value, which under the versioning table at the bottom
  of this file is "a breaking change to a response, a filter" — a client
  generated from 1.0.0 switches on `kind` exhaustively and now has a case it has
  not handled. It is the right kind of break: the old vocabulary could not
  express `caf`, so the registry was publishing `api` for a binary.

  **The value is curated, and the reason is that a manifest cannot carry it.**
  core's conventions rule 3 says a repository with no `exposes` "is a library or
  a spec repo", which is neither guard nor caf; `language` names a toolchain
  rather than a shape; and guard (a gateway whose OpenAPI document has not been
  written yet) and caf declare the *same* absence of surface. So the two curated
  values for a surface-less manifest are `api` and `cli`, and nothing in a
  manifest decides between them. The tempting derivation — a compiled language
  and no surface is a CLI — calls guard a binary, and since this same packet adds
  `blockedBy: library` it would also call `cafaye-rb` one. Three repositories,
  one rule, two wrong answers.

- **`blockedBy: library`, and `docs` and `cafaye-rb` have rows.** MD2. Both carry
  a valid `cafaye.yml` and appeared nowhere in the registry, parked in an
  `UNDECIDED` constant in `tests/drift.rs` since pantry-03. Neither is registered
  — registration claims `caf dev` can bring the thing up, and a documentation site
  and a gem are depended on rather than started — and the `UNDECIDED` constant is
  gone, because a third list where an undecided repository needs no reason is a
  check that can be made green by not checking. The fourth `blockedBy` value is
  the alternative MD2 chose over teaching the drift test that libraries are
  exempt.

  The row is not weaker than the others: `library` goes stale in one direction,
  and it is the direction that has already fired three times. A library whose
  manifest declares `exposes` or a non-empty `consumes` is something the platform
  starts and routes to, so `every_exclusion_reason_is_still_true` fails it with
  *"must be REGISTERED"*.

- **CI now checks the registry against the fleet, and the badge means it.** The
  `workspace-drift` job is enabled. It was `if: false` because it cloned the
  cafaye repositories over SSH with a deploy key from this repository's
  settings, on the assumption that the repositories are private — and they are
  public, all of them. So the key was a long-lived private key created to work
  around a problem that does not exist; it is gone, and the clones are
  anonymous HTTPS with no secret anywhere in the workflow. The clone list is the
  whole organisation rather than the subset the registry registers, because
  deriving it from the registry makes coverage a function of the registry and a
  ninth service is then the one nobody checks. The `test -f core/schemas/…`
  guard is kept and widened to every checkout, since a missing file there does
  not fail anything — it makes every drift test skip and the run go green.
  `tests/ci.rs` is new and asserts the four things a person could break by
  accident: the clone list covers everything `registry/index.yml` curates, the
  clones carry no credential, the job is enabled, and the suite is pointed at
  the checkouts. It needs no workspace, so it runs in the `build` job too.
  **The job has never executed on a runner** — it is verified against a
  workspace of eleven anonymous HTTPS clones of the real fleet, which is the
  honest local equivalent and not the same thing.
- **`caf` is registered.** `registry/services/caf/cafaye.yml` is a byte-identical
  copy of the platform CLI's own manifest, and `registry/index.yml` carries its
  row. `caf-03` rewrote that manifest into core's frozen shape, which made the
  exclusion record's "still the pre-core draft" reason false; the tripwire in
  `tests/schema.rs` failed the suite rather than letting the registry go stale.
- **`courier` is registered.** `registry/services/courier/cafaye.yml` is a
  byte-identical copy of courier's own manifest, and the exclusion row is gone.
  `courier-03` renamed courier's five event types from `email.queued` and
  siblings to `courier.email.queued` and siblings, which made the row's stated
  reason — that two-segment types fail core's `eventType` pattern — false. The
  tripwire has now caught `darkroom`, `caf` and `courier`: three firings, three
  correct ones, for a check added in 0.1.0.
- **A registry copy is now byte-checked, not just field-checked.** See the
  decision below; this is the change that settles the comment-rot contradiction
  and it applies to every copy from here on.

### Decisions worth the changelog

- **A new vocabulary value is not a branch, it is a closed set.** The curated
  branch of `check_kind` used to refuse `worker` and say "curate this as `api`",
  which left `both` admitted for a manifest that declares no contract surface.
  Nothing in the check rejected it; a test in another file did. Adding `cli` is
  exactly the moment that would have been baked in as a permanent hole, so the
  branch now admits a closed set — `api` or `cli` — and its refusal names both.
  `tests/kind.rs` is new and holds the whole table as a check rather than as a
  comment: seven tests over real registry directories on disk, because the rule
  being tested is that a row, a manifest and a `basePath` agree with each other.
- **A private repository is a fact about the runner, and it is declared.** MD2's
  `cafaye-rb` row makes `the_drift_job_clones_every_repository_pantry_curates`
  true no matter how it is written, because an anonymous runner cannot clone a
  private repository and a credential in the workflow is what pantry-04 deleted.
  So the job carries `CAFAYE_UNREADABLE: cafaye-rb` beside `CAFAYE_REPOS`, with
  the reason; `tests/schema.rs` prints a `SKIP` naming the row it could not check;
  and `tests/ci.rs` refuses an unreadable name that is registered, that the
  registry has stopped curating, or that is also being cloned. The coverage gap
  is real and it is named in three files rather than being papered over by a
  silent one.

- **The exclusion record was fixed in the data, not in the test.** caf's and
  courier's rows each mixed one fact with one opinion, and each fact was made
  false by a packet in a different repository. The tempting fix was a
  `…unless it's a CLI` or `…unless it's courier` branch in
  `every_exclusion_reason_is_still_true`, and that is a weakening: the check
  would go on catching only the exclusions nobody disputes. Both now publish a
  valid manifest in the platform's own contract language, so both are in the
  fleet whether or not anything routes to them.
- **Registry copies are verbatim, comments included, and the drift test now
  asserts it.** `AGENTS.md` said copies are kept "verbatim, including its
  comments" while the drift test's own doc comment said "a comment-only edit
  upstream is not drift". Both cannot have been true, and pantry-02 found the
  proof: `registry/services/guard/cafaye.yml` was missing guard's entire
  `guard-04` `DECISION NEEDED (REDIS_URL)` block, and the drift test passed,
  because it compared parsed fields. **billing was stale the same way** and
  nobody had reported it — pantry-02 checked guard and stopped.

  The choice is verbatim. A stale comment in a registry copy is not cosmetic:
  these files carry the `DECISION NEEDED` blocks that say *why* an entry looks
  the way it does, and a copy that silently drops one answers a reviewer's
  question wrongly — it makes a service with three open questions look settled.
  The failure was silent, which is the worst kind; byte-equality turns it into a
  red that names the file and the fix. The cost of the other choice is that the
  weaker claim stays untested, which is how this contradiction survived two
  packets in the first place.

  So `tests/drift.rs` now asserts byte-equality for every entry, and both stale
  copies are refreshed in the same commit. The prose in `AGENTS.md`, the
  doc comment on the drift test and this entry all say the same thing now.
  The one thing byte-equality gives up is tolerance of a pure comment edit
  upstream, and the fix for that is `cp ../<service>/cafaye.yml
  registry/services/<service>/cafaye.yml` — a mechanical step, not a judgement
  call, and it is what the check's failure message says to run.
- **`kind` for caf is curated, and the vocabulary has a gap.** caf's manifest
  omits `exposes` on purpose — "caf is a binary, not a service" — and declares no
  `consumes`, so nothing in it can derive a `kind`. `api` is the only value the
  current checks admit (`check_kind` refuses `worker` for a manifest that
  declares no surface; `tests/drift.rs` refuses `both` without a real api
  surface and a real subscription), so it is a choice under constraint rather
  than a derivation, and the row says which. `> DECISION NEEDED (pantry)` in
  `registry/index.yml` proposes a fourth `kind` for binaries.
  **Answered in pantry-05 (MD1): the vocabulary has `cli`, caf says it, and the
  curated set for a surface-less manifest is `api` and `cli`.** What this entry
  got right is kept: the value is still curated, because a manifest that declares
  nothing cannot say which of the two it is. What it called a choice under
  constraint is no longer that — it is the right answer for a binary, recorded by
  a person.
- **`basePath` for caf is `null`.** It declares no `exposes.api` and its
  checkout publishes no OpenAPI document, so core's `/vN` rule has nothing to
  read. `/v1` by analogy with the api entries would be the longest-common-prefix
  guess that rule exists to prevent.
- **`basePath` for courier is `/v1`, and its OpenAPI document is partial.**
  courier-03 flagged that `openapi.yaml` covers `/v1/webhook_endpoints` only and
  says so in its own header. That does not make the value a guess: every path the
  document publishes is under `/v1`, and every non-probe route in courier's
  router is under `/v1` as well, so the undocumented routes cannot move the
  prefix. `tests/manifest.rs` pins the rule that decides it — a partial document
  still yields a base path, a document with no `/vN` yields none, and a document
  with two prefixes needs a decision rather than an average.

### Changed

- **`workspace-drift` clones the fleet non-shallow.** `--depth 1` was right until
  recorded-ref checking; it is wrong now, because `git show <sha>:<path>` cannot
  reach a commit a shallow clone does not have, so every recorded-ref check would
  become a skip — an honest one naming the ref it could not read, but a CI run
  that verified nothing about any registry copy while looking green.
  `tests/ci.rs::the_drift_job_clones_deep_enough_to_reach_a_recorded_ref` fails
  if `--depth` comes back. The credentials test was updated in the same commit:
  it previously *required* `--depth 1`, and it now forbids it.
- **`manifest::validate_schema` keeps its signature and gains
  `manifest::validate_against`.** The first validates against the schema compiled
  into the binary, which is what `/readyz` must keep doing offline; the second
  takes the schema as an argument, which is what a test resolving core at a ref
  needs. No behavioural change to the first.
- **The two copy-comparison tests moved out of `tests/drift.rs`**, leaving a
  tombstone naming where they went and why. What stayed in that file is the drift
  that is genuinely about this repository — the curated `kind` and `basePath` —
  which has no recorded ref, because a `basePath` is derived from a document the
  service publishes today.
- **README's skip count corrected from 11 to 21, measured rather than estimated.**
  The old number was stale and the test count beside it (89) was stale too. Both
  are now 114 passing in either environment, with 21 skips in a pantry-only clone
  — so the counts in that section were checked against a real run instead of
  carried forward.

### Removed

- **`the_schema_accepts_core_s_own_valid_examples`**, which read
  `core/examples/valid/*.yml` from the working tree and validated every one
  against the manifest schema. It is replaced by
  `every_manifest_this_repository_ships_validates_at_the_ref_it_vendored`, which
  makes the claim the old one was trying to make: *the manifests this repository
  ships validate against the schema at the ref this repository has actually
  vendored.* A tombstone in `tests/schema.rs` records why, because a reader who
  greps for the old name should find out where it went.

- **`?kind=cli` returns two entries, and `?language=typescript` returns two.**
  `[caf, cafaye-ts]` and `[cafaye-ts, guard]` — a command, an imported package,
  and a gateway whose document is unwritten. `?contract=^0.2.0` gains
  `cafaye-ts` alongside `caf`; the `^0.1.0` group is unchanged, because
  cafaye-ts's own manifest says `^0.2.0` and the registry records what the file
  says. `?language=typescript&contract=^0.2.0` stopped being an empty question,
  so the empty-list case became `?language=python&contract=^0.1.0` — muse is the
  only python service and it is on `^0.2.0`. That case has now been invalidated
  by a registration twice, and the third replacement was checked rather than
  assumed.
- **Paging over the whole registry is five pages, not four.** Nine entries at
  `limit=2` finish on a page of one, so `a_page_limit_slices_the_list_and_the_
  cursor_finishes_it` and its in-process twin now assert a short final page. A
  limit is a maximum and an odd length produces one; a client that assumed full
  pages would ask once more and find nothing.

- **A checkout's remote is compared by repository, not by spelling.**
  `every_registered_repository_url_is_the_real_services_remote` compared the
  registry's declared URL to `remote.origin.url` as strings, and the two are
  written in different transports by construction: a developer's checkout is
  SSH, and a hosted runner cloning a public repository anonymously is HTTPS. So
  the moment the drift job was enabled against a real workspace, the suite was
  red on the first registered entry for a checkout that was correct in every
  respect a registry can observe. The comparison is now `owner/name`; the SSH
  requirement is unchanged and still enforced, by core's schema pattern, which
  every entry is validated against in `tests/schema.rs`. An origin naming a
  different repository on the same host, or one that names no GitHub repository
  at all, is still a failure — the second now with a message naming the value
  rather than a string mismatch.
- **`?language=go` and the `^0.2.0` filter each gained a member.** caf is the
  second `go` repository in the registry, so those filters no longer return a
  single service.
- **`?language=elixir` and `?contract=^0.1.0` gained a member too.** courier is
  the first `elixir` repository and the third on `^0.1.0`. The `^0.1.0` group
  matters to a caller: pre-1.0 a caret pins the minor, so a client asking
  `?contract=^0.2.0` still does not get courier, and that is courier's manifest
  saying `^0.1.0` and the registry recording it unchanged.
- **`?language=elixir` is no longer an empty result.** It was a
  `a_filter_that_matches_nothing_is_an_empty_list` case, and a registration
  invalidated it. The replacement is `?kind=both` and
  `?language=elixir&contract=^0.2.0` — still empty, and the second is now a
  real pre-1.0 boundary check rather than an accident of courier being excluded.
  A case that a registration can invalidate is still worth having; the fix is to
  pick a genuinely empty question, not to delete it.
- **The CI workflow now says out loud what a green run does not prove, and
  carries the missing job disabled rather than absent.** `.github/workflows/ci.yml`
  clones pantry alone, so every drift test skips there. A green badge on this
  repository means *pantry is internally consistent*, not *the registry matches
  the fleet* — and the workflow says so in the job that runs, in the job that
  does not, and in the README. A `workspace-drift` job is written out in full
  with `if: false`: it clones the eight service repositories beside a `pantry`
  checkout and runs the whole gate with `PANTRY_CAFAYE_ROOT` set. It is disabled
  rather than absent because an absent job is forgotten and a disabled one says
  on its face that the coverage does not exist yet. It cannot be enabled from
  here — the cafaye repositories are private and kit's shared CI deliberately
  reaches no cafaye service — so **how a runner authenticates is a manager
  decision**, and the README says that rather than leaving a green badge to imply
  coverage.
- **The registry was found to be silently omitting two repositories.**
  `docs` and `cafaye-rb` both carry a valid `cafaye.yml` on master and appeared
  nowhere in this repository. The existing coverage test could not catch this:
  it checks that the names it knows are registered or excluded, and a repository
  nobody added to that hand-maintained list is invisible to it.
  `no_workspace_repository_is_missing_from_the_curation_lists` walks the
  workspace instead and asks from the other direction, which is the only
  direction that catches a repository nobody remembered. Both are recorded in its
  `UNDECIDED` constant with a reason and a `DECISION NEEDED`, because no
  `blockedBy` value honestly describes "valid manifest, not a service" — the
  vocabulary has `schema`, `no-manifest` and `not-a-service`, and a static site
  and a library are none of them.
- **No response shape changed.** No field was added, removed or renamed, so
  `info.version` stays at `1.0.0` and no `caf pantry` client needs regenerating
  for this packet.

### Fixed

- **`registry/services/identity/cafaye.yml` was stale, and the field comparison
  had been reporting it as WRONG rather than as out of date.** `identity-08` has
  landed: it added the scoped API token routes, so identity's own manifest now
  publishes two more events (`identity.api_key.created`,
  `identity.api_key.revoked`) and a description that names scoped API tokens. The
  copy in this repository had neither, so `every_registered_entry_matches_the_
  real_service_on_disk` and `every_registered_entry_is_a_verbatim_copy_of_the_
  services_own_bytes` were both red. Fixed with the `cp` the failure message
  prints, which is the whole of the fix — a registry copy of another
  repository's file is refreshed, never edited.

  **It is in its own commit** because it has nothing to do with registering
  `cafaye-ts`, and a reviewer should be able to see the two findings apart: this
  one is drift the tripwire caught on its own, and the other is a repository the
  registry never knew about.

## [0.1.0] — 2026-09-30

The registry, official-only, as a service with an HTTP surface and as data in
this repository.

### Added

- **`GET /v1/services`** — the official cafaye service set, sorted by name, with
  `?kind=`, `?language=`, `?contract=` filters and `?limit=` / `?cursor=` paging
  per core's conventions. A filter that matches nothing is `200` with `"data":
  []`, never a `404`.
- **`GET /v1/services/{name}`** — one entry as the same object the list wraps, or
  `404` with core's `application/problem+json` envelope.
- **`GET /healthz`** — liveness. `200` whenever the process is running, including
  when the registry failed to load, so a data problem does not become a restart
  loop.
- **`GET /readyz`** — readiness. `200` only when the registry loaded and every
  entry validated against core's manifest schema; `503` naming what could not be
  read.
- **`registry/`** — the official service set as repository data: one
  `registry/services/<name>/cafaye.yml` per service, a verbatim copy of that
  service's own manifest, plus an index holding the two facts a manifest cannot
  carry (`kind`, `basePath`) and a machine-checked exclusion record for the
  repositories that are known and deliberately held back. The layout is one
  directory per entry because `caf contract lint` only lints files named exactly
  `cafaye.yml`, so the platform's own CLI validates the whole registry:
  `caf contract lint registry/services`.
- **`openapi/v1.yaml`** — pantry's own committed HTTP contract, the machine half
  of the response shape the `caf pantry` packet consumes. A test holds it against
  the router in both directions.
- **`schemas/cafaye.manifest.schema.json`** — core's manifest schema, vendored so
  the binary can validate every entry at startup. A test asserts the copy is
  byte-identical to core's while a workspace is reachable.
- **The cafaye constraint grammar**, ported from `caf/internal/contract` so there
  is one resolver in the platform rather than two. `?contract=` matches by range
  intersection, because a manifest holds a constraint rather than a version.
- **`X-Trace-Id` on every response**, with W3C `traceparent` propagation per
  PLAN.md §7.

### Decisions worth the changelog

- **A service object is its manifest.** Every key is a key of that service's
  `cafaye.yml`, spelled as the manifest spells it, plus `kind` and `basePath`.
  The two exceptions exist because core closes the manifest with
  `additionalProperties: false`; inventing vocabulary there breaks every consumer
  of the manifest, and therefore every consumer of this registry. Nothing is
  translated, so nothing can drift.
- **`basePath` is core's `/vN` rule, not the longest common path prefix.** Those
  look equivalent and are not: muse publishes one path, `/v1/route`, whose longest
  common prefix is a resource rather than a base. A document with no single `/vN`
  prefix is an error naming core's rule rather than a prefix invented to hide it.
- **`kind` is derived from the manifest wherever the manifest can speak, and
  curated where it cannot.** Exactly one entry is curated — guard, which declares
  no `exposes` at all and records that as a `DECISION NEEDED` in its own manifest
  — and `Registry::load` refuses the curated answer the moment the manifest
  becomes decisive.
- **`language: spec` cannot be registered.** core's schema defines it for
  specification-only repositories, so `core` is in the exclusion record with the
  reason rather than in the registry.

### Not in this release

Official-only, deliberately. A marketplace, third-party submissions, a
registration webhook, a plugin loader, a UI, a database, and health polling of
other services are all Phase 5 (PLAN.md §4b item 3). pantry describes how to reach
a service and never calls one. The reasoning is in `README.md`,
"Deliberately not built".

## Versioning

core is pre-1.0, so **Minor** carries breaking changes to the HTTP contract:

| Change | Version |
| --- | --- |
| Breaking change to a response, a filter or a code | Minor |
| New endpoint, new optional field, new filter | Patch |
| Internal refactor with no wire change | Patch |

`info.version` in `openapi/v1.yaml` moves with the contract and is what a
generated client records. A breaking change bumps it and adds a row here under
"Unreleased" — which is where the next packet starts.
