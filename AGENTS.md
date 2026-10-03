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
- **Contains:** a Go service, its committed OpenAPI document, a PostgreSQL
  schema with row-level security in front of it, and the registry as data and
  as seed.
- **Language:** Go, chosen for this service and not for fleet consistency. It
  reads a database, speaks HTTP, and its gate stands a scratch PostgreSQL up on
  every run; see DECISIONS.md D33 for why that settled it and for what it cost.

## The four rules that matter more than the rest

1. **A rule that is not in `core/schemas/` is not a cafaye rule.** Field names,
   patterns and the constraint grammar come from
   `cafaye/core/schemas/cafaye.manifest.schema.json` and
   `cafaye/core/docs/{manifest-conventions,openapi-conventions,event-naming}.md`.
   pantry adds no vocabulary to a manifest. If a fact does not fit in one, it goes
   in `registry/index.yml` and is documented as a registry fact — which is how
   `kind` and `basePath` work.
2. **A copy nobody checks is a copy that rots.** `registry/services/*/cafaye.yml`
   are copies of other repositories' files, kept **verbatim, comments
   included**, and each row in `registry/index.yml` records the commit the copy
   was taken from. `tests/recorded_copy.rs` enforced that byte for byte on every
   run and **was deleted with the Rust service; nothing enforces it now.** The
   rule is unchanged and the enforcement is not, which is the most important
   thing in this file for a new reader to know. A stale comment in one of these
   files is not cosmetic: it is usually a missing `DECISION NEEDED`, and a
   registry copy missing one is a service with open questions that looks settled.
   DECISIONS.md D33 carries the gap and the name the replacement has.
3. **A merge in another repository is not a failure in this one.** Any check that
   reads a sibling checkout must read it at a *recorded ref*, and any distance
   from that ref must be reported by name rather than asserted. This is MD15
   applied twice — to `core` (via `vendir.lock.yml`) and to every registered
   service (via `recordedAt`) — and it is the rule that stops this repository
   being reported as broken because `identity` merged. **Every check that did
   this was Rust and every one of them is gone**, so the rule currently has no
   enforcement; it is kept because the next person to write those checks in Go
   needs it, and a rule with a reason beats a rule with a test.
4. **Tests first.** Per PLAN.md §3. Add the test, run it, watch it fail, then make
   it green by changing the implementation — not by loosening the assertion.

## Order of work

1. Write the test beside the code — `internal/<pkg>/…_test.go` for Go, or
   `tests/rls_checks.sh` for SQL.
2. Run it and **show it failing**. A test that has never failed has never been
   proven to test anything.
3. Make it green by changing the implementation.
4. If the failure is the wrong failure, the test is wrong. Fix the test first.
5. `./bin/prime` before every commit.

There are no sleeps in this suite because there is nothing to wait for. The HTTP
tests drive the router with `net/http/httptest` in-process, and the
database-backed tests bring their own PostgreSQL: they run `tests/rls.sh --serve`,
which does `initdb`, picks a free port, applies `migrations/`, seeds
`tests/seed.sql`, and removes the directory on exit. That is why the gate needs
PostgreSQL's **binaries** and not a server, and why `gate.yml` declares that as
an external requirement rather than assuming one.

## Registering or changing a service

Adding or removing an entry is the change most likely to be wrong, so:

1. `cd ../caf && go run ./cmd/caf contract lint ../<service>/cafaye.yml` — it
   must be `OK`.
2. Copy it to `registry/services/<name>/cafaye.yml`. **Verbatim, byte for byte,
   comments included** — and this is enforced, not merely intended:
   `tests/recorded_copy.rs::every_registered_copy_is_verbatim_at_the_ref_this_registry_records`
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
4. ~~`cargo test --test recorded_copy`.~~ **There is no step 4.** That test
   and `tests/drift.rs` were Rust and were deleted with `src/`; see DECISIONS.md
   D33. Diff the copy against the service by hand until they exist again.
5. If you are *removing* a service, add an `excluded` row with a `blockedBy`, a
   reason and a `verify` command. `tests/schema.rs` used to run that command and
   assert the reason still holds, so a row could not rot into a fiction, and it
   was Rust and it is gone. Run the `verify` command yourself.

Never edit a service's real `cafaye.yml` from this worktree. This repository
reads them; it does not own them.

## Changing the response shape

`GET /v1/services` is a published contract and `caf pantry` will be generated
from it. A change is:

1. `openapi/v1.yaml` — the machine half.
2. `README.md` — "The response shape", the human half.
3. `internal/catalog/catalog.go` (and `postgres.go`) — the code. `internal/api/
   api.gen.go` is generated from (1) by `go generate ./internal/api/`; do not
   hand-edit it, and `bin/prime-go` checks its generator header.
4. A test in `internal/httpapi/httpapi_test.go` that names the field, so the
   three above cannot disagree. The test that lists every documented key is the
   first thing that should fail when a key is added.

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

## The vendored schema, and the ref it came from

`schemas/cafaye.manifest.schema.json` is a byte copy of core's, and
`tests/schema.rs` asserts it **at the commit `vendir.lock.yml` records**:

```sh
cp ../core/schemas/cafaye.manifest.schema.json schemas/
git -C ../core rev-parse HEAD     # into vendir.lock.yml, same commit
```

`/readyz` validates every entry against it at startup, so a stale copy makes the
readiness probe a decoration. When core adds a field, copy the new schema in the
same commit that reads it — a manifest field pantry does not know is a field
pantry silently drops.

**The pin is not bookkeeping — it is what makes the check possible.** The
alternative was to compare the vendored copy against `core/schemas/…` in the
working tree, which is what this repository used to do, and which meant a commit
in *core's* repository decided whether *this* repository's gate passed. It did,
three times. `core-09` added two valid gate declarations to
`core/examples/valid/`, and a test that validated every `*.yml` in that directory
against the **manifest** schema went red here with `"owner" is a required
property` — a manifest rule quoting a document that was never a manifest. Read at
the recorded ref instead, the check is "this copy is what core published at R",
which is true or false for a reason in this repository.

**A stale pin is legal.** The gate stays green; `this_repository_says_how_far_
behind_core_it_is` prints the distance, and a pin more than 9 commits behind fails
with the one-command fix. Bumping the pin is a decision, and the report is how you
notice a decision is due. See `DECISIONS.md` D4.

## The registry is a copy, and the copy records where it came from

`registry/services/<name>/cafaye.yml` is a copy of another repository's file. It
has to be: pantry has no database, no plugin loader, and `Registry::load` runs at
startup inside a container with no sibling checkouts and no network, so serving the
registry requires the bytes. What changed is *what the copy is a copy of*: every
row in `registry/index.yml` carries a `recordedAt` — the commit of that service's
own repository the copy was taken from — and
`tests/recorded_copy.rs` verifies the copy against **that commit**, not against the
working tree.

The difference is who a red belongs to. Comparing to the working tree asserts
"this copy is what the service says right now", so `identity-09` and `muse-06`
landing turned pantry's gate red and were reported as *pantry* being broken —
which is how a comment-only staleness and a registry publishing `required: false`
for a dependency muse had made **required** (a muse without identity is 503 on
every request) both arrived as the same story. At a recorded ref, those become: a
copy edited here, or a `recordedAt` bumped without re-copying. Both are defects in
this repository.

**So, when you copy a manifest, bump `recordedAt` in the same commit.** Every gate
run prints how far behind each copy is; a copy more than 9 commits behind fails
with the `cp` that fixes it. See `DECISIONS.md` D4.

## Two document kinds in core's examples

`core/examples/valid/` holds **service manifests** (`*.cafaye.yml`, governed by
`cafaye.manifest.schema.json`) and **gate declarations** (`gate.*.yml`, governed
by `gate.schema.json`). A consumer that validates every `*.yml` there against the
manifest schema breaks the moment core adds a second kind — which it did.

`tests/core_pin.rs` keeps a table naming every non-manifest example **and the
schema that governs it**, and asserts the classification is *total* in both
directions: a file nobody classified fails with its name, and a table row whose
file no longer exists fails too. pantry deliberately does **not** vendor
`gate.schema.json` and does not validate gate declarations; the recommendation to
core is to split the directory by kind, and the reasoning is `DECISIONS.md` D3.

## Out of scope, deliberately

Phase 1 is official-only. These are Phase 5 (PLAN.md §4b item 3) and building
them now is how a registry becomes an unaudited code-execution surface:

- **No marketplace, no third-party submissions, no registration webhook.** No
  untrusted manifest is ever accepted at runtime. Adding a service is a commit
  and a reviewed pull request.
- **No plugin loader.** pantry reads rows from a database whose policies are
  fixed by `migrations/`, and YAML from a directory this repository owns.
- **No UI.**
- **No cross-service database access.** pantry owns its own tables and reads
  nothing else. The database is not optional any more — it is the read model —
  but it is pantry's alone, and `tests/rls.sh` holds the four roles' grants to
  84 checks.
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
flipping — and report the open list. One number per decision, ever. A decision
recorded in the affected file **and** in `DECISIONS.md` (this repository's own
open list, `D1`, `D2`, …) is recorded once in two places, not twice: the second
reader is meant to find the same argument, not a second argument.

Four are open. Two are the HTTP contract's, and both are recorded in
`openapi/v1.yaml` and the README:

1. **`method_not_allowed` (405)** is not in core's reserved code list. Recorded
   here for a core amendment rather than invented quietly. The cost of flipping:
   one slug in `internal/httpapi/problem.go` and one in the document.
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

**It is now load-bearing a second time, and that is not what it was ratified
for.** `cafaye-ts` (pantry-06) is registered as a `cli` too, and it is not a
binary: no `bin`, no entry point, imported rather than run. So the value covers
both "a binary" and "a package other programs import", and the registry now
answers the same question two ways for the two client libraries in the fleet —
`cafaye-ts` is a `cli`, the `cafaye-rb` gem is a `blockedBy: library`. **That is
`DECISIONS.md` D1, and it is open.** Read it before touching either row: the
tempting repair in either direction moves a row another packet wrote.

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
`an_unreadable_repository_is_neither_cloned_nor_registered` in `tests/ci.rs`. It
has company — `site` is private too — and the two entries now carry **one**
claim between them, which is what pantry-23 fixed and what D2 needed.

The third gap, `cafaye-py`, is the one that closed, and it closed the other way:
its row was `no-manifest` on a directory that was not a repository, so CI
satisfied "the manifest is absent" with a directory that did not exist — the
vacuous pass `DECISIONS.md` D2 was about. That repository has since been
written, it is public, and the row is now checked on every CI run. D2 is
**RULED** on 2026-10-02, by the repository landing rather than by a decision.

**A manifest is the record of its own visibility, and a rename is not a
publication.** `site` is `parlor` renamed, `parlor` is public, and `site` is
private — so "a fork of a public template is public" is a reasonable inference
and a wrong one. pantry-23 made it, put `site` in `CAFAYE_REPOS`, and would have
taken the drift job's clone step down under `set -euo pipefail`. The check that
would have caught it is one `git ls-remote`, and the record that answers it is
the manifest's own `repository:` block.

The lesson D2's ruling leaves behind is the one worth carrying, and it is a
general shape rather than a coincidence: **prefer the `blockedBy` value whose
meaning does not depend on the thing being temporary.** `no-manifest` means "no
`cafaye.yml` on master", which was true before the repository existed and is
true after it. The fifth value D2 considered — `planned`, meaning "no repository
exists" — would have been true once, false forever, and would have cost a
variant in the registry loader, an arm in `every_exclusion_reason_is_still_true`,
a row in the table, a README section and a published meaning in `openapi/v1.yaml`,
none of which could ever be deleted.

**D29 — the fixture pin's schema clause checks validity, not byte-identity.**
A pantry decision rather than an MD one (its reasoning is `DECISIONS.md` D29,
and this is the cross-reference its header asks for). `tests/core_pin.rs` used to
require core's manifest schema at the pin `39acaed` to be byte-identical to the
one at core HEAD, so that a pass at the old ref could not be passing for the
wrong reason — **and it was Rust, and it is gone**, which makes it the second
place this packet leaves the vendored pin unverified (the first is
`vendir.lock.yml`'s own header). **That ref does not exist and never will:** `94f8d25` (the gate
declarations) is an *ancestor* of `ec28365` (the schema), so every ref carrying
HEAD's schema already carries the `gate.*` files, and the two sets are disjoint.
Rewriting core's history to manufacture one is not available.

The clause now asks the question its own comment was asking — do the pinned
examples still **validate** against core HEAD's schema — instead of asking
whether two blobs are equal. Byte-identity was the *strictly stronger* bare
proposition and this does not pretend otherwise; what it was stronger *about*
was a proxy, and a proxy that fired on `ec28365`'s three rewritten description
strings had already trained its readers to skip it. **A clause that can never be
true is not a strict check, it is a deleted check wearing a disguise.** Two
tests hold the new one down, one of which mutates a throwaway copy of the
schema and requires the clause to go red naming the finding. If you are about to
re-pin or relax that clause, read D29 first — and note that the re-pin is not
available.

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
