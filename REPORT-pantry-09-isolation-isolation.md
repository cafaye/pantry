# REPORT — pantry-09-isolation (tenant isolation, negative tests first)

Worktree `pantry-worker-pantry-09-isolation`, branch
`worker/pantry-09-isolation`, four commits, **not pushed**.

## The headline: the packet's premise does not hold in this repository

D18 measured cross-tenant negative tests at identity 7, courier 19, **pantry 0**,
and the packet asked for a negative test per **account-scoped entry point**
following `darkroom-09`. I set out to do exactly that and found the question is
not expressible here. That is the finding, and it is a fact about the
repository rather than a gap in the work:

| the packet assumed                     | what pantry actually is                                    |
| -------------------------------------- | ---------------------------------------------------------- |
| FastAPI routes                         | 4 axum routes, all `get`, plus 2 fallbacks                  |
| service functions                      | 58 `pub fn` in `src/`; 8 of them reachable from a request   |
| DB queries touching tenant data        | **no database.** YAML read once at startup                  |
| account-scoped entry points            | **none.** no accounts exist to scope                        |
| `MUSE_CORE_SCHEMAS`-gated DB tier      | **no such variable.** the env-gated tier here is `PANTRY_CAFAYE_ROOT` |

The evidence, so a reader does not have to take the table's word:

* `grep -riE "account|tenant|owner|org_id|user_id|customer" src/` returns
  **one** hit — `Manifest::owner`, the accountable *team* on a `cafaye.yml`.
  `src/manifest.rs` says of it: "the accountable team, not the author. Renaming
  one is a…". Core's manifest schema defines it and no query filters on it.
  `owner` is a GitHub org in `repository: owner/name` and nothing more.
* **No database.** `src/registry.rs` module docs, "There is no database": *"Phase
  1 has none, and this is a property rather than an omission."* `AGENTS.md` lists
  it under "Out of scope, deliberately — No database, and no cross-service
  database access." `Cargo.toml` carries no driver.
* **No write surface.** `ROUTES` is four `get`s. No `post`, `put`, `patch` or
  `delete` in `src/`.
* **No auth.** No `authorization`, `cookie`, `jwt`, `api_key` or session in
  `src/`; the only `env::var` is `PANTRY_BIND`.

So **D18's `pantry 0` is a correct measurement of a property, not a coverage
gap.** A suite written to the packet's shape would have asserted the existence
of an `account_id` that does not exist. The useful deliverable is to make the
property *checkable* rather than asserted, which is what I did.

## The enumeration

Two files, deliberately split the way `darkroom-09` split its two — one answers
"is the property still written down" (a property of the source, no database) and
the other "does it hold" (a property of the router, no database either, because
there is none to need).

### `tests/scoping.rs` — 14 tests, the source half

Reads `src/` with `include_str!`, so a red is a fact about the source that built
the binary, not a grep somebody has to remember to run. Four of the seven counts
are **derived from the code** rather than read off a list, so both directions
hold: a new `post` fails without anyone editing a list, and deleting a check
fails too.

| set                                                          | count |
| ------------------------------------------------------------ | ----- |
| `pub fn` / `pub async fn` in `src/`                          | **58**    |
| HTTP entry points a request can reach (4 routes + 2 fallbacks) | **6** |
| registry data entry points (`registry.rs` + `filter.rs`)     | **23**    |
| of those, ones a request can change                          | **8**     |
| routes registered with a verb other than `get`               | **0**     |
| write primitives in `src/`                                   | **0**     |
| 403 / `forbidden` / `unauthorized` in `src/`                 | **0**     |

58 = 17 registry + 7 manifest + 7 pin + 10 contract + 5 http + 6 filter + 5
problem + 1 view. The 8 request-reachable data functions are `get`, `query`,
`page` (registry) and `Filter::from_query`, `Filter::matches`, `Page::new`,
`Page::from_query`, `Page::offset` (filter). `load` is deliberately *not* among
them: the directory it reads is chosen at startup, never from a request.

### `tests/entry_point_isolation.rs` — 11 tests, the behavioural half

Drives the router in-process with `tower::ServiceExt::oneshot`. No socket, no
port, no bind, no sleep, no retry.

| op     | entry points | negative cases                     |
| ------ | ------------ | ---------------------------------- |
| read   | 1            | **45** names                       |
| list   | 1            | **19** parameters + 6 empty-match  |
| update | **0**        | 16 verb/path pairs, all 405        |
| delete | **0**        | the same 16                        |
| probe  | 2            | 6 each, scope-shaped query         |
| 404    | 1 fallback   | 8 paths                            |
| 405    | 1 fallback   | 16 + 2 (registered vs not)         |

**update and delete are at zero, and that is asserted rather than assumed.**
`every_write_verb_on_every_registered_path_is_405_and_never_a_success` sweeps
POST / PUT / PATCH / DELETE against all four registered paths — 16 requests —
and requires no 2xx on any of them.

## Absence, never 403

Every negative asserts two things together:

1. the status is not 401 or 403 — **and not 405 either**, because a 405 that
   differs between a registered name and an unregistered one is the same oracle
   in a different status;
2. the response **shape** is byte-identical to the baseline for a name that has
   never existed.

"Shape" is `status`, `type`, `title`, `code` and content type, with `detail` and
`instance` masked. Both masked fields quote the caller's **own** input back —
`instance` is the path asked for, `detail` is the name asked for — so neither can
tell a caller anything it did not already know. A test demanding byte-identical
bodies *including* `detail` would be demanding that pantry stop telling a caller
what it asked for, which is a worse service. The masking is argued in the file
header rather than left as a convenience.

**No 403 was found, so nothing in `src/` changed.** That is the honest outcome
of the packet's "a found 403 is a FINDING; changing it is in scope": there was
none to find, and the 0 rows above are what stops one arriving.

### The negative set is the part that matters

**The six held-back repositories.** Read from `registry/index.yml` at runtime
rather than written into the test, so a new exclusion row is a new probe:
`core`, `docs`, `cafaye-rb`, `cafaye-py`, `parlor`, `kit` — four of which carry
a valid `cafaye.yml`. A registry answering for one would be publishing a service
the platform cannot start. This is the closest thing to "account B's data" that
exists in this repository.

**Eleven scope-shaped query parameters.** `?account_id=`, `?accountId=`,
`?tenant=`, `?tenant_id=`, `?org_id=`, `?organization=`, `?owner=`, `?user_id=`,
`?customer_id=`, `?scope=`, `?visible_to=`. All 400, all with a shape
byte-identical to `?colour=blue`. A caller cannot tell a parameter pantry
recognises-but-refuses from one it has never heard of — the 403 oracle wearing
a 400.

**Eight scope-shaped headers.** `x-account-id`, `x-tenant-id`, `x-org-id`,
`x-owner`, `x-user-id`, `authorization`, `cookie`, `x-forwarded-user`, checked
against both the list and the read path. Header *values* are never printed on
failure, so a red names the header without echoing what it carried.

**Ten path shapes and nineteen near misses.** `..`, `%2e%2e%2fetc%2fpasswd`,
`Courier`, `courier%20`, `courier%00.yaml`, `courier%3Fkind=api`. These are what
a `Path::join` on `{name}` or a normalisation in `Registry::get` would turn into
a hit.

**Nine scope-shaped names.** Added by mutation 4, below. `account-1`, `tenant`,
`org-1`, `user-1` and five more.

## Two URI facts found while writing these

Both are easy to get wrong and both changed the test:

1. **A space, a newline and a null byte are not legal URI characters.**
   `Request::builder` refuses to build the request, so those cases never reach a
   handler — a fact about `http`'s parser, not about pantry. The percent-encoded
   spellings are what a client actually sends, and they are the ones that arrive
   *decoded* at `Registry::get`, so they are the ones worth testing.
2. **`kind=api&language=go` is not an empty match.** `identity` is `go`, so it
   returns one entry. The empty-list test now uses combinations constructible
   from the published vocabulary (`kind=worker&language=go`,
   `kind=cli&language=ruby`, `contract=^99.0.0`), which is the better test
   anyway: a caller can build one of those from the docs.

## The gate was caught failing, not asserted

`AGENTS.md` rule 4 — "a test that has never failed has never been proven to test
anything" — and four mutations run against the two new files. **One of them
found a hole in my own test.**

| # | mutation                                                    | caught by |
| - | ----------------------------------------------------------- | --------- |
| 1 | a `delete` route on `/v1/services/{name}`                    | 2 source tests + 2 behavioural (reporting `it is 200`) |
| 2 | `read_to_string(registry_dir().join(&name))` in `get_service` | `a_service_name_reaches_a_string_comparison_and_never_a_path` |
| 3 | `to_lowercase()` in `Registry::get`                          | both halves; `Courier` becoming a hit is caught |
| 4 | **`Problem::forbidden` + `if name.contains("tenant")`**       | **`tests/scoping.rs` twice, and the behavioural file *not at all*** |

**Mutation 4 is the finding, and it is why there is a separate commit for it.**

The mutation is the plausible three-line version of "add a tenant boundary": a
new `Problem::forbidden` constructor, and a `contains("tenant")` guard in the
handler. `tests/scoping.rs` caught it — the constructor is named in
`no_status_reveals_that_a_resource_exists_but_is_not_yours`, and the public
function count moved from 58 to 59.

**The behavioural file caught nothing.** All 36 of its read negatives were near
misses on *real service names* and *path shapes*, and not one contained the
substring the guard looked for. The test passed against a live 403 oracle,
because the oracle was branches away from everything it asked. A suite that
holds a property and does not fail when the property is broken is not holding it.

Nine scope-shaped names were added (36 → 45) and the same mutation is now red at
the 403. The lesson is in the file header next to them, because it generalises
past this packet: **a negative set has to contain the shapes of the *mistake*,
not only the shapes of the *attack*.** A near miss tests the comparison; a
scope-shaped name tests the refusal. I had one and not the other, and the gate
was green on a suite that could not see the thing it was written for.

All four mutations reverted. `git diff src/` is empty.

## Two things recorded as deliberate, not treated as defects

A green run should not imply these were checked and found fine, so both have a
test that says so on purpose:

* **`the_registry_is_public_and_serving_two_hundred_is_deliberate`.** pantry
  *does* have one existence oracle: a registered name answers 200 and an
  unregistered one 404. That is not a 403 — it reveals nothing about a caller,
  because the registry is public curated data (official cafaye services, so
  `caf dev`, `caf deploy` and a developer reading the platform all get one
  answer), and every caller is served all of it. The test asserts the *list* is
  the whole registry, so a partial list would fail as a scope. If the registry
  ever stops being public, this is the line that has to change, and it should
  change deliberately.
* **`a_write_verb_answers_the_same_whether_or_not_the_name_is_registered`.** A
  405 that differed by name would be the same oracle as a 403, reached with
  `DELETE`. Both names get the same 405, for the same reason — the verb.

## Two tripwires for the next packet

Neither is about a defect today. Both are there so a tenant boundary cannot
arrive unasked:

* `nothing_reads_an_authorization_header_or_a_credential` — no
  `authorization` / `Bearer` / `cookie` / `jwt` / `api_key` / `session_id` in
  `src/`.
* `the_only_environment_variables_the_service_reads_are_the_two_named` —
  exactly one `env::var` in `src/`, and it is `PANTRY_BIND`, a listen address.

A credential read or a scope read at request time is how the first half of a
tenant boundary arrives. If a decision ever needs one, AGENTS.md's rule is the
route: a curated fact in `registry/index.yml` and a `DECISIONS.md` entry first.

## Gate

`./bin/prime`: `cargo fmt --check` clean, `cargo build` clean, `cargo clippy
--all-targets -D warnings` clean, `cargo test --no-fail-fast` **136 passed, 3
failed, 0 ignored**.

**Pass and skip, separately, and the 3 failures are pre-existing and not mine.**
Master at `92ca41e` on the same machine: **111 passed, 3 failed** — the same
three test names, byte for byte:

| failing test                                          | target           |
| ----------------------------------------------------- | ---------------- |
| `this_repository_says_how_far_behind_core_it_is`        | `schema`         |
| `every_example_in_core_s_valid_examples_is_classified_by_this_table` | `core_pin` |
| `the_registry_says_how_far_behind_each_copy_is_and_names_the_fix` | `recorded_copy` |

All three are the recorded-ref distance reports: `core` is **24 commits** past
the `vendir.lock.yml` pin, over the documented 9-commit budget, and a registry
copy is behind its `recordedAt`. They are the checks working — AGENTS.md is
explicit that a stale pin is legal and the gate is *meant* to go red past the
budget — and fixing them is a decision about another repository's commits, which
is not this packet's to make. `core-11` and `DECISIONS.md` D4 own it. My diff
is `tests/` and `CHANGELOG.md`; `src/`, `registry/`, `schemas/` and
`vendir.lock.yml` are untouched.

**111 → 136 is this packet: 14 + 11 = 25 new tests, no test removed, none
weakened, no `#[ignore]`d, no sleep, no raised retry, no loosened assertion.**

**The env-gated tier, named.** `PANTRY_CAFAYE_ROOT` gates the workspace-walking
tier — `drift`, `recorded_copy`, `core_pin` and the schema pin all read sibling
checkouts. Unset, they find no workspace and report the distance instead. Set to
`/Users/kaka/Code/any/moon/cafaye`, `drift` runs its 12 tests for real (all
pass). **There is no `MUSE_CORE_SCHEMAS` variable in this repository and no
Postgres tier** — the packet's expectation of one belongs to a service with a
database, and pantry has none. **0 tests skipped**: nothing in the suite is
`#[ignore]`d, and the workspace-gated tests report rather than skip.

**No secret was logged.** The tests print no token, key or JWT; the one
assertion that sends scope-shaped headers prints the header *name* and never its
value, so a failure reports which header changed the answer without echoing what
it carried.

## Commits

| commit | what |
| ------ | ---- |
| `dcffde5` | the enumeration — `tests/scoping.rs`, 14 tests, seven counts |
| `b040157` | a negative case per entry point — `tests/entry_point_isolation.rs`, 11 tests |
| `82907d9` | nine scope-shaped names, because mutation 4 needed them |
| `81730e7` | CHANGELOG, at the top of Unreleased |

Committed inside the first ten minutes as the addendum asked, then after each
test group.

## What I would do next, and what I did not do

* **I did not add a tenant dimension.** Pantry has no accounts and inventing one
  is Phase 5 (`AGENTS.md`: a registry whose entries can be written at runtime is
  an unaudited code-execution surface). The tripwires above make the first
  arrival of a scope a red rather than a review question.
* **I did not change `src/`.** No 403 to remove, and the two existence behaviours
  above are correct. Changing behaviour to satisfy a premise this repository does
  not have would be the wrong kind of green.
* **Worth a decision, and I am not making it:** the packet asked for
  *account-scoped* isolation, and the fleet's services that do have accounts are
  `identity` and `courier`, which D18 already measured at 7 and 19. If the intent
  was to close a gap in *those*, pantry was the wrong worktree — the count it
  reports (0) is the correct number for a service with no accounts, and the
  packet's own framing should be the thing that changes.
* **The pin is 24 commits behind and over budget.** Not mine to fix, but it is red
  on every run and will be red at merge, and `core-11`'s one-command fix
  (`cp ../core/schemas/cafaye.manifest.schema.json schemas/` plus the new sha in
  `vendir.lock.yml`) is a decision the manager may want to take with this merge
  rather than after it.
