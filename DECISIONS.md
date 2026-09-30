# pantry's open decisions

Manager-owned, like every other decision in the fleet: **the packet drafts it, the
manager rules on it.** Nothing here is settled, and nothing here is decided by a
test. A decision recorded twice and still open is worse than one recorded once,
because the second reader concludes somebody must already have ruled — so each
entry is numbered once, and a later packet that adds a decision takes the next
number rather than re-arguing an old one.

The decision numbers here are **pantry's own** and deliberately not MD numbers:
`/Users/kaka/Code/any/moon/DECISIONS.md` records decisions about the *fleet* and
MD1/MD2 are the two pantry rulings it has already made. What MD1 and MD2 built
is answered in `AGENTS.md` under "Answered decisions, and where they went", and
is not repeated here — this file is the open list, and it exists because the two
pantry packets so far each found at least one thing they had to choose and could
not derive.

Each entry is also marked in the file it affects, as
`> DECISION NEEDED (pantry):` or with the packet's name, because a decision
nobody can find from the code it is about is a decision that gets made twice.

---

## D1 — a client library is registered as a `cli`, and the gem beside it is not — OPEN

**Found** 2026-09-30, registering `cafaye-ts` (pantry-06).

**The question.** `cafaye-ts` ships the fleet's OpenAPI documents — six of them,
one per service that has one, vendored into `specs/` — and serves nothing. Its
manifest deliberately omits `exposes`, so `serves_http`, `publishes_events` and
`subscribes` are all false and no derived row of the `kind` table reaches it.
`check_kind` refuses `worker` and `both` for a surface-less manifest, which
leaves the two curated values. One of them is guard's (`api`: a service that
serves HTTP and has not written its document yet), and saying it here would be a
falsehood a client acts on, because `kind: api` is a routing instruction.

**What pantry-06 did, and why it is a decision rather than a derivation.**
`kind: cli`, `basePath: null`, registered. `cli` is the honest value among those
the schema permits: installed rather than brought up, routed to by nobody, which
is a true description of an npm package. The argument in full is on the row in
`registry/index.yml` and in `README.md`, "kind" — the short version is that
`api` is a falsehood, `worker` and `both` are refused, and a value that is
merely less wrong is still better than a falsehood a client routes on.

**The cost, stated plainly, because this is the part that needs a ruling.**
MD1 added `cli` for a binary: *installed **and run***. `cafaye-ts` has no `bin`
and no entry point; it is imported. So the third row of the curated set is an
overstatement of the second, and the bill arrives immediately:

| repository | shape | recorded as |
| --- | --- | --- |
| `caf` | a binary, no surface | registered, `kind: cli` |
| `cafaye-ts` | an imported package, no surface, vendors six documents | registered, `kind: cli` |
| `cafaye-rb` | a gem, no surface | **excluded**, `blockedBy: library` |
| `docs` | a static site, no surface | **excluded**, `blockedBy: library` |

**Two client libraries, two answers, in one file.** And the answer a caller gets
depends on the language of the service they happen to be writing: "what do I
install?" is answerable for a TypeScript service and unanswerable for a Ruby
one. That is arbitrary, and `kind` is documented as a column a client switches
on.

**Alternatives.**

1. **Read `cli` as "an artifact a person installs, run or imported", and move
   `cafaye-rb` into `registry/services/` as a second `cli`.** The boundary
   becomes *is it published as an installable artifact*, and `docs` stays a
   `library` because a documentation site is installed by nothing. One row, one
   directory, one reworded value; `kind` means one thing again; and the registry
   answers the question its clients actually ask.
2. **Keep `cli` narrow — a repository with an executable entry point — and move
   `cafaye-ts` to `blockedBy: library` beside `cafaye-rb`.** One answer for one
   shape, and the reading of today's vocabulary is the stricter one. The cost is
   a published client being invisible to `?kind=` and to `?language=typescript`
   as a *service entry*, and a registry that cannot tell a TypeScript developer
   what to install.
3. **A core amendment** — an affordance that distinguishes a library from a
   binary, so the value is derived rather than curated. This is the gap caf's row
   already names, and a fourth repository is now asking for the word. It is a
   core packet; core is read-only from here.

**Recommended: (1), then (3).** (1) is small, it removes a real inconsistency
rather than documenting it, and it is the only option under which the registry
answers the only question a TypeScript or Ruby developer has about it. (3) is
the durable fix and is not this repository's to make.

**Why pantry-06 did not take (1).** It moves a row a different packet wrote, on
a question the packet was not asked, and the instruction for that packet was to
register rather than exclude. Changing `cafaye-rb`'s curation is the manager's
call, so the inconsistency is recorded in four places — this file, the
`cafaye-ts` row, `README.md`, and `CHANGELOG.md` — instead of being resolved
quietly in either direction.

**Cost of flipping to (2):** move one directory, delete one row, `?kind=cli`
returns `[caf]` again, and the two libraries are consistent. **Cost of flipping
to (1):** one directory, one index row, and `cafaye-rb`'s `library` tripwire in
`tests/schema.rs` stops applying to it — correctly, because a registered entry is
checked by four drift tests instead of one, including byte-equality against the
checkout and the `repository.url` origin comparison. It also means the
`workspace-drift` job can clone and check it, which it already lists in
`CAFAYE_UNREADABLE` only because the repository is private.

---

## D2 — what `blockedBy` says about a directory that is not a repository yet — OPEN

**Found** 2026-09-30, the same packet.

**The question.** `moon/cafaye/cafaye-py/` exists, is empty, and is not a git
repository. MD6 (`DECISIONS.md`, the fleet file) ruled that the Python client is
**hand-written and not generated** — hey-api's Python generator is v0.0.24 and
emits parameterless methods with unsubstituted path templates — so the directory
is a placeholder for planned work with a decided shape.

pantry-06's brief offered three answers: remove it, leave it with an explained
exclusion, or something else justifiable. **What it did: left it, and gave it an
`excluded` row with `blockedBy: no-manifest`**, and strengthened the workspace
walk so the directory cannot be invisible again.

**The cost, stated plainly.** `no-manifest` is documented as *"the repository
carries no `cafaye.yml` on master yet"* — and there is no repository. The row's
own value is therefore a slight overstatement of the fact, in the same way
`cafaye-ts`'s `cli` is a slight overstatement of its fact in D1, and for the same
underlying reason: **the vocabulary was built for repositories, and the workspace
now contains things that are not repositories yet.**

There is a second cost, and it is a real coverage gap that a green badge would
hide. In CI the `workspace-drift` job clones repositories; `cafaye-py` is not one,
so the directory is absent and `every_exclusion_reason_is_still_true`'s
`no-manifest` arm — "the file is not there" — is satisfied by a directory that
does not exist. That is a **vacuous pass**, and it is named as one in the row and
in `README.md` rather than being left to be inferred.

**This packet got the second half wrong first, and the tripwire corrected it.**
The row was written with `cafaye-py` deliberately absent from
`CAFAYE_UNREADABLE`, on the reasoning that the list is a claim about what the
runner cannot *read* and a repository nobody has written cannot be read by
anyone. Then `the_drift_job_clones_every_repository_pantry_curates` refused the
whole registration: a curated name the job neither clones nor declares unreadable
is a claim no drift test can check, and cloning it would fail the job's clone step
on a 404 — the same 404 as `cafaye-rb`'s, for the opposite reason. So it is now
declared, with the difference spelled out beside the list.

That correction is the **second half of the argument for a `planned` value**, and
it is stronger than the first half. `CAFAYE_UNREADABLE` is now carrying two
different claims — *the runner has no credential for this* and *there is nothing
here to clone* — under one name, and its own comment has to explain which is
which for each entry. A list whose entries mean different things is a list whose
entries eventually mean neither. `blockedBy` already has the same problem one
level up: `no-manifest` on a directory that is not yet a repository.

**Alternatives.**

1. **What pantry-06 did:** `blockedBy: no-manifest` on a directory that is not
   yet a repository, plus a workspace walk that sees every non-hidden,
   non-worktree directory. The row is a live tripwire — the day a
   `cafaye.yml` appears there, `tests/schema.rs` fails with *"now carries a
   cafaye.yml — register it or change this row's blockedBy and say why it is
   still held back"* — and the walk means no future version of the test can
   step over it.
2. **Remove the directory.** It is empty, so nothing is lost — and nothing is
   *recorded*, which is the problem. An empty directory is not tracked by git, so
   the deletion is unreviewable: a reviewer sees a commit whose entire content is
   a CHANGELOG sentence about something git cannot show them. It also removes
   MD6's Python decision from the filesystem, and `AGENTS.md` says not to touch
   anything outside the worktree besides reading it.
3. **A fifth `blockedBy` value** — `planned`, say: a known cafaye repository that
   does not exist yet, with the decision that shapes it named in the reason. This
   is the honest vocabulary, and it is the same move MD2 made with `library`
   rather than teaching the drift test that documentation sites are exempt. It
   costs a fifth value for one row, and a fifth value is a precedent — but it
   would also give `CAFAYE_UNREADABLE` its honest name back, because a `planned`
   row would not be a coverage claim the runner has to answer for.

**Recommended: (1) now, (3) when a second planned repository appears.** (1) is
correct for a single row and its tripwire is real. The argument for (3) is the
count: `planned` is a vocabulary gap that MD6's own decisions will keep opening —
Go and Rust clients are also unstarted, and a third empty directory is a matter
of time — and the CI correction above has now shown the gap costs something
outside the registry as well. One value per decision, and the first value to
arrive is not this row.

**Cost of flipping to (2):** one `rmdir` outside this worktree, one CHANGELOG
line, and a tripwire that no longer exists. **Cost of flipping to (3):** one
variant in `src/registry.rs`, one arm in `every_exclusion_reason_is_still_true`,
the `blockedBy` table in `registry/index.yml`, the README section that explains
it, and the value's meaning in `openapi/v1.yaml` if it is ever published — which
it is not today, because `blockedBy` is pantry-internal.
