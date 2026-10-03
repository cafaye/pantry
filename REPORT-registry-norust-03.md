# REPORT — registry-norust-03

**What this packet did.** Deleted the Rust implementation of pantry — `src/`,
`Cargo.toml`, `Cargo.lock`, `docker/Dockerfile` and all fourteen files under
`tests/` — and rewrote the three things that described it: the gate declaration
(`gate.yml`), the CI workflow, and the gate's own proof that its declaration can
fail (`bin/gate-self-test`). Then rewrote the documentation that asserted a Rust
service was still there, because a document that names a deleted file is a
document that lies quietly.

Branch `worker/reg-norust-03`, based on pantry `master` at `b15e0a0`.

---

## 1. Why Go, and what that cost

Go was decided before this packet (`HANDOFF-pantry-go-01.md`), for the service
rather than for the fleet. pantry reads a PostgreSQL database, speaks HTTP and
nothing else, and its gate stands a scratch cluster up on every run. A compiled
binary with `net/http` and `pgx` is the smallest thing that does all three.

The measured result, on this machine:

| | Rust | Go |
|---|---|---|
| warm gate | ~4 min 17 s | **~12.5 s** (4.3 s user) |
| crates / modules compiled | ~110 | 2 packages with tests |
| declared external requirements | 4 | 4 (one of them new: PostgreSQL **binaries**) |

Twenty times faster is not the point on its own. The point is what it bought:
**the full `bin/gate-self-test` became cheap enough to run**, which is what made
the declaration work below verifiable rather than asserted.

## 2. The gate declaration had a proof that could not fail

This is the finding the packet exists for, and it is worth stating precisely
because the shape is general.

The old `gate.yml` carried:

```yaml
    - id: skip-count
      match: '^skips: ([0-9]+)'
      minimum: 0
```

A `minimum` is a **lower bound**. The healthy value of that number was `0`. No
run of that gate had ever produced a negative skip count, so the proof was
satisfied by every possible execution — including the one it existed to catch,
a tier standing down. Its own comment said so at length and then shipped the
floor anyway, which is the shape of a check that documents its own uselessness
and stays in the file.

The second defect was quieter. The `suite` floor read the **last** cargo
`test result:` line. Cargo prints one per test binary in a fixed order and
`Doc-tests pantry` is always last, at zero passed — so that floor could never be
satisfied by a healthy run either.

The third was an omission. `tests/rls.sh` ran **84 SQL checks** on every
`./bin/prime`, and not one of them appeared in any proof. `rm tests/rls.sh`
would have left the declaration green.

### What replaced it

Seven proofs, each naming a line the gate really prints:

| id | matches | floor |
|---|---|---|
| `toolchain` | `==> toolchain: go 1.26.N (mise.toml pins 1.26.1)` | — |
| `suite` | `suite: N passed, 0 failed` | **30** |
| `rls` | `rls: N of M checks passed, K tier skipped` | **84** |
| `data-path` | `data path: every check ran against a real PostgreSQL (0 skipped)` | — |
| `vet` | `==> go vet ./...` | — |
| `lint` | `==> the manifest validates against core's schema` | — |
| `ok` | `==> ok` | — |

`data-path` is a **presence** proof on the success branch, not a floor on a
count. That is the direct answer to the `minimum: 0` defect: a stood-down
database stops matching the line, so the finding is `gate.proof-missing` and the
gate is red.

**Consequence, stated because it changes behaviour:** a machine with no
PostgreSQL is now red, where it used to be a skip. `bin/prime` still exits 0
there for a developer; the workflow sets `PANTRY_DB_REQUIRED=1` and
`PANTRY_RLS_REQUIRED=1`, which turn both harnesses' stand-down into `exit 3`.

### Why `suite`'s margin is zero

The floor is 30 against a measured 30. The old floor was 144 against a measured
146 — a margin of two, which hid the smallest test file in the suite. **The
margin must be smaller than the smallest unit that can go missing.** In Rust the
smallest unit was a file (`tests/recorded_copy.rs`, four tests). In Go there is
no such unit, because a test file also holds helpers — `internal/pantrydb/
pgtest_test.go` holds `dbGate` and `serve`, and deleting it removes the
compiler's opinion of a test rather than the test. The smallest deletable unit
here is a single `func Test`, so the margin that catches every deletion is
**zero**.

The cost is paid on purpose: a deliberate test **removal** is a red until the
floor is lowered in the same commit.

### Four mutations, each caught by name

| mutation | finding |
|---|---|
| `minimum: 30` → `31` | `FAIL gate.floor: proof 'suite' reported 30 and the declaration's floor is 31` |
| `t.Skip("mutation")` in one test | `FAIL gate.floor: proof 'suite' reported 29 and the declaration's floor is 30` |
| `PANTRY_PG_BIN=/nonexistent` | three: `suite` 20 vs 30, `rls` 0 vs 84, `data-path` `gate.proof-missing` |
| `go vet` step deleted from `bin/prime-go` | `FAIL gate.proof-missing: proof 'vet' never appeared` |

### The near-miss, asserted rather than reasoned about

In a scratch copy, deleting one RLS check (`A2`) and removing the `rls` proof
leaves `checks: 83 run, 83 passed` and `gate_check.py --prove` returns
`OK … 0 failure(s), 3 warning(s)`.

That is the proof that `rls` and `suite` measure genuinely different subjects
and neither substitutes for the other. `tests/rls_checks.sh` is shell and SQL;
it is invisible to a proof that counts `--- PASS:` lines **by construction**. It
is now a case in `bin/gate-self-test` that asserts green, so a future edit that
made the two floors redundant would be noticed.

## 3. CI had a second job that verified nothing

`workspace-drift` cloned twelve repositories anonymously and pointed
`PANTRY_CAFAYE_ROOT` at them, for the four Rust test files that compared
`registry/` against them. All four were deleted. Keeping the job would mean
cloning twelve repositories to verify nothing — a slow, expensive green that
says the fleet was checked when no compiled code here looked at it.

It was deleted, and what is consequently no longer checked anywhere is listed in
the workflow's own trailing comment and in `DECISIONS.md` D33:

1. each `registry/services/<name>/cafaye.yml` is a verbatim copy of that
   service's own manifest at its `recordedAt` commit;
2. `vendir.lock.yml`'s recorded core sha still matches `schemas/`;
3. `registry/index.yml` curates nothing this repository cannot read, and each
   exclusion row's reason is still true;
4. `CAFAYE_UNREADABLE` is still exactly the set an anonymous clone cannot fetch.

None of it is restored here, and none of it should be restored in Rust.

The replacement workflow is one job. It runs `actions/setup-go` with
`go-version-file: go.mod` (the pin lives in two files, one of them
compiler-enforced, rather than three where CI can disagree), proves PostgreSQL
server binaries exist, shallow-clones `caf` as a sibling, and runs `./bin/prime`.

**One measured correction in the PostgreSQL proof.** A first draft branched on
the exit status of `tests/rls.sh --print-pg-bin`. That harness **exits 0** when
it finds nothing, printing `skip: …` on stderr and an empty stdout, because its
contract is "report what I resolved". So the draft would have gone green on a
runner with no PostgreSQL, printed an empty `PANTRY_PG_BIN`, and let the gate
skip 96 checks behind a green badge. The step tests the **value**.

## 4. A false green in my own workflow, found by the self-test

After the rewrite, `bin/gate-self-test`'s case *"a CI workflow whose gate step
carries a different command"* went green over a workflow with no gate step in
it.

core's `check_ci` decides whether CI runs the gate by substring-matching the
declared argv against every `run:` body, and `_appears` is:

```python
return element in body or (element.startswith("./") and element[2:] in body)
```

So a `./` prefix is optional, and **a line that merely CONTAINS the gate's name
satisfies the check**. My own PostgreSQL-proof step ended its error message with
the words `bin/prime would then be a green badge over 84 SQL checks` — inside a
`run:` body. Deleting the gate step from CI would have been silent.

It now says "the gate", and the reason is a comment in the workflow and a **new
green case** in `bin/gate-self-test` that reproduces the blind spot and asserts
it, so a future editor meets it before adding the sentence.

This one is worth more than the rest of the packet's CI work: it is the second
time this repository has shipped a check that could not fail, found by the same
machinery, and the mechanism that found it is now itself a case.

## 5. `bin/gate-self-test`

The self-test copies the repository, breaks exactly one thing per copy, and
asserts core's checker goes red **and names the finding**. Most of its cases were
cargo-shaped and have been rewritten against what is actually here:

| case | was | is |
|---|---|---|
| smallest deletable unit | `rm tests/recorded_copy.rs` (4 tests) | delete one `func Test` from `routes_test.go` |
| the near-miss | delete a Rust file, drop the `total` proof, assert green | delete RLS check `A2`, drop the `rls` proof, assert green |
| suite total gone | patch `bin/prime` | patch `bin/prime-go` (the echo moved) |
| floor raised | `minimum: 7` | `minimum: 30`, and a second on `rls` |
| `minimum: 0` cannot fail | raise a skip floor, assert green | **deleted** — the proof it was about is gone. Replaced by a case that removes the three database proofs and asserts that the remaining declaration is green with no PostgreSQL: the old shape, reconstructed and shown to pass |
| no capture group | cargo `test result:` pattern | `^suite: ([0-9]+) passed, 0 failed$` |
| second capture group | the `total` pattern | the `suite` pattern, with a comment on why **not** `rls` (the `rls` line is only printed on success, so a group appended there matches nothing on a red run and the checker would report `proof-missing` and return before reading the group count — the case would "pass" on a different finding while testing nothing) |
| toolchain check broken | `bin/prime` rustc block | `bin/prime-go` go block |

Also added: the `manifest-tier-stood-down` green twin, which asserts its own
precondition (`$WORK/../caf/go.mod` must not exist) rather than assuming it, and
the CI-mention blind spot from §4.

**Static half, measured:** 37 breakages, 7 warning cases, **0 failures**, 18
skipped (the proving half).

## 6. What is no longer verified

Written here so it cannot be discovered by somebody who believed the word
"checked":

- `registry/` is maintained by reading. See `DECISIONS.md` D33.
- `schemas/` is asserted by nothing. `vendir.lock.yml`'s header says so at
  length, in the file a re-vendor would actually open.
- A registered service with no `exposes.api` is **loaded** rather than refused.
  The Rust `check_kind` went with `src/` and the Go service has no equivalent.
- The loader's own refusals — a file misnamed for its service, a manifest with no
  index row, an index row with no manifest, an unparseable `core` constraint, a
  `kind` the manifest contradicts — are gone, because the Go service does not
  load `registry/` at all. It opens a database.

## 7. Things worth carrying to the next packet

1. **A floor on a count that is zero when healthy cannot fail.** If a number is
   zero in the healthy case, do not give it a floor; match the line that says it
   happened.
2. **A proof that reads the LAST matching line of a multi-target runner is a
   proof about the runner's ordering.** Go prints one `ok` per package; cargo
   printed one `test result:` per binary in a fixed order. Neither is a count.
3. **The margin must be smaller than the smallest deletable unit**, and in a
   language where a test file holds helpers that unit is a function, not a file.
4. **A CI check that greps can be satisfied by a sentence.** `gate.ci` matches a
   substring; a `::error::` message naming the gate is a proof that the gate
   exists.
5. **Print the count of what stood down, and let CI turn it into a red.** The
   `data path:` line plus `PANTRY_DB_REQUIRED=1` is the shape that survives a
   language change; a skip reported only by the test runner did not.
6. **A coverage gap that is written down is recoverable.** One that is papered
   over with a job that clones repositories nobody reads is not.