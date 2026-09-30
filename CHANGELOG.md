# Changelog

All notable changes to `pantry` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project is
pre-1.0, so **Minor** carries breaking changes — see the versioning note at the
bottom.

## [Unreleased]

### Added

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
