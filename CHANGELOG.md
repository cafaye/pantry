# Changelog

All notable changes to `pantry` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project is
pre-1.0, so **Minor** carries breaking changes — see the versioning note at the
bottom.

## [Unreleased]

Nothing yet.

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
