# REPORT — registry-migdown-04

**Repo:** `cafaye/pantry` · **Branch:** `worker/reg-migdown-04` · **Base:** `78f2ccb`

**The packet in one line:** find out whether this repository can undo its own
schema, and find out by making the gate do it rather than by reading the `-- +
goose Down` sections and hoping.

**It could not.** The rollback was arrested by a missing `drop policy` that had
been sitting in `migrations/00006_rls.sql` since that migration landed, and
nothing in the repository — not the gate, not CI, not `tests/rls.sh` — could see
it. Both are now fixed.

---

## 1. How this packet happened, which is the part worth reading

The previous packet was `registry-norust-03`. Before deleting its worktree I
went looking at `wt-reg-pantry-schema-01`, an unmerged branch from an earlier
packet in this stream, to decide whether it held anything unique or was safe to
retire. I expected it to be superseded: `tests/rls_checks.sh` had replaced its
`migrations/assertions/isolation.sql`, `migrations/README.md` documented the
replacement, and the schema-02 packet had re-decided its D31.

That was true of everything except one thing. `bin/prime-db` on that branch had
a tier:

```
step "every -- +goose Down names everything its Up created"
goose -dir migrations postgres "$DB_URL" reset
goose -dir migrations postgres "$DB_URL" up
```

and `tests/rls.sh` has **no equivalent and never had**. It extracts the Up
sections with `awk` and applies them with `psql` — the header says so, and it is
a good decision for a different reason — but the Down half was simply not applied
anywhere. So the property went out with the file that asserted it, in a
supersession nobody had decided was a deletion.

**A coverage gap is created the same way whether you delete it deliberately or
delete it by replacing the thing that held it.** The `registry-norust-03` report
says this about `registry/`. It turns out it applies one tier over, to the file
that report itself pointed at.

## 2. Why the obvious version of this check is vacuous

The first thing I wrote was `goose reset`: run every Down in reverse, assert the
schema is empty, run every Up again. It is the check `bin/prime-db` ran, and it
cannot fail, for a reason that is not obvious until you look at `00001`:

```sql
-- +goose Down
drop schema if exists pantry cascade;
```

`00001_roles.sql`'s Down is a **CASCADE**. Run it and every object any later
migration's Down forgot is removed anyway. "The schema is empty afterwards" is
true whether or not `00002` through `00007`'s Downs named their own objects — and
if `00002`'s Down were an empty string the check would still be green.

So this check holds `00001` back:

```
DOWN = the Down sections of 00007..00002, in reverse
```

The schema `00001` created is left standing, and it must be standing over
nothing. **That is the only version of the check that can see anything**, and the
difference between the two is the whole packet.

## 3. What it found

The first run, with no mutation applied to anything but the new code, failed:

```
FAIL  a -- +goose Down section does not run (shape 3)
      ERROR: 2BP01: cannot drop function pantry.service_is_visible(pantry.services)
             because other objects depend on it
```

Diffing the policies `00006_rls.sql` creates against the ones its Down drops:

```
21d20
< service_versions_public_read
```

**One policy out of 33 was created and never dropped.** It is the last thing
standing on `pantry.service_is_visible()`, which is what `00005`'s Down tries to
drop — so the rollback did not merely leave an object behind, it **stopped**, and
`00003`, `00002` and the rest never ran. The fix is one line, and it carries the
longest comment in the migration file, because the reason this survived is the
reason it is worth writing down:

> A Down section is code that has never run until something runs it. It sits
> beside an Up that is exercised on every gate run, reads like it was exercised
> too, and is a paragraph of guessed `drop` statements.

## 4. Five mutations, and an honest account of which check caught what

Every mutation below was applied, run, and reverted. `checks: 89 run, 89 passed`
after each revert.

| | mutation | caught by |
|---|---|---|
| M1 | `00006` Down stops dropping `services_public_read` | the Down apply, `2BP01` — the original defect, reproduced |
| M2 | `00005` Down stops dropping the trigger | the re-apply, `trigger already exists` |
| M3 | `00002` Down stops dropping `pantry.publishers` | the re-apply, `relation already exists` |
| M4 | **`00001` put back into the Down chain** (with M3's leftover) | `gate` — **J1 stays green**, J2 goes red |
| M5 | `00005` Down stops dropping `compat_closure` | the Down apply, `2BP01` on `service_compat` |

**M4 is the one that matters**, and it is why J2 exists. With `00001`'s cascade
back in the chain, the leftover table from M3 is removed anyway and **J1 — the
emptiness assertion, the check the whole section is built around — is green over a
database where six of six Downs did nothing.** The only check that noticed was the
one asserting the schema was still *there*, which is the complement. Neither check
proves the other; the pair does.

**What I could not show, stated rather than glossed.** J1 never went red on its
own. Every leftover object I could construct was caught earlier and more loudly,
by the apply step or the re-apply, because this schema's Down chain is
dependency-dense (`drop table` refuses rather than cascades) and every Up is
either `create or replace` or re-run against a clean slate. J1's independent value
is therefore narrower than I first wrote in its comment: it is the guard for a
leftover that neither errors nor breaks the re-apply, and no such leftover exists
in this schema today. It stays because it is one query and it is the check that
*would* notice — but it is not carrying the weight M1–M5 put on the tier.

## 5. The hazard I created, and the shape it took

`00007`'s Down is `revoke pantry_public from pantry`, and **role membership is
cluster-wide, not per-database.** Building a database that stays rolled back
therefore revoked a grant that checks I1–I3 depend on, and the suite reported:

```
FAIL  I1  membership: pantry is a member of pantry_public
        expected output to contain: 1
        got: 0
```

Correctly. The harness broke the cluster and said so. The repair is one statement
in `tests/rls.sh` and it is commented as a finding rather than as tidying:

> That the repair is a hand-written grant is itself the finding, and it is the
> same one `00001` already recorded: **a grant the test suite must perform is a
> step missing from the thing that defines the database.** Here it is missing
> from the ROLLBACK rather than from the provisioning.

Membership and schema are different scopes and the repair respects that: the
grant goes back, no DDL runs, and J1 cannot see it.

## 6. `psql`, not `goose`

The old tier shelled out to goose. This one does not, and the reason is about
what the gate is allowed to need:

`gate.yml`'s `external.requirements` names a Go toolchain, a module proxy, a
PostgreSQL server, and a `caf` checkout. **goose is not among them, and on this
machine it is a `go install` artefact in mise's Go bin directory — not something
`mise install` provides.** A clean machine would have gone red for a reason the
declaration had explicitly disclaimed. The property under test is the *sections*,
not the runner, and `psql` asks it with no new requirement. This is the same
reason `tests/rls.sh` builds its own scratch cluster instead of asking for a
`postgres` service container.

## 7. What landed

- `tests/rls.sh` — extracts `$DOWN` (00007..00002, reverse) and `$REUP`, builds
  **shape 3** (up → down → up, failing loudly at each step) and **`$DOWNSTATE`**, a
  second database frozen in the rolled-back state so the emptiness assertion can
  be queried live. Measures `$BASE_TABLES` and `$BASE_POLICIES` from shape 1 so
  the round-trip checks compare against a **derived** baseline, not a typed list.
- `tests/rls_checks.sh` — group **J**, five checks: J1 nothing left behind, J2
  the schema survives (the M4 guard), J3 the re-applied table set matches a fresh
  migrate, J4 the policy count matches, J5 `FORCE` survived.
- `migrations/00006_rls.sql` — the missing `drop policy`, and the comment.
- `gate.yml` and every document that states the count — **84 → 89**, margin still
  zero. `DECISIONS.md` and `CHANGELOG.md` entries that say 84 are left alone: they
  record what was true when they were written.

**Not `dblink`.** Comparing shape 3 against shape 1 from inside one database is
the obvious tool and it costs an extension this harness does not have. The suite's
entire claim is that it needs nothing but PostgreSQL server binaries, and the
baseline is measured in `tests/rls.sh` and interpolated instead.

## 8. Verification

```
bash tests/rls.sh    →  checks: 89 run, 89 passed, 0 failed, 0 skipped
./bin/prime          →  data path: every check ran against a real PostgreSQL (0 skipped)
                        suite: 30 passed, 0 failed
                        rls: 89 of 89 checks passed, 0 tier skipped
                        ==> ok
```

Five mutations, each reverted, each caught by a named failure — tabled in §4.

## 9. The single next move

**Run this same check against `identity`.** I measured the fleet rather than
guessing at it, and the measurement is narrower than I expected:

| service | migrations | with a `-- +goose Down` |
|---|---|---|
| pantry | 7 | 7 |
| **identity** | **17** | **17** |
| muse | 2 | **0** |
| darkroom | 3 | **0** |
| courier, billing, parlor, kit, guard | — no `migrations/` | — |

So there are two follow-ups, and the second is the one I would not have predicted:

1. **`identity` is the same defect at seventeen times the size.** Every one of its
   migrations has a Down section, none is exercised by anything, and identity
   holds the most consequential schema in the fleet. `service_versions_public_read`
   was one forgotten line in 33, found by the first run of a check that took
   twenty minutes to write. Port `tests/rls.sh`'s `$DOWN`/`$REUP` extraction
   there and run it — expect it to be red before it is green.
2. **`muse` and `darkroom` have migrations with NO Down section at all.** That is
   not "the Down is untested", it is "there is no Down": a rollback for those
   schemas is a `drop database` and everything in it. If that is deliberate —
   these are pre-release services and a rebuild is cheaper than a migration — it
   should be written down, because the absence currently reads like an oversight
   in a directory where every neighbouring file has one.

