# Changelog

All notable changes to `pantry` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project is
pre-1.0, so **Minor** carries breaking changes — see the versioning note at the
bottom.

## [Unreleased]

### Added

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
