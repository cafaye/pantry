# HANDOFF — pantry-go-01

> **RESOLVED. Nothing below is a current instruction.** This file was written
> while the Go service and the Rust service were both in the tree and the gate
> ran two tiers, and it describes that state: the Rust tier red, `/readyz` 503,
> data arriving in pantry-02. All of that has since happened.
>
> What came of it: `cmd/pantry` serves all four operations from PostgreSQL with
> row-level security in front of it; `internal/catalog` is mounted to
> `internal/catalog/postgres.go` rather than being a guess at a seam; CI calls
> `./bin/prime` and nothing else; and **the Rust implementation, `src/`,
> `Cargo.toml`, `Cargo.lock` and `docker/Dockerfile` have all been deleted** —
> see `REPORT-registry-norust-03.md` and `DECISIONS.md` D33.
>
> It is kept rather than deleted because the reasoning below is the reasoning
> those outcomes were made from, and a packet that deletes the reasoning while
> keeping the outcome leaves the next reader unable to tell which part was
> reversible.

## What the packet was for

Rewrite pantry in Go as a real HTTP service with an OpenAPI contract, a
container image and a green gate — the first packet of the rewrite, not all of
it. Get a Go service that boots, serves and passes its own gate. Data comes in
pantry-02. The Rust implementation stays in the tree; a later packet removes it.

## What is done and verified

Two commits on `worker/pantry-go-01`, both after a run:

1. `16e85fd` — the module, the generated contract, `net/http` + `chi`,
   `/healthz` answering.
2. `bba08d2` — the Go tier's gate, the container image, the suite that holds
   both, and the Go-vs-Rust wiring in `bin/prime` and `mise.toml`.
3. the docs commit (README layout + "The Go service", CHANGELOG under
   Unreleased) and this file.

**The Go tier, run and green:**

```
$ ./bin/prime-go
==> toolchain: go 1.26.1 (mise.toml pins 1.26.1)
==> gofmt -l            every file under ./cmd and ./internal is gofmt-clean
==> go build ./...
==> go vet ./...
==> go test ./...       ok github.com/cafaye/pantry/internal/httpapi  0.325s
==> the generated server matches the OpenAPI document
                        internal/api/api.gen.go is oapi-codegen v2.8.0 output, committed
==> ok
exit 0
```

14 Go tests, no sleeps, no network, no database. The media-type assertion was
**shown failing first**: changing `problemContentType` to `application/json`
produced

```
--- FAIL: TestProblemResponsesUseTheProblemMediaType
    GET /readyz Content-Type is "application/json", want application/problem+json
    GET /v1/services Content-Type is "application/json", want application/problem+json
    GET /v1/services/identity Content-Type is "application/json", want application/problem+json
```

**The service, run for real:**

```
$ go run ./cmd/pantry
GET /healthz -> 200 {"status":"ok"}            X-Trace-Id: 210ab4ad14e4d9c409b8416b1c4dcfa6
GET /readyz  -> 503 application/problem+json
  {"code":"unavailable","detail":"no data source is mounted: pantry-01 stands the
   rewrite up with no read path, and pantry-02 mounts the Postgres-backed one",
   "instance":"/readyz","status":503,"title":"Service unavailable",
   "trace_id":"77f4836c8a4aef764ce31c64e782efa6",
   "type":"https://errors.cafaye.com/unavailable"}
```

**The declared gate** is `./bin/prime` (gate.yml: `command: [bin/prime]`). It
runs both tiers now. Its Rust tier is RED on this branch for reasons that have
nothing to do with this packet — see below.

## The currently-failing command, with its real output

`PANTRY_CAFAYE_ROOT=/Users/kaka/Code/any/moon/cafaye ./bin/prime` → **exit 101**,
`error: 4 target(s) failed`:

```
error: 4 targets failed:
    `--test core_pin`
    `--test drift`
    `--test recorded_copy`
    `--test schema`

suite: 142 passed across 13 test binaries
skips: 0 — every tier ran
bin/prime: cargo test exited 101
```

The four, with their own messages:

- `core_pin::every_example_in_core_s_valid_examples_is_classified_by_this_table`
  — `2 file(s) in core/examples/valid/ match neither rule and neither row of
  NON_MANIFEST_EXAMPLES: tenancy.rls-declared-none.yml, tenancy.rls-enforced.yml`
- `drift::every_directory_in_the_workspace_is_a_repository_the_registry_curates`
  — `noul — /Users/kaka/Code/any/moon/cafaye/noul carries NO cafaye.yml — so it is
  a directory, not yet a repository`
- `recorded_copy::the_registry_says_how_far_behind_each_copy_is_and_names_the_fix`
  — `caf is 18 commit(s) behind … identity is 22 … pantry is 14 … parlor is 20`
  (budget 9)
- `schema::this_repository_says_how_far_behind_core_it_is`
  — `58 commit(s) behind core master — over the 9-commit budget`

**These are workspace facts, not this packet's.** Every one of them reads a
sibling checkout or a file this packet did not touch. The full diff of this
branch against `3cb6f08` is:

```
 .gitignore  Dockerfile  bin/prime  bin/prime-go  cmd/pantry/main.go  go.mod
 go.sum  internal/**  mise.toml
```

No change to `registry/`, `schemas/`, `vendir.lock.yml`, `tests/`, `src/`,
`cafaye.yml`, `openapi/`, `gate.yml` or `.github/`. `noul` is a new directory in
`/Users/kaka/Code/any/moon/cafaye` that this worktree did not create, core has
moved 58 commits ahead, and four registry copies have aged past the budget. All
four are fixed by the packets that own them: re-copy `registry/services/*`, bump
`vendir.lock.yml`, and decide what `noul` is (registered, or excluded with a
reason). **Do not "fix" them by loosening the assertions.**

## What is half-done, and why

- **`/readyz` is 503, and both data routes are 503.** No catalog is mounted:
  pantry-01's brief says data comes in pantry-02, and a `200` with
  `"services": 0` would be indistinguishable from a platform with no services —
  which the document names as the exact reason the 503 exists. The 200 path is
  written and tested through a fake catalog
  (`TestReadyzIsTwoHundredWithACatalogMounted`), so it is not untested code
  pretending to work.
- **`internal/catalog.Catalog` is a guess at a seam.** Three methods, in the
  generated types. The filter grammar, cursor encoding and compatibility rules
  are deliberately absent — a schema decision made here would be wrong. Expect
  pantry-02 to change this interface; that should be a two-line commit and the
  HTTP-contract tests should not move.
- **The kit Go template is adopted in part.** distroless rather than kit's
  debian-slim (kit's base is debian because its image migrates at boot; this
  build owns no schema), and no provenance stamp (copying the script without the
  CI that feeds it five build-args would stamp `unknown` and make the labels a
  lie that reads as data). Both are recorded in the Dockerfile's own header.
- **CI.** `bin/prime` is called by the `workspace-drift` job, which has
  `actions/setup-go@v7`, so the Go tier runs there. The `build` job re-spells the
  cargo steps individually and does not call the gate — a difference gate.yml
  already records as a manager's call (REPORT-pantry-08-gate.md). I did not
  change how CI verifies this repository.

## Decisions made, and why

- **`/healthz` 200, `/readyz` 503.** The document states both rules itself
  ("`200` whenever the process is running — including when the registry did not
  load"; "`200` only when the registry loaded"). This build simply has not loaded
  one.
- **The Go service reads the same `openapi/v1.yaml`**, not a new document. The
  rewrite staying compatible with the existing contract is the brief's
  requirement, and two documents is one more thing to drift.
- **Generated into `internal/api` as `package api`, no subpackage.** identity
  uses `client/generated` because its hand-written and generated packages both
  declare `Client`; there is no such collision here and everything is under
  `internal/` anyway.
- **`strict-server` is not generated.** Its interface returns a typed response
  per method, but this service also has to write problem documents on two routes
  the generated code does not own (chi's own 404 and 405). Generating it and not
  using it would be thousands of lines of output a lint exclusion then excuses.
- **The Go Dockerfile is `./Dockerfile`, not `docker/Dockerfile.go`.** Measured:
  `go build ./...` prints `docker/Dockerfile.go:1:1: illegal character U+0023 '#'`
  — kit's filename works in a repository with no `go.mod` and is a parse error in
  a module. It is also identity's arrangement.
- **`go test -race` is deliberately absent.** This build has no shared mutable
  state; a race detector that always finds nothing is cost without signal. It
  arrives with the catalog.
- **No panic, no retry, no sleep, no widened bound** anywhere in the gate.

## The successor's first move

Read `internal/catalog/catalog.go`, then **decide the catalog's shape against the
schema rather than against the interface this packet guessed** — three methods
typed in generated types is a placeholder, and `catalog.Filter` is provisional by
its own comment.

Then, in this order:

1. `cp ../core/schemas/cafaye.manifest.schema.json schemas/` and put
   `git -C ../core rev-parse HEAD` into `vendir.lock.yml` (the 58-commit pin).
2. Re-copy the four stale manifests and bump their `recordedAt` — the test prints
   the `cp` for each.
3. Decide what `../noul` is: a repository the registry registers, or a row in
   `registry/index.yml`'s exclusion record with a reason that is still true.
4. `cd ../caf && go run ./cmd/caf contract lint ../pantry/cafaye.yml`.

Only then mount a catalog, at which point `/readyz` answers 200 and
`TestTheDataRoutesAreFiveOhThreeAndNotAnEmptyRegistry` becomes the test that
should fail.