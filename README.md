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
| no contract surface, and the repository serves HTTP | `api` (curated) |
| no contract surface, and the repository is a binary | `cli` (curated) |
| no contract surface, and it is an installed artifact with no entry point | `cli` (curated) |

The last three rows are the honest gap, and they are **two values rather than
three because a manifest cannot tell the cases apart**. A repository that
declares no contract surface says nothing about what it is: guard (a gateway
whose OpenAPI document has not been written yet) and caf (a binary — "caf is a
binary, not a service: it exposes no HTTP surface and publishes no events")
declare the same absence, and so does cafaye-ts, which serves nothing and
vendors six other repositories' OpenAPI documents into `specs/`. core's own rule
3 says such a repository "is a library or a spec repo", which is none of the
three. `language` does not help — it names a toolchain, not a shape.

So the distinction is recorded by a person in `registry/index.yml`, and
`Registry::load` holds it honest:

* for a manifest with no contract surface, **only** `api` and `cli` are
  admitted, and the refusal names both — a fifth value added to the vocabulary
  has to be given an answer there or the load says nothing about it;
* `cli` is refused the moment the manifest declares a surface, and refused a
  non-null `basePath`, because a binary publishes no document for one to be
  derived from.

`caf` was `api` before `cli` existed. That was a value the vocabulary offered for
a command, and a client reading `kind: api` and routing to caf would have found
a binary with no HTTP surface. The `> DECISION NEEDED (pantry)` on caf's row in
`registry/index.yml` is now the **next** question rather than this one: core's
manifest schema has no way to say "I am a binary" either, so `cli` stays
curated. Every other `kind` in the registry is checked against the manifest at
load.

**cafaye-ts is the second `cli`, and it is where that gap started costing
something.** It is a package, not a command — no `bin`, no entry point,
`dependencies: {}` — so "installed and run" is only half true of it, and the
other half of the answer is that it is *imported*. The vocabulary has no word
for that, and the price is visible one screen down: `cafaye-rb` is this
repository in Ruby, the same shape with the same absence, and it is held back as
`library` rather than registered. **That is a DECISION NEEDED, not a settled
distinction** — see `DECISIONS.md` D1 for the three options and which one is
recommended, and the `> DECISION NEEDED (pantry-06, D1)` block on cafaye-ts's row
for the argument the reviewer of a registration needs first. It is recorded
rather than resolved because resolving it means moving a row another packet
wrote, and that is not this packet's to do.

### `basePath`

The `/vN` prefix every contract path in the service's OpenAPI document sits
under — `/v1` for six of the nine entries, `null` for **guard**, **caf** and
**cafaye-ts**, which publish no document for the rule to read. cafaye-ts is the
one that looks like an exception: it vendors six OpenAPI documents into `specs/`,
one per service that has one. Those are **inputs**, copied at recorded commits
and provenanced in `specs/index.json`, and a base path is derived from a document
a service *publishes* — so deriving one from a document this package copied would
be core's longest-common-prefix guess, on six different documents.

**A partial document is still a document — and the next one will be too.**
courier's OpenAPI document *was* partial: it covered `/v1/webhook_endpoints`
only and said in its own header that the notification preferences routes were in
the router and not in the file. `courier-05` closed that, and the document now
covers every route the router serves except the two probes — but the rule it was
registered under is unchanged, because the next service to publish an incomplete
document has to meet it too. A partial document does not make the base path a
guess: every path it *publishes* is under `/v1`, which is core's rule, and a
document that is partial is one pantry can still derive from. What pantry refuses
to do is invent a prefix for a document that publishes none, or average two
prefixes into one — both are refused, with the rule named, by
`a_partial_openapi_document_still_yields_a_base_path_from_the_paths_it_publishes`
in `tests/manifest.rs`. `basePath` is re-derived from the service's own file on
every run, so the day any service documents a path under a second prefix its row
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
    ├── cafaye-ts/cafaye.yml
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
`tests/recorded_copy.rs` verifies every copy against the service **at the commit
this registry records** on every run — **byte for byte**, and also field by field
— and `tests/drift.rs` checks the two registry-side facts against the service's
own OpenAPI document and git remote. That test is the mechanism. Discipline is
not.

**And the copy records where it came from.** Every row in `registry/index.yml`
carries a `recordedAt`: the commit of that service's own repository the copy was
taken from. The copy is therefore a copy *of a named commit*, and the check is
"this copy is what service S said at ref R" — a claim that is true or false for a
reason inside this repository.

That is a real change, and the reason is attribution. When the comparison was
made against the sibling checkout's **working tree**, the claim was "this copy is
what the service says right now", so a merge in `identity` or `muse` turned
*pantry's* gate red and was reported as *pantry* being broken. It happened three
times; once, muse's copy was publishing `required: false` for a dependency muse
had since made **required** — a muse without identity is 503 on every request —
and nothing in the failure said whose repository had moved.

How far behind a copy is has therefore become a **report**, not an assertion:
`the_registry_says_how_far_behind_each_copy_is_and_names_the_fix` prints every
service's distance on every run and fails only past a 9-commit budget. A stale
copy is legal — it is a copy that has not been bumped yet — and a scheduled
report that is red every week is a report that gets muted. See `DECISIONS.md`
D4.

### A copy is verbatim, comments included

`registry/services/<name>/cafaye.yml` is a byte-for-byte copy of the service's
own file, and `every_registered_copy_is_verbatim_at_the_ref_this_registry_records`
is what keeps it that way — checked at the `recordedAt` commit in
`registry/index.yml`, not at the sibling checkout's working tree. It fails with
the `cp` that fixes it, names the first line that differs, and says whether the
YAML fields moved as well or only the comments did.

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
`no-manifest`, `not-a-service` or `library`. `tests/schema.rs` asserts each
reason still holds, so the record is a tripwire in one direction: the day
courier's events gained the three-segment prefix core requires, its row failed
with *"now validates — register it"* instead of the registry quietly going stale.
courier-03 renamed them, and pantry-03 registered it.

| repository | held back because | re-verified against the checkout on |
| --- | --- | --- |
| `parlor` | still the pre-core draft shape (`apiVersion: cafaye/v0-draft`, `metadata`/`spec`); an app shell, not a platform service. `caf contract lint`: `is missing required fields ["name", "language", "core", "repository", "owner"]` | 2026-09-30 (pantry-05) |
| `kit` | carries no `cafaye.yml`; configuration only, and its own AGENTS.md says "not a CLI, a package, or a service" | 2026-09-30 (pantry-05) |
| `core` | `language: spec` — a specification, not a service. `Registry::load` refuses any `spec` manifest, so this stays true if someone copies one in. `caf contract lint`: `OK` | 2026-09-30 (pantry-05) |
| `docs` | `library` — the documentation site. A valid manifest with no `exposes` on purpose: its own file says it "serves no HTTP traffic of its own … The same shape a library takes". A static site is depended on, not started. `caf contract lint`: `OK` | 2026-09-30 (pantry-05) |
| `cafaye-rb` | `library` — the shared Ruby gem. core's schema says to "omit `exposes` entirely for libraries" and its manifest does, and says why: "it is not deployed, serves no traffic and publishes no events". `caf contract lint`: `OK` | 2026-09-30 (pantry-05), on a local checkout — see below |
| `cafaye-py` | `no-manifest` — **a directory, not a repository.** No files, no checkout, no manifest: the planned hand-written Python client, which MD6 ruled hand-written rather than generated. The row is here so the registry has an opinion about a directory a reader can see, and so a `cafaye.yml` appearing there fails the suite | 2026-09-30 (pantry-06) — and **the check on it is vacuous in CI**, see below |

The first five were re-read against their checkouts in pantry-05, which was the
fourth packet to do that by hand and each time it was necessary: every one of the
first three had a reason another repository's packet made false.

**The sixth row cannot be re-read, and saying so is why it is written down.**
`cafaye-py` is a directory with nothing in it, so there is no checkout to confirm
the reason against, and in the `workspace-drift` job it does not exist at all:
the `no-manifest` arm asserts the manifest is *absent*, and a directory that was
never cloned satisfies that. **A green run does not mean this row was checked.**
It is verified by a developer's run of `tests/schema.rs` with
`PANTRY_CAFAYE_ROOT` set, and by nothing else.

It *is* named in `CAFAYE_UNREADABLE`, which this packet first claimed it would
not be and was wrong about — `the_drift_job_clones_every_repository_pantry_curates`
refuses a curated name the job neither clones nor declares unreadable, and the
clone would fail the job's clone step on a 404. The entry is there with the
reason written beside it, and that reason is a different one from `cafaye-rb`'s:
nothing about `cafaye-py` is *unreadable*, because there is nothing there to
read. One list now carries two different claims, which is the other half of
`DECISIONS.md` **D2**'s argument for a fifth `blockedBy` value that says
"planned" instead of "not readable".

What the row does buy is a live tripwire. A `cafaye.yml` appearing in that
directory fails `every_exclusion_reason_is_still_true` with *"now carries a
cafaye.yml — register it or change this row's blockedBy and say why it is still
held back"*, so an empty directory cannot be tolerated indefinitely by doing
nothing. That `no-manifest` is a slight overstatement — the value presumes a
repository, and there is not one yet — is `DECISIONS.md` **D2**, open.

### `library` — valid, and not something anyone brings up

The fourth `blockedBy` value, and the reason it exists rather than a branch in a
test. `docs` and `cafaye-rb` both carry a **valid** `cafaye.yml` and neither is a
service anything starts, yet none of the original three values describes them:
they are not `schema`-invalid, not `no-manifest`, and not `not-a-service` —
which means `language: spec`, and a Starlight site is `typescript` and a gem is
`ruby`. Both record that gap about their own `language` value in their own
repositories.

They were invisible until pantry-03's `no_workspace_repository_is_missing_from_
the_curation_lists` walked the workspace and asked from the other direction, and
they were parked in an `UNDECIDED` constant in that test while the question was
open. **That constant is gone**, and its removal is the point: a third list
where an undecided repository needs no reason is a check that can be made green
by not checking. Both have a row and a machine-checked claim now, and the claim
goes stale in the same direction as every other row's — a `library` whose
manifest declares `exposes` or a non-empty `consumes` is something `caf dev`
brings up and `guard` routes to, so `tests/schema.rs` fails it with *"now
declares `exposes` … it must be REGISTERED"*.

**Neither is registered**, and that is a decision rather than a gap.
Registration claims `caf dev` can bring the thing up and gives it a base path to
route to. A documentation site is read and a gem is depended on; neither is a
process, and listing them as services would make `kind` mean two different things
in one column.

**And the next entry added to the registry made that sentence false, which is
the useful part.** `cafaye-ts` — the TypeScript client, the npm counterpart of
`cafaye-rb`'s gem, with the same manifest shape and the same deliberate absence
of `exposes` — **is** registered, as a `cli`. So the exclusion record now holds
a client library and the registry serves another, and the reason given above
("registration claims `caf dev` can bring the thing up") does not distinguish
them: neither is brought up. This is `DECISIONS.md` **D1**, open, with three
options and a recommendation; it is recorded rather than fixed because every fix
moves a row this repository did not write. What is *not* left to chance is that
the two rows are now three screens apart and neither comment mentions the other,
which would be the actual rot.

### A green run does not mean this table is accurate

`every_exclusion_reason_is_still_true` reports exclusions that have gone
**stale**. It says nothing about whether the six reasons still **hold**. Those
are different questions and only one of them is machine-checked, so a green run
is not evidence that this list is right — it is a reason to go and read the
checkouts. That has been necessary on four consecutive packets (darkroom, caf,
courier, and pantry-05), and each time it was: every one of the first three
exclusions had a reason that another repository's packet made false. One of the
six is not checkable at all, and its row says which and why.

Two rows are checked in fewer places than the other four, for two different
reasons. **`cafaye-rb` is a private repository**, so an anonymous runner cannot
clone it — the GitHub API
answers `404` for it without a credential, which reads as "does not exist" rather
than "not yours". The `workspace-drift` job names it in `CAFAYE_UNREADABLE`, its
reason is verified by a developer's run of `tests/schema.rs` and by nothing else,
and `tests/schema.rs` prints a `SKIP` line saying so rather than passing quietly.
`tests/ci.rs` keeps that list honest in both directions: a registered service may
never be on it, a name the registry has stopped curating may not be on it, and
nothing on it may also be cloned. The other answer — a credential in the
workflow — is what pantry-04 removed on the grounds that the fleet is public, and
adding it back for one gem would be worse than the gap.

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

### The drift test needs a workspace, and CI builds one

**Read this before reading a green badge on this repository.**

`registry/` is a set of copies of other repositories' files, and it is
`tests/recorded_copy.rs` that makes those copies true. It reads a cafaye
workspace — a directory holding `core/`, `identity/` and the rest — next to this
checkout, or at `PANTRY_CAFAYE_ROOT`. Those are sibling checkouts under
`moon/cafaye/`. `tests/drift.rs` does the same for the curated `kind` and
`basePath` facts.

The clones must be **non-shallow**, and CI's is: `git show <sha>:<path>` cannot
reach a commit a `--depth 1` clone does not have, so a shallow clone would turn
every recorded-ref check into a skip. `tests/ci.rs` fails if `--depth` comes
back.

With a workspace, these tests run and compare. Without one, **25 of them print
`SKIP …` on stderr naming the directory that would make them run**, and return. A
skip is reported, never hidden. The full gate, run by hand:

```console
$ PANTRY_CAFAYE_ROOT=/Users/kaka/Code/any/moon/cafaye ./bin/prime
```

The shape of the problem is that a pantry-only clone and a real workspace print
**the same 146 passing tests and the same exit code 0**. The only difference
between them is those 25 `SKIP` lines — and, since pantry-05, one more for the
private repository this job cannot clone (see "Not registered, and why" below).
Nothing in a green run distinguishes
"verified the fleet" from "verified itself" — which is why the job that has the
fleet has to be a separate job with a name that says so.

**Count them, do not eyeball them.** Both numbers above were measured by running
the suite in a clone with no siblings beside it and comparing:

```console
$ git clone --no-hardlinks --no-local ../pantry /tmp/lonely/repo && cd /tmp/lonely/repo
$ cargo test --no-fail-fast -- --nocapture 2>&1 | grep -c '^SKIP'
25
```

`cargo test` prints skips on stderr, which a terminal shows but a captured
pipeline does not. A gate that reports "146 passed" without also reporting how
many of those 146 verified nothing is the defect this repository has been
reporting against itself three times, so the count is part of the claim.

**The `wt-*` worktree rule is deliberately in the half that does not skip.** Two
of pantry's worktree tests build their own workspace and never ask for a real
one, so the rule is proven in a pantry-only clone; only the test that *reports*
what the walk skipped on your actual disk skips without a workspace. The bug
that rule came from was a rule that was wrong about every shape actually
present, and a check that only runs where the shapes are is how that happens.

**A green badge on the `build` job has verified pantry against itself, not
against reality.** That job is a clone of pantry alone. It proves the registry
is internally consistent, that every entry satisfies core's vendored schema,
and that the filter and paging contracts hold. It proves **nothing** about
whether any entry still says what its service says. It is a fast inner loop,
not the gate on the registry.

**The `workspace-drift` job is the gate, and it runs.** It clones the whole
cafaye organisation beside the `pantry` checkout and runs the same
`./bin/prime` with `PANTRY_CAFAYE_ROOT` pointed at what it cloned, so the drift
tests compare instead of skipping. Every cafaye repository is public, so the
clones are anonymous HTTPS and the job uses no secret of any kind. A green run
of that job means, as of that run: every registered entry is a verbatim byte
copy of the service's `cafaye.yml` **at the commit that row records**, every field
pantry publishes about a service is a field the service publishes, each curated
`kind` and `basePath` agrees with the service's own manifest and OpenAPI
document, the vendored schema is core's **at the ref `vendir.lock.yml` records**,
every exclusion row's reason is still true, and the platform CLI's own
`contract lint` passes over all of it.

It also prints, on every run, how far each recorded ref is from what it was taken
against. A copy that has fallen behind is reported by name with the fix and does
**not** turn this gate red until it passes the budget — a merge in another
repository is a fact about that repository, and this gate is not where it should
be reported.

It is still worth reading what that does not cover, and the job says the same
three things on its own face:

- it reads each sibling's **default branch as of the run**, so a change in
  `identity` that has not reached `identity`'s master is invisible here;
- it proves the registry is **true, not complete** — a cafaye repository nobody
  has added to the job's clone list is not cloned, not registered and not
  noticed, and the list has to be updated by hand. `tests/ci.rs` is what makes
  that hand-maintained list mean something: it fails the `build` job when
  `registry/index.yml` curates a repository the job neither clones nor declares
  unreadable;
- a red there is almost always a finding about **another** repository, and the
  fix is to report it and open the change where the file lives. `registry/`
  here is a copy of somebody else's file and this repository does not own it.

**And one curated repository is not in that workspace at all.** `cafaye-rb` is
private, so the job cannot clone it and its exclusion row's reason is not checked
there — verified by a developer, with a `SKIP` line saying which row and why. It
is a real gap in the coverage and it is named in the workflow rather than left to
be inferred from a green badge; see "Not registered, and why" above.

The job also guards its own workspace, because a clone that quietly did not
happen is the one failure whose symptom is a green run: without core's schema
at the expected path, `cafaye_root()` misses and every drift test skips, so a
`prove the workspace is real` step checks every checkout for git metadata and
core's schema before the gate runs.

A pantry-only clone still proves only that pantry is well-formed, and the way
to get that wrong is to delete the clones rather than the job — so `tests/ci.rs`
also fails if the drift job is disabled, if a secret reference reappears, or if
`PANTRY_CAFAYE_ROOT` stops pointing at the checkouts.

---

## Layout

```
pantry/
├── bin/prime                  # the gate
├── cafaye.yml                 # this service's own manifest
├── DECISIONS.md               # this repository's open decisions, D1, D2, …
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
└── tests/                     # manifest, schema, contract, filters, api, drift, ci
```

Read `AGENTS.md` before changing anything here.

## License

MIT. See [LICENSE](LICENSE). `Cargo.toml` declares the same thing in its
`license` field.

pantry is the registry the fleet is consumed through, so the licence a
dependency arrives under is the licence the dependency graph hands on. MIT
keeps a consumer's own licensing situation unchanged by adding pantry, which is
the whole reason the registry model works.
