# REPORT — pantry-06

**Register `cafaye-ts`, and settle the empty `cafaye-py/` directory.**

Branch `worker/pantry-06`, five commits on top of `123afca` (pantry-05). Nothing
outside this worktree was modified; `cafaye-ts`, `identity`, `caf`, `cafaye-rb`
and `docs` are all clean in `git status` and `cafaye-py/` is still empty and
still there. Nothing was pushed, no remote was created, no visibility was
changed.

---

## 1. The `kind`, and the argument for it

**`kind: cli`, `basePath: null`, registered.**

The manifest decides more than it looks like it decides, and then stops. cafaye-ts
declares no `exposes` and no `consumes` — its own file says that absence is the
substantive statement in the whole document, and that it is there because
"cafaye-ts does not serve traffic, does not receive it, and does not publish
events. It is a package other things install" — so `serves_http`,
`publishes_events` and `subscribes` are all false, and **no row of the `kind`
table in `src/registry.rs` reaches a manifest in that state**. `check_kind`
refuses `worker` and `both` for a surface-less manifest, which is not a policy
but a consequence of the two earlier arms. That leaves the curated set, which is
closed at two values, and the other one is guard's: `api`, meaning *a service that
serves HTTP and has not written its OpenAPI document yet*. Putting `api` on
cafaye-ts would say a service serves an API when it serves nothing, and
`kind: api` is not a label a client files — it is a routing instruction. So
`api` is a falsehood and `cli` is not, and of the values the schema permits that
is the whole of the argument. `basePath: null` follows from the same place: a base
path is derived from a document a service *publishes*, and this package publishes
none. The six documents in `specs/` are somebody else's publications, copied at
recorded commits — reading a prefix out of one of them would be core's
longest-common-prefix guess, on six different documents, for a repository that
routes nothing.

**And `cli` is an overstatement, which is the part that needs your ruling.** MD1
added `cli` for a binary: *installed **and run***. `cafaye-ts` has no `bin` and
no entry point — `package.json` has no `bin` field at all, and `"sideEffects":
false` with an `exports` map is the shape of something imported. So it is
installed and it is *imported*, and the vocabulary has no word for the second
thing. The bill for that lands immediately and visibly, one screen below the new
row:

| repository | shape | recorded as |
| --- | --- | --- |
| `caf` | a binary, no surface | registered, `kind: cli` |
| `cafaye-ts` | an imported package, no surface, vendors six documents | registered, `kind: cli` |
| `cafaye-rb` | a gem, no surface | **excluded**, `blockedBy: library` |
| `docs` | a static site, no surface | **excluded**, `blockedBy: library` |

Two client libraries, two answers, in one file. And the answer a caller gets now
depends on the language of the service they happen to be writing: *what do I
install?* is answerable for a TypeScript service (`?kind=cli` returns
`[caf, cafaye-ts]`, `?language=typescript` returns `[cafaye-ts, guard]`) and
unanswerable for a Ruby one. That is arbitrary, in a column clients are
documented to switch on. I did not resolve it, because every resolution moves a
row a different packet wrote — including the one your brief ruled out. It is
`DECISIONS.md` **D1**, in four places (`registry/index.yml` on the row, README
"kind", `AGENTS.md` under MD1, and the changelog), with three options and a
recommendation: **(a)** read `cli` as "an artifact a person installs, run or
imported" and move `cafaye-rb` into `services/` as a second `cli`, leaving `docs`
a `library` because a documentation site is installed by nothing — one row, one
directory, `kind` means one thing again, and the registry answers the question
its clients ask; **(b)** keep `cli` narrow (an executable entry point) and put
cafaye-ts in the exclusion record beside `cafaye-rb` — one answer per shape, at
the cost of a published client being invisible; **(c)** a core amendment
distinguishing a library from a binary, so the value is derived rather than
curated, which is the gap caf's row already names with a fourth repository now
asking. **Recommended: (a), then (c).**

Nothing in the exclusion record was changed.

## 2. `cafaye-py/`

**Left in place, with a row: `blockedBy: no-manifest`, a reason that says what the
directory is for, and a workspace walk that can now see it.**

`moon/cafaye/cafaye-py/` is empty, has no git repository and no files. The
registry's job is to have an opinion about every member of the fleet, and a
directory a reader can see with no recorded opinion is the exact shape that
produces a wrong answer later. The row says what the directory is for: the
planned **hand-written** Python client, MD6 having ruled Python hand-written
rather than generated (hey-api's Python generator is v0.0.24 and emits
parameterless methods with unsubstituted path templates; openapi-generator
inverts `const` discriminants to `any`).

**Why not remove it**, since you asked for a recommendation rather than a guess:

1. **A deletion would be unreviewable.** An empty directory is not tracked by
   git, so the commit would consist entirely of a CHANGELOG sentence about
   something git cannot show a reviewer. A registry change is easier to review as
   three commits than one; a change whose evidence is invisible is not a
   reviewable change.
2. It would remove MD6's Python decision from the filesystem, and `AGENTS.md`
   says plainly: *do not touch anything outside this worktree*.

**What the row buys: a tripwire.** `every_exclusion_reason_is_still_true`
asserts the manifest is **absent**, so the day anyone writes
`cafaye-py/cafaye.yml` the suite goes red with *"now carries a cafaye.yml —
register it or change this row's blockedBy and say why it is still held back"*.
A directory tolerated by doing nothing is tolerated forever; this one has to be
answered.

**And the walk now sees it.**
`every_directory_in_the_workspace_is_a_repository_the_registry_curates` (new)
asks about every non-hidden, non-worktree directory rather than only the ones
carrying a `cafaye.yml`. The existing walk asked the right question of the wrong
set: a cafaye repository that lost its manifest in a merge, and a directory
created for a repository nobody has written, are both invisible to it.
`cafaye-py` had been there through four packets. Three exclusions, none of them
an exemption — a hidden directory is not a repository (`.git`, the workspace's
own `.github`), a worktree is not a repository (named `<service>-worker-<packet>`,
and a manifest inside one is that service's manifest, already checked through its
own checkout), anything else is a repository the registry should have an opinion
about or a stray worth finding — and **no list of tolerated names**.

`no-manifest` is a slight overstatement: the value means "the repository carries
no `cafaye.yml` on master yet", and there is no repository yet. That is
`DECISIONS.md` **D2**, with a fifth value (`planned`) recorded as the alternative
and a recommendation to wait for a second such directory before spending a
precedent on one — the Go and Rust clients are also unstarted, so the count is
the argument, not this row.

## 3. A finding that was not in the brief

**The baseline was red for two reasons, not one.** The brief says it fails "only
on the drift target" — true, all three failures were in `tests/drift.rs` — but two
of them were not about `cafaye-ts`:

```
---- every_registered_entry_matches_the_real_service_on_disk stdout ----
assertion `left == right` failed: identity does not match …/identity/cafaye.yml
  left:  description: "…identity, scoped API tokens, and the OIDC provider."
         events: [… identity.api_key.created, identity.api_key.revoked]
  right: description: "…identity, scoped API tokens, and the OIDC provider."
         events: [… identity.api_key.created, identity.api_key.revoked]
```

`identity-08` has landed. identity's own manifest publishes two more events and
a new description, and this repository's copy of it had neither. The tripwire
caught it by itself, exactly as designed. Fixed with the `cp` the failure message
prints, in **its own commit** so that the two findings stay apart for review —
this one is drift, the other is a repository the registry never knew about.

Also worth a glance, and not acted on because it is not this repository's:
`cafaye-ts/specs/index.json` records six vendored documents
(`billing, courier, darkroom, identity, muse, pantry`) and that is correct —
guard has no OpenAPI document, which is why six and not seven. Nothing to report.

## 4. The gate

**Baseline, before any change** — `./bin/prime` → `EXIT=101`, **89 tests, 86
passed, 3 failed**:

| target | result |
| --- | --- |
| `tests/drift.rs` | **FAILED. 8 passed; 3 failed** |
| `tests/api.rs` | ok. 23 passed |
| `tests/filters.rs` | ok. 13 passed |
| `tests/kind.rs` | ok. 7 passed |
| `tests/manifest.rs` | ok. 13 passed |
| `tests/schema.rs` | ok. 7 passed |
| `tests/contract.rs` | ok. 9 passed |
| `tests/ci.rs` | ok. 6 passed |
| lib + main + doc | 0 passed |

**After, `./bin/prime` → `EXIT=0`, 93 passed, 0 failed:**

```
==> cargo fmt --check
==> cargo build
==> cargo clippy -D warnings
==> cargo test (no network, no database)
     Running tests/api.rs        test result: ok. 24 passed; 0 failed
     Running tests/ci.rs         test result: ok.  6 passed; 0 failed
     Running tests/contract.rs   test result: ok.  9 passed; 0 failed
     Running tests/drift.rs      test result: ok. 14 passed; 0 failed
     Running tests/filters.rs    test result: ok. 13 passed; 0 failed
     Running tests/kind.rs       test result: ok.  7 passed; 0 failed
     Running tests/manifest.rs   test result: ok. 13 passed; 0 failed
     Running tests/schema.rs     test result: ok.  7 passed; 0 failed
==> the manifest validates against core's schema
  OK …/registry/services/cafaye-ts/cafaye.yml      (and the other eight)
==> ok
```

`drift.rs` 11 → 14 and `api.rs` 23 → 24, so 89 tests → 93. **No test was deleted,
no assertion loosened, no threshold raised, no sleep and no retry anywhere in
this packet.** `info.version` stays at **1.1.0**: no field added, removed or
renamed, no enum value added — only prose in `openapi/v1.yaml` and the README.

## 5. Tests first, and what they said before the registry changed

Every assertion was written and run against the un-registered repository first.
**10 reds across three targets**, then the registration. Verbatim:

```
---- the_registry_is_the_whole_official_set_sorted_by_name stdout ----
assertion `left == right` failed: every official service, including pantry itself…
  left: ["billing", "caf", "courier", "darkroom", "guard", "identity", "muse", "pantry"]
 right: ["billing", "caf", "cafaye-ts", "courier", "darkroom", "guard", "identity", "muse", "pantry"]

---- a_package_that_vendors_the_fleets_documents_serves_none_of_them stdout ----
assertion `left == right` failed: 404 {"code":"not_found","detail":"no official cafaye service
  is named \"cafaye-ts\"","instance":"/v1/services/cafaye-ts","status":404,…}
  left: 404
 right: 200

---- every_service_repository_in_the_workspace_is_registered_or_excluded stdout ----
cafaye-ts exists in the workspace but is neither registered in registry/services/ nor recorded in
registry/index.yml's exclusion list. Add it to one or the other — a registry that silently omits a
service is a registry that cannot be audited.
```

and the same for `the_two_curated_kinds_are_distinguishable_from_outside`,
`readyz_reports_a_loaded_registry_and_refuses_an_unloaded_one`,
`no_filter_returns_every_official_service_sorted_by_name`,
`kind_filter_accepts_every_kind_in_the_vocabulary`,
`language_filter_covers_every_language…`,
`contract_filter_matches_by_range_intersection`, `every_filter_narrows_the_list`,
`a_filter_that_matches_nothing_is_an_empty_list`,
`two_filters_are_both_applied`, and both paging tests.

Then the `cafaye-py` half, one red, verbatim:

```
---- every_directory_in_the_workspace_is_a_repository_the_registry_curates stdout ----
1 directory is in the workspace that registry/index.yml curates in neither direction:

  cafaye-py — /Users/kaka/Code/any/moon/cafaye/cafaye-py  carries NO cafaye.yml — so it is a
  directory, not yet a repository
```

**Two counts that had to move, and were not left to chance:**
`?language=typescript&contract=^0.2.0` stopped being an empty question when
cafaye-ts registered, so the empty-list case became
`?language=python&contract=^0.1.0` (muse is the only python service and it is on
`^0.2.0`). That case has now been invalidated by a registration **twice** — it was
`?language=elixir` until courier registered and `?language=go` until caf did — so
the third replacement was checked rather than assumed. And `/readyz`'s count went
8 → 9, which is now a literal a reviewer can read rather than a number nobody
notices moving.

## 6. The tripwire still bites — three reds, three restores, three greens

### Proof 1 — rename the service in the index

`registry/index.yml`: `cafaye-ts:` → `cafaye-ts-client:`.

```
test result: FAILED.  3 passed; 21 failed   (tests/api.rs)
test result: FAILED.  5 passed;  1 failed   (tests/ci.rs)
test result: FAILED.  2 passed; 11 failed   (tests/drift.rs)
test result: FAILED.  5 passed;  8 failed   (tests/filters.rs)

---- every_registered_entry_matches_the_real_service_on_disk stdout ----
thread '…' panicked at tests/drift.rs:94:52:
the official registry loads: Invariant("…/registry/services/cafaye-ts/cafaye.yml is not in
registry/index.yml. A manifest is only a registry entry once the index says which kind it is and
what its base path is — copy it into services/ and add the row in the same commit.")
```

41 panics from one rename. Restored: **92 passed, 0 failed** (the count is as
of that commit; the fixture test added afterwards is the 93rd).

### Proof 2 — one byte in the vendored copy

A single trailing space on line 16 of `registry/services/cafaye-ts/cafaye.yml`,
which is a comment. Nothing else changed.

```
test result: FAILED. 12 passed; 1 failed   (tests/drift.rs)

---- every_registered_entry_is_a_verbatim_copy_of_the_services_own_bytes stdout ----
1 registry copy is not byte-identical to the service they were copied from.

  cafaye-ts
    copy   registry/services/cafaye-ts/cafaye.yml (5342 bytes)
    real    cafaye-ts/cafaye.yml (5341 bytes) — first differs at line 16
    every YAML field already matches — a COMMENT-ONLY drift. The copy is out of date, not wrong:
    it is missing the decisions the service has recorded since it was taken.
    fix     cp …/cafaye-ts/cafaye.yml registry/services/cafaye-ts/cafaye.yml
```

One byte, and the test names the line, the byte counts, that no YAML field moved,
and the `cp`. Restored with `git checkout`, and `cmp` against
`../cafaye-ts/cafaye.yml` confirms byte-identity again. **92 passed, 0 failed.**

### Proof 3 — `basePath` at a path no document has

`basePath: null` → `basePath: /v2`.

```
test result: FAILED.  3 passed; 21 failed   (tests/api.rs)
test result: FAILED.  2 passed; 11 failed   (tests/drift.rs)
test result: FAILED.  5 passed;  8 failed   (tests/filters.rs)

---- a_document_at_the_repository_root_is_served_verbatim stdout ----
thread '…' panicked at tests/api.rs:70:59:
the official registry loads: Invariant("cafaye-ts is a `cli`, which publishes no OpenAPI document,
so its basePath must be null: Some(\"/v2\") is a path a binary does not serve")
```

That message is `check_kind`'s own arm — pantry-05's addition, and the reason it
was worth adding: a `cli`'s `basePath` is the one value that cannot be derived
from a document, so the only thing that can hold it is a check. Restored:
**92 passed, 0 failed**.

### A fourth red the packet did not plan

Adding the `cafaye-py` row turned a check this packet had already read green:

```
---- the_drift_job_clones_every_repository_pantry_curates stdout ----
1 cafaye repository is curated in registry/index.yml that .github/workflows/ci.yml neither clones
nor declares unreadable:

  cafaye-py — curated, and read by neither list

A repository the job cannot read is a repository no drift test can compare against anything, and
the tests will not say so: the ones that need it print a SKIP and return green. Add the names to
CAFAYE_REPOS, or — if the runner genuinely has no credential for it, as it does for the private
cafaye-rb — name it in CAFAYE_UNREADABLE with the reason in the comment beside that list.
```

I had written that row asserting `cafaye-py` was *deliberately not* in
`CAFAYE_UNREADABLE`, on the reasoning that the list is a claim about what a runner
cannot **read** and a repository nobody has written cannot be read by anyone. The
check refused the registration, and the fix was not to argue: a curated name the
job can neither clone nor account for is a claim no drift test can check, and
cloning it would fail the job's clone step on a 404. It is declared now, with the
reason beside the list — and the reason is a *different* one from `cafaye-rb`'s:
nothing about `cafaye-py` is unreadable, because there is nothing there to read.
One list now carries two claims, which is the stronger half of D2's case for a
`planned` value. The wrong claim is corrected in all four places it was made
(row, reason field, README, changelog) rather than quietly dropped.

## 7. What I decided that you might want to overrule

1. **`kind: cli` for an imported package** (D1, above). The manager's brief
   expected this; my own reservation is recorded rather than acted on, because the
   alternative moves `cafaye-rb`'s row and the brief forbade the exclusion route.
   **If you rule (b) — exclude cafaye-ts as `library` — the change is small and
   mechanical:** move the directory, delete the row, `?kind=cli` returns `[caf]`
   again, and `?language=typescript` returns `[guard]`.
2. **A directory in the exclusion record.** `no-manifest` on something that is not
   a repository (D2). A fifth `blockedBy: planned` is the honest vocabulary; I did
   not add one, because a vocabulary value for a single row is a precedent and the
   count has not arrived yet. If you would rather have the value now, the arm is
   small: one variant, one match arm in `every_exclusion_reason_is_still_true`, the
   `blockedBy` table in the index, and the README section.
3. **The widened workspace walk.** It will fail on a stray directory in
   `moon/cafaye/` — a scratch checkout, an unpacked tarball, a `dist/`. That is
   the intended behaviour and the failure message says so, but it is a new way for
   the suite to go red on somebody's machine for a reason that has nothing to do
   with the registry. If you would rather it only fire on directories that look
   like cafaye repositories, that is a one-line change to the filter and I would
   not argue.
4. **`DECISIONS.md` is new.** The brief asked for pantry's own `DECISIONS.md` and
   the repository had none. It is D-numbered (`D1`, `D2`) rather than
   MD-numbered, because the fleet's MD numbers are the manager's and MD1/MD2 are
   already spoken for. `AGENTS.md`'s open-decision section points at it, and says
   that a decision in both places is one decision recorded twice, not two.

## 8. Files

| | |
| --- | --- |
| `registry/services/cafaye-ts/cafaye.yml` | byte-identical copy, 5341 bytes |
| `registry/index.yml` | the row, and the `cafaye-py` exclusion; header counts 5 → 6 |
| `DECISIONS.md` | **new** — D1, D2 |
| `tests/drift.rs` | 2 new tests, `cafaye-ts` and `cafaye-py` in `known`, the walker |
| `tests/api.rs` | 1 new test, 4 expectations moved |
| `tests/filters.rs` | 6 expectations moved, one new empty case |
| `.github/workflows/ci.yml` | `CAFAYE_REPOS` + `cafaye-ts`; `CAFAYE_UNREADABLE` + `cafaye-py` |
| `openapi/v1.yaml` | prose only, still 1.1.0 |
| `README.md`, `AGENTS.md`, `CHANGELOG.md` | the `kind` table, the exclusion record, D1, D2 |

Commits, in order:

```
ac55290 test: pin the cli document walker's narrowness, which cafaye-ts depends on
f59e875 feat: give cafaye-py a row, and let the workspace walk see a bare directory
085a4b7 feat: register cafaye-ts — a `cli` that ships six documents and serves none
3053b8f fix: refresh identity's registry copy, which identity-08 made stale
```

## 9. DECISION NEEDED for the manager

1. **D1 — does a client library belong in the registry at all, and if so, is
   `cafaye-ts` a `cli` or should it be a `library` beside `cafaye-rb`?** Recorded
   with three options; recommended **(a)** — read `cli` as "an artifact a person
   installs, run or imported" and promote `cafaye-rb` to a second `cli`.
2. **D2 — is a fifth `blockedBy` (`planned`) worth a precedent for one row?**
   Recommended: not yet; when the Go or Rust client directory appears.
3. **A gap outside this repository, for whoever owns it:** `kind: cli` and
   `blockedBy: library` are both pantry's vocabulary, and the reason this packet
   had to choose between them is that **core's manifest schema cannot say "I am a
   binary" or "I am a library" either**. Three repositories now ask core for that
   word and the fourth is a client. Every fix inside pantry is a curated row and a
   human; the fix in core makes the value derived. That is a core packet.
