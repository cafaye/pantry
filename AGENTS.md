# AGENTS.md — working in `cafaye/pantry`

> Conventions for this repo. Read before changing anything here. If a rule is
> not written here, it is not a rule.

## What this repo is

- **Name:** `pantry` (`github.com/cafaye/pantry`)
- **Purpose:** the cafaye service registry. What a service *is* — identity,
  contract, health, how to reach it — so `caf dev`, `caf deploy` and product
  developers have one source of truth instead of hardcoded endpoints.
- **Scope:** official cafaye services only, curated in this repository. Phase 1.
  See "Out of scope" below; it is not a preference.
- **Contains:** a Rust service, its committed OpenAPI document, and the registry
  as data.

## The three rules that matter more than the rest

1. **A rule that is not in `core/schemas/` is not a cafaye rule.** Field names,
   patterns and the constraint grammar come from
   `cafaye/core/schemas/cafaye.manifest.schema.json` and
   `cafaye/core/docs/{manifest-conventions,openapi-conventions,event-naming}.md`.
   pantry adds no vocabulary to a manifest. If a fact does not fit in one, it goes
   in `registry/index.yml` and is documented as a registry fact — which is how
   `kind` and `basePath` work.
2. **A copy nobody checks is a copy that rots.** `registry/services/*/cafaye.yml`
   are copies of other repositories' files. `tests/drift.rs` is the only thing
   that makes them true, and it must fail the moment one diverges.
3. **Tests first.** Per PLAN.md §3. Add the test, run it, watch it fail, then make
   it green by changing the implementation — not by loosening the assertion.

## Order of work

1. Write the test in `tests/`.
2. Run it and **show it failing**. A test that has never failed has never been
   proven to test anything.
3. Make it green by changing `src/`.
4. If the failure is the wrong failure, the test is wrong. Fix the test first.
5. `./bin/prime` before every commit.

There are no sleeps in this suite because there is nothing to wait for: the HTTP
tests drive the router with `tower::ServiceExt::oneshot` in-process, and the
registry is read from disk at startup.

## Registering or changing a service

Adding or removing an entry is the change most likely to be wrong, so:

1. `cd ../caf && go run ./cmd/caf contract lint ../<service>/cafaye.yml` — it
   must be `OK`.
2. Copy it to `registry/services/<name>/cafaye.yml`. **Verbatim**, including its
   comments: the copy is what a reviewer reads when asking "what does pantry
   think this service is". The directory is not cosmetic — `caf contract lint`
   only lints files named exactly `cafaye.yml`, so this layout lets the
   platform's own CLI validate the whole registry:

   ```sh
   cd ../caf && go run ./cmd/caf contract lint ../pantry/registry/services
   ```
3. Add or edit the row in `registry/index.yml`, with a comment saying *why* that
   `kind` and that `basePath` are what they are.
4. `cargo test --test drift`. If `basePath` is wrong the test prints what the real
   OpenAPI document says.
5. If you are *removing* a service, add an `excluded` row with a `blockedBy`, a
   reason and a `verify` command. `tests/schema.rs` asserts the reason still
   holds, so a row cannot rot into a fiction.

Never edit a service's real `cafaye.yml` from this worktree. This repository
reads them; it does not own them.

## Changing the response shape

`GET /v1/services` is a published contract and `caf pantry` will be generated
from it. A change is:

1. `openapi/v1.yaml` — the machine half.
2. `README.md` — "The response shape", the human half.
3. `src/view.rs` — the code.
4. A test in `tests/api.rs` that names the field, so the three above cannot
   disagree. `a_service_object_carries_exactly_the_documented_keys` lists every
   key; if you add one, that list is the first thing that should fail.

A **breaking** change also needs `info.version` bumped and a CHANGELOG entry
under "Unreleased". Adding an optional field does not.

Field-naming rules, and they are not negotiable:

- A key that exists in a manifest keeps the manifest's spelling.
  `defaultBranch`, not `default_branch`.
- A key that only pantry has (`kind`, `basePath`) matches the spelling of the key
  it sits beside.
- The envelope is core's, spelled core's way: `data`, `page.next_cursor`,
  `page.has_more`, and a problem's `type` / `title` / `status` / `detail` /
  `instance` / `code` / `trace_id`. Do not rename them.

## The vendored schema

`schemas/cafaye.manifest.schema.json` is a byte copy of core's, and
`tests/schema.rs` asserts it while a workspace is reachable:

```sh
cp ../core/schemas/cafaye.manifest.schema.json schemas/
```

`/readyz` validates every entry against it at startup, so a stale copy makes the
readiness probe a decoration. When core adds a field, copy the new schema in the
same commit that reads it — a manifest field pantry does not know is a field
pantry silently drops.

## Out of scope, deliberately

Phase 1 is official-only. These are Phase 5 (PLAN.md §4b item 3) and building
them now is how a registry becomes an unaudited code-execution surface:

- **No marketplace, no third-party submissions, no registration webhook.** No
  untrusted manifest is ever accepted at runtime. Adding a service is a commit
  and a reviewed pull request.
- **No plugin loader.** pantry reads YAML from a directory this repository owns.
- **No UI.**
- **No database**, and no cross-service database access. pantry owns nothing
  persistent.
- **No health polling of other services.** pantry describes how to reach a service
  and never calls one — not a probe, not a metrics scrape. If pantry could become
  a load-bearing dependency of platform availability, that would be a fact about
  the dependency graph nobody chose.
- **No `caf pantry` subcommand.** `caf` is a separate repository and a separate
  packet.

## Open decisions

Specs are manager-owned: **you draft, the manager decides** (core's AGENTS.md).

Never silently resolve a genuine design question, and never resolve it in a way
that is hard to reverse. Mark it `> DECISION NEEDED (pantry):` in the affected
file — what the choice is, the alternatives, your recommendation, and the cost of
flipping — and report the open list. One number per decision, ever.

Two are already recorded, both in `openapi/v1.yaml` and the README:

1. **`method_not_allowed` (405)** is not in core's reserved code list. Recorded
   here for a core amendment rather than invented quietly. The cost of flipping:
   one slug in `src/problem.rs` and one in the document.
2. **Cursors do not expire.** core says a cursor older than 24 hours answers
   `cursor_expired`; the registry is repository data that does not change between
   deploys, so there is nothing for a stale cursor to mis-read. The cost of
   flipping: a timestamp in the cursor encoding.

## Git

- Primary branch is `master` everywhere (PLAN.md §1).
- Work on `worker/<packet-id>`. **Never push. Never create a remote.** The
  manager merges to `master` after reading the diff and running the suite.
- Remotes for anything cafaye owns are SSH (`git@github.com:cafaye/…`), never
  HTTPS. core's manifest schema rejects an HTTPS remote, so a bad habit fails the
  suite.
- Commit often. A registry change is easier to review as three commits than one.

## Style

- Comments explain **why**, not **what**. A comment restating the line below it is
  noise; a comment recording a decision that would otherwise be relitigated is the
  point.
- Boring beats clever. No framework, no macro, no reflection. A file that needs a
  paragraph to explain is a file that will be misread.
- Errors are for the person reading them: name the file, the field, the value and
  what was allowed. Every rejection in this repo says which rule was broken.
- **Never weaken a check, a threshold or an assertion to get the gate green.** If
  a check is wrong, fix the check and say so in the commit message.
- Bound every long or networked command with `timeout N`.
- Do not touch anything outside this worktree.
