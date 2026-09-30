# pantry

**The cafaye service registry.** What a cafaye service *is*: its identity, the
contract it exposes, and how to reach it — so `caf dev`, `caf deploy` and a
developer reading the platform read one answer instead of a hardcoded list in
each of them.

```console
$ curl -s localhost:8080/v1/services | jq '.data[].name'
"billing"
"caf"
"courier"
"darkroom"
"guard"
"identity"
"muse"
"pantry"
```

---

## The response shape, for the `caf pantry` client

This is the contract. `openapi/v1.yaml` is the same contract in machine form and
is what a client should be generated from; the two are held against each other by
`tests/api.rs`, which fails when the router and the document disagree in either
direction.

### `GET /v1/services`

Filters, all optional and ANDed: `?kind=api|worker|both`,
`?language=go|ruby|elixir|python|typescript|rust`, `?contract=^0.2.0`. Paging:
`?limit=` (1–100, default 25) and `?cursor=`.

```json
{
  "data": [
    {
      "name": "identity",
      "description": "Authentication, sessions, MFA, OAuth, accounts and tenancy, and the OIDC provider.",
      "language": "go",
      "kind": "api",
      "core": "^0.1.0",
      "basePath": "/v1",
      "exposes": {
        "api": "openapi/v1.yaml",
        "events": ["identity.user.created"]
      },
      "consumes": [],
      "dependencies": [],
      "repository": {
        "url": "git@github.com:cafaye/identity.git",
        "defaultBranch": "master",
        "visibility": "public"
      },
      "owner": {
        "team": "identity",
        "contact": "identity@cafaye.com"
      }
    }
  ],
  "page": { "next_cursor": null, "has_more": false }
}
```

### `GET /v1/services/{name}`

The **same object**, not wrapped — core's convention wraps collections in
`data`/`page`, and a single resource is not a collection. Unknown name: `404`
with the problem envelope below.

### Where every field comes from

| field | source |
| --- | --- |
| `name`, `description`, `language`, `core`, `exposes`, `consumes`, `dependencies`, `repository`, `owner` | the service's **own `cafaye.yml`**, spelled as the manifest spells it |
| `kind` | `registry/index.yml` — a manifest cannot state it |
| `basePath` | derived from the service's own OpenAPI document; `null` when it publishes none |

**A service object is its manifest.** Every key is a key of that service's
`cafaye.yml`; the only two that are not are `kind` and `basePath`, and both exist
because core closes the manifest with `additionalProperties: false`. Nothing is
renamed and nothing is translated, so a client that already reads a `cafaye.yml`
reads a pantry response with the same names, and a field that moves in core's
schema moves here in the same commit.

Three rules a generated client has to know:

* **`exposes` is `null`** when the manifest declares no contract surface.
  Absent and empty are different facts and pantry keeps the difference.
* **`consumes` and `dependencies` are `[]`** when absent. No consumer needs to
  tell "declared empty" from "absent".
* **Optional manifest keys are omitted**, not nulled: `description`,
  `defaultBranch`, `visibility`, `contact`, and a dependency's `version` /
  `required` appear only when the manifest states them.

### `kind`

| the manifest declares | kind |
| --- | --- |
| `exposes.api`, no `consumes` | `api` |
| `exposes.api` and a non-empty `consumes` | `both` |
| no `exposes.api`, some event work (`exposes.events` or `consumes`) | `worker` |
| no contract surface at all | curated |

The last row is the honest gap: a service that declares nothing cannot be
classified from its manifest, so the registry states it and `Registry::load`
refuses the curated answer the moment the manifest becomes decisive. Two entries
are curated today, and they are curated for different reasons:

* **guard** — serves HTTP, but has no OpenAPI document yet, so there is nothing
  for its manifest to point `exposes.api` at. Its own file records this as a
  `DECISION NEEDED`.
* **caf** — the platform CLI, which omits `exposes` on purpose: "caf is a
  binary, not a service: it exposes no HTTP surface and publishes no events."

Both say `api`, and for `caf` that is a choice under constraint rather than a
true answer — `worker` and `both` are both refused by the checks for a manifest
that declares no surface. `> DECISION NEEDED (pantry)` in `registry/index.yml`
proposes a fourth value for binaries. Every other `kind` in the registry is
checked against the manifest at load.

### `basePath`

The `/vN` prefix every contract path in the service's OpenAPI document sits
under — `/v1` for six of the eight entries, `null` for **guard** and **caf**,
which publish no document for the rule to read.

**A partial document is still a document.** courier's OpenAPI document covers
`/v1/webhook_endpoints` only and says in its own header that the notification
preferences routes are in the router and not in the file. That does not make
`/v1` a guess: every path courier *publishes* is under `/v1`, and every
non-probe route in courier's router is under `/v1` too, so the routes it has not
documented yet cannot move the prefix. What pantry refuses to do is invent a
prefix for a document that publishes none, or average two prefixes into one —
both are refused, with the rule named, by
`a_partial_openapi_document_still_yields_a_base_path_from_the_paths_it_publishes`
in `tests/manifest.rs`. `basePath` is re-derived from the service's own file on
every run, so the day courier documents a path under a second prefix this row
fails rather than drifting.

It is core's rule and not a pantry invention: `docs/openapi-conventions.md` says
every path carries one, that the prefix *is* the API version, and that `caf
contract lint` will enforce it. Note it is **not** the longest common path
prefix: muse publishes exactly one path, `/v1/route`, whose longest common prefix
is `/v1/route` — a resource, not a base. A document with no single `/vN` prefix
is an error, not a prefix invented to hide it.

### Errors

Every non-2xx is RFC 9457 `application/problem+json` with core's extensions — no
service invents its own error body, so this includes the framework's own `404`
and `405`:

```json
{
  "type": "https://errors.cafaye.com/not_found",
  "title": "Not found",
  "status": 404,
  "detail": "no official cafaye service is named \"nope\"",
  "instance": "/v1/services/nope",
  "code": "not_found",
  "trace_id": "0af7651916cd43dd8448eb211c80319c"
}
```

| status | `code` | when |
| --- | --- | --- |
| 400 | `validation_failed` | a filter value outside its vocabulary, an empty parameter value, an unknown parameter, a limit out of bounds, an unusable cursor |
| 404 | `not_found` | unknown service name, unknown route |
| 405 | `method_not_allowed` | the route exists, the method does not |
| 503 | `unavailable` | the registry did not load |

**Two decisions flagged for the manager**, both written down rather than guessed
at silently:

1. `method_not_allowed` (405) is **not** in core's reserved code list. A client
   generated from this document needs to branch on it, and core's conventions say
   the reserved list is where such codes go — so it is recorded here for a core
   amendment rather than invented quietly.
2. **Cursors do not expire.** core says a cursor older than 24 hours answers
   `cursor_expired`; the registry is repository data that does not change between
   deploys, so there is nothing for a stale cursor to mis-read.

### Probes

| route | meaning |
| --- | --- |
| `GET /healthz` | **liveness.** `200` whenever the process is running — *including when the registry failed to load*. A process that cannot serve is still alive, and an orchestrator that restarts on this turns a data problem into a crash loop. |
| `GET /readyz` | **readiness.** `200` only when the registry loaded and every entry validated against core's schema, with `services` counting what loaded. `503` with a `detail` naming what could not be read. |

Every response carries `X-Trace-Id`, 32 lowercase hex. An inbound W3C
`traceparent`'s trace-id is propagated unchanged (PLAN.md §7), so one call chain
has one id end to end; a malformed header is ignored rather than echoed.

---

## The registry

```
registry/
├── index.yml                    # the two facts a manifest cannot carry + what is held back
└── services/
    ├── billing/cafaye.yml       # verbatim copies of each service's own manifest
    ├── caf/cafaye.yml
    ├── courier/cafaye.yml
    ├── darkroom/cafaye.yml
    ├── guard/cafaye.yml
    ├── identity/cafaye.yml
    ├── muse/cafaye.yml
    └── pantry/cafaye.yml
```

One **directory** per entry, each holding a file named exactly `cafaye.yml`, and
that is not cosmetic: `caf contract lint` only lints files with that exact name,
so this layout means the platform's own CLI validates every registry entry —

```console
$ cd ../caf && go run ./cmd/caf contract lint ../pantry/registry/services
OK ../pantry/registry/services/billing/cafaye.yml
OK ../pantry/registry/services/darkroom/cafaye.yml
...
```

— rather than the registry being checkable only by the thing it is. A flat
`<name>.cafaye.yml` looks tidier and is a directory the canonical validator walks
straight past.

**There is no database.** Phase 1 has none, and that is a property rather than an
omission: a registry whose entries can be written at runtime is a registry that
can be given a new service by anything that can reach the process. Adding a
service is a commit and a reviewed pull request — that is the entire trust model.

**Why a copy at all.** A container has no sibling checkouts, so the registry has
to be self-contained; but a copy nobody checks is a copy that rots. So
`tests/drift.rs` verifies every copy against the real service on every run —
**byte for byte**, and also field by field, plus the two registry-side facts
against the real service's OpenAPI document and git remote. That test is the
mechanism. Discipline is not.

### A copy is verbatim, comments included

`registry/services/<name>/cafaye.yml` is a byte-for-byte copy of the service's
own file, and `every_registered_entry_is_a_verbatim_copy_of_the_services_own_bytes`
is what keeps it that way. It fails with the `cp` that fixes it, names the first
line that differs, and says whether the YAML fields moved as well or only the
comments did.

The reason is worth stating, because it is not tidiness. **These files carry
their services' `DECISION NEEDED` blocks**, and a copy that has silently lost one
is not a stale comment — it is the registry answering a reviewer's question
wrongly. A reviewer asking "what does pantry think guard is" would have been told
a gateway with three open questions has none, because the copy predated guard-04's
`REDIS_URL` block. The same was true of billing and nobody had reported it. A
comment-only edit upstream is therefore drift here, and the fix for it is
mechanical rather than a judgement call.

This settles a contradiction that was live until this packet: `AGENTS.md` said
copies are kept "verbatim, including its comments" while the drift test's own doc
comment said "a comment-only edit upstream is not drift". Both were in this
repository and they cannot both have been true. The field comparison is kept as
well as the byte comparison — it is the one that names *which field* moved — but
byte-equality is the check that catches a copy nobody has refreshed.

### Registering a service

1. The service's `cafaye.yml` must validate:
   `cd ../caf && go run ./cmd/caf contract lint ../<service>/cafaye.yml`
2. Copy it to `registry/services/<name>/cafaye.yml` — **byte for byte,
   comments included.** A drifted copy is a failed test, not a nit.
3. Add a row to `registry/index.yml` with `kind` and `basePath`, and a comment
   saying why those two values are what they are. They are the only facts
   pantry states that a manifest cannot state for itself, so they are the only
   facts a reader cannot get from the service's own file.
4. `cargo test --test drift`. If `basePath` is wrong the test says what the real
   document says instead; if the copy is stale it says which service, which
   line, whether fields moved too, and the `cp` to run.

`Registry::load` refuses, with a message saying what to do: a file misnamed for
its service, a manifest with no index row, an index row with no manifest, a
`language: spec` manifest, an unparseable `core` constraint, and a `kind` that
the manifest contradicts.

### Not registered, and why

`registry/index.yml` carries an exclusion record: a known repository, a reason, a
command that proves it, and a machine-checked `blockedBy` — `schema`,
`no-manifest` or `not-a-service`. `tests/schema.rs` asserts each reason still
holds, so the record is a tripwire in one direction: the day courier's events
gained the three-segment prefix core requires, its row failed with *"now
validates — register it"* instead of the registry quietly going stale. courier-03
renamed them, and this packet registered it.

| repository | held back because | re-verified against the checkout on |
| --- | --- | --- |
| `parlor` | still the pre-core draft shape (`apiVersion: cafaye/v0-draft`, `metadata`/`spec`); an app shell, not a platform service. `caf contract lint`: `is missing required fields ["name", "language", "core", "repository", "owner"]` | 2026-09-30 (pantry-03) |
| `kit` | carries no `cafaye.yml`; configuration only, and its own AGENTS.md says "not a CLI, a package, or a service" | 2026-09-30 (pantry-03) |
| `core` | `language: spec` — a specification, not a service. `Registry::load` refuses any `spec` manifest, so this stays true if someone copies one in. `caf contract lint`: `OK` | 2026-09-30 (pantry-03) |

### A green run does not mean this table is accurate

`every_exclusion_reason_is_still_true` reports exclusions that have gone
**stale**. It says nothing about whether the three reasons still **hold**. Those
are different questions and only one of them is machine-checked, so a green run
is not evidence that this list is right — it is a reason to go and read the
three checkouts. That has now been necessary on three consecutive packets
(darkroom, caf, courier), and each time it was: every one of those exclusions
had a reason that another repository's packet made false.

**The record is not a queue, and leaving it is not a way to avoid a decision.**
`caf` and `courier` both sat in this list with one fact and one opinion each,
and both left by being registered — never by teaching
`every_exclusion_reason_is_still_true` that their case is exempt. An exemption
branch is a check that has stopped checking, and a carve-out here would be a
weakened check, which PLAN.md §1 forbids. A service that has declared itself in
the platform's own contract language is in the fleet whether or not anything
routes to it.

---

## Deliberately not built

Phase 1 is official-only and curation-only. These are Phase 5 (PLAN.md §4b item
3) and building them now is how a registry turns into an unaudited
code-execution surface:

* **No marketplace and no third-party submissions.** No registration webhook, no
  self-service, no untrusted manifest accepted at runtime.
* **No plugin loader.** pantry reads YAML from a directory this repository owns.
* **No UI.**
* **No database**, and no cross-service database access. pantry owns nothing
  persistent; it holds the registry in memory for the life of the process.
* **No health polling of other services.** pantry describes how to reach a
  service and never calls one. No probe of any cafaye service is made from this
  process, so `pantry` cannot become a load-bearing dependency of the platform's
  availability.
* **No `caf pantry` subcommand.** `caf` is a separate repository and a separate
  packet. The response shape above is the interface that packet consumes.

What *is* left room for: a `kind` and a `basePath` a curated record already
carries, an `exposes` that a service fills in when it has a document, and the
exclusion record's shape — which is what a submission queue would grow out of if
Phase 5 ever wants one.

---

## Running it

```console
$ cargo run                     # reads ./registry, binds 0.0.0.0:8080
$ PANTRY_BIND=127.0.0.1:9000 PANTRY_REGISTRY_DIR=./registry cargo run
$ docker build -f docker/Dockerfile -t pantry .
```

Two variables, both in `.env.example`: `PANTRY_REGISTRY_DIR` and `PANTRY_BIND`.
Logs are JSON, filtered by `RUST_LOG`.

```console
$ ./bin/prime                   # fmt, build, clippy -D warnings, test, contract lint
$ PANTRY_CAFAYE_ROOT=/Users/kaka/Code/any/moon/cafaye ./bin/prime
```

### The gate

`cargo fmt --check`, `cargo build --all-targets`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --no-fail-fast`, and
`caf contract lint` on this repository's own manifest when `../caf` is present.

**No test touches the network or a database.** `tower::ServiceExt::oneshot`
drives the axum router in-process, and `jsonschema` is built with
`default-features = false` so it has no HTTP fetcher to reach out with.

### The drift test needs a workspace

`tests/drift.rs` reads the real services, which are sibling checkouts under
`moon/cafaye/`. Run the suite from inside that directory and it finds them; set
`PANTRY_CAFAYE_ROOT` to point at it from anywhere else. With neither, the drift
tests print a `SKIP` on stderr naming the directory that would make them run and
return. **A skip is reported, not hidden** — a green run without a workspace has
verified pantry against itself, not against reality. CI runs the rest of the gate
and says so in the job's comment.

---

## Layout

```
pantry/
├── bin/prime                  # the gate
├── cafaye.yml                 # this service's own manifest
├── openapi/v1.yaml            # the HTTP contract, machine half of the table above
├── registry/                  # the official service set, as data
├── schemas/                   # core's manifest schema, vendored
├── src/
│   ├── contract.rs            # the constraint grammar, ported from caf
│   ├── filter.rs              # ?kind, ?language, ?contract, ?limit, ?cursor
│   ├── http.rs                # the four routes and the two probes
│   ├── manifest.rs            # a parsed cafaye.yml, core's fields only
│   ├── problem.rs             # RFC 9457 with core's extensions
│   ├── registry.rs            # load, validate, index, exclusions
│   └── view.rs                # the response shape, and its provenance
└── tests/                     # manifest, schema, contract, filters, api, drift
```

Read `AGENTS.md` before changing anything here.
