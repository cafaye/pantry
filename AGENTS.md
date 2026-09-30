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
   are copies of other repositories' files, kept **verbatim, comments
   included** — see "Registering or changing a service" for why, and
   `tests/drift.rs` for the byte-equality check that enforces it. A stale comment
   in one of these files is not cosmetic: it is usually a missing
   `DECISION NEEDED`, and a registry copy missing one is a service with open
   questions that looks settled.
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
2. Copy it to `registry/services/<name>/cafaye.yml`. **Verbatim, byte for byte,
   comments included** — and this is enforced, not merely intended:
   `tests/drift.rs::every_registered_entry_is_a_verbatim_copy_of_the_services_own_bytes`
   compares bytes and fails with the `cp` that fixes it. The reason is not
   tidiness: the copy is what a reviewer reads when asking "what does pantry
   think this service is", and a service's `DECISION NEEDED` blocks live in its
   comments. A copy that has silently lost one makes a service with three open
   questions look settled, which is the registry answering a question wrongly.

   The directory is not cosmetic — `caf contract lint` only lints files named
   exactly `cafaye.yml`, so this layout lets the platform's own CLI validate the
   whole registry:

   ```sh
   cd ../caf && go run ./cmd/caf contract lint ../pantry/registry/services
   ```
3. Add or edit the row in `registry/index.yml`, with a comment saying *why* that
   `kind` and that `basePath` are what they are.
4. `cargo test --test drift`. If `basePath` is wrong the test prints what the real
   OpenAPI document says; if the copy is stale it prints which service, which
   line, whether the fields also moved, and the `cp` to run.
5. If you are *removing* a service, add an `excluded` row with a `blockedBy`, a
   reason and a `verify` command. `tests/schema.rs` asserts the reason still
   holds, so a row cannot rot into a fiction. It asserts a row that has gone
   **stale**; it does not tell you a reason that is still **true** — re-read
   each row against its checkout before trusting a green run.

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

Two are open, and both are recorded in `openapi/v1.yaml` and the README:

1. **`method_not_allowed` (405)** is not in core's reserved code list. Recorded
   here for a core amendment rather than invented quietly. The cost of flipping:
   one slug in `src/problem.rs` and one in the document.
2. **Cursors do not expire.** core says a cursor older than 24 hours answers
   `cursor_expired`; the registry is repository data that does not change between
   deploys, so there is nothing for a stale cursor to mis-read. The cost of
   flipping: a timestamp in the cursor encoding.

## Answered decisions, and where they went

Kept, because the reasoning is what stops the next packet from re-opening them,
and because a list that only grows reads as though nothing was ever settled. **Do
not delete an entry to tidy this section** — the numbers were one per decision,
so a later packet inherits a number and never reuses one.

**MD1 — `kind` had no value for a binary.** `caf` is a CLI that publishes a valid
manifest, and `api` was admitted only because `worker` and `both` were both
refused: a value that is merely less wrong, chosen for want of a right one.
Answered by `kind: cli` (pantry-05, 2026-09-30) — `ServiceKind::Cli`, caf's row
says `cli`, and `openapi/v1.yaml` moves to 1.1.0 because a client that switches on
`kind` exhaustively now has a case it has not handled.

The part worth keeping is **why the value is still curated**, because the obvious
next question is why it is not derived like the other three. A manifest that
declares no contract surface cannot say what it is: core's rule 3 says such a
repository is "a library or a spec repo", guard and caf are neither, and their
manifests declare the same absence. So `api` and `cli` are the only two values
admitted for a surface-less manifest, `check_kind`'s curated branch is a **closed
set** rather than a list of refusals, and `tests/kind.rs` holds all of it down.
The question that replaced it is on caf's row: core has no way to say "I am a
binary" either.

**MD2 — `blockedBy` had no value for "valid, and not a service".** `docs` and
`cafaye-rb` carry valid manifests and neither is a service, so none of `schema` /
`no-manifest` / `not-a-service` described them. Answered by `blockedBy: library`
(pantry-05, 2026-09-30), with both recorded and **neither registered** —
registration claims `caf dev` can bring the thing up, and a documentation site and
a gem are depended on rather than started.

The part worth keeping is what was **not** done: the `UNDECIDED` constant in
`tests/drift.rs` is gone rather than kept as a third list. The alternative to a
fourth value was teaching the drift test that libraries are exempt, and an
exemption branch in a check that has caught three real problems is a check that
has stopped checking. The new value's tripwire is the same one in the same
direction: a library whose manifest declares `exposes` or a non-empty `consumes`
is something the platform starts, so the row fails with "register it".

One coverage gap came with it and is named in three files rather than papered
over: `cafaye-rb` is a **private** repository, so the CI job cannot clone it and
that row's reason is verified by a developer's run of `tests/schema.rs` rather
than on every CI run. See `CAFAYE_UNREADABLE` in `.github/workflows/ci.yml` and
`an_unreadable_repository_is_neither_cloned_nor_registered` in `tests/ci.rs`.

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
