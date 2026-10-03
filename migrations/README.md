# migrations/README.md — how pantry's schema runs

**The database is the only read source.** `registry/services/<name>/cafaye.yml`
is an *ingest source* now, not something the service reads at request time. A sync
reads the fleet's manifests from git and upserts them into these tables. That
moves the drift check from a test that reads a checkout into an assertion the
ingest makes: a manifest in git that disagrees with its row is an ingest failure
with a diff, not a red test on a machine that happens to have siblings checked
out.

## The files, in the order goose runs them

| # | file | what it owns | what it deliberately does not |
|---|---|---|---|
| 1 | `00001_roles.sql` | the `pantry` schema, `pg_trgm`/`unaccent`, and the four roles every policy names | credentials — cluster provisioning owns passwords |
| 2 | `00002_publishers.sql` | `publishers`; a publisher is a GitHub identity and nothing else | password hash, email, session, plan, billing identity |
| 3 | `00003_services.sql` | `trust_level`/`service_state`/`service_kind`, `services`, `service_versions`, the search indexes | a column per manifest field — the manifest is jsonb and pantry adds no vocabulary to it |
| 4 | `00004_service_compat.sql` | `service_compat`, the compatibility graph, indexed both ways | a version scheme — core's four-form grammar is stored verbatim |
| 5 | `00005_functions.sql` | `current_publisher_id()`, `begin_publisher/1`, `service_is_visible()`, the touch trigger, the `compat_closure` view | anything that decides trust |
| 6 | `00006_rls.sql` | enable + FORCE + policies + grants on all four tables | a `for all` policy — one policy per command, always |
| 7 | `00007_roles_and_compat_read.sql` | the **membership** `pantry` needs to become `pantry_public`, and the public read policy on the compatibility graph | `pantry_admin` — the one membership deliberately NOT granted |
| — | `../tests/rls.sh` | **the denials, run against a real cluster** — 84 checks over every role, every table and every command | a skip that counts as a pass |

## Two goose things that will bite you

**Dollar-quoted bodies need `-- +goose StatementBegin` / `StatementEnd`.** Goose's
SQL parser cannot read a `do $$ … $$;` or an `as $caf$ … $caf$;` without them, and
the error is `failed to parse SQL migration file: unexpected state 0` or
`unterminated dollar-quoted string`, neither of which names the real cause.
identity's `migrations/README.md` records the same thing for the same reason.

**The `$caf$` tag survives verbatim rather than being rewritten to `$$`.** Inside
`StatementBegin`/`StatementEnd` goose hands the block to Postgres whole, and
Postgres parses dollar quoting correctly. A migration that re-tagged the quotes
would work and would also be a fork of kit's shared substrate, which is the thing
kit's README means by "a template that offers one is a template the weaker one
gets chosen from".

**Goose runs in FILENAME order, so a migration cannot use what a later one
defines.** `00003` creates `services` and `00005` creates
`pantry.touch_updated_at()`, so the TRIGGER is in `00005` and not in `00003`
where the table is — a trigger in `00003` fails with
`42883 function pantry.touch_updated_at() does not exist`. This is the ordinary
failure of splitting a migration directory by topic rather than by dependency,
and it was measured on the run that found it rather than reasoned about.

## Running them

```sh
# local, disposable cluster
createdb pantry_dev
DATABASE_URL=postgres://postgres@127.0.0.1:5432/pantry_dev \
  goose -dir migrations postgres "$DATABASE_URL" up

# what it left, and whether it is applied rather than merely checked
goose -dir migrations postgres "$DATABASE_URL" status
```

## NOTHING MIGRATES AT BOOT, and saying so is a decision

There is no entrypoint that migrates before the service starts, and the image is
distroless **because** of that: kit's runtime is debian-slim because its
`docker/entrypoint.sh` needs a shell to run migrations, and with no boot-time
migration there is no script and no shell to buy. `kit`'s own file names the way
back — set `KIT_MIGRATE=off`, migrate from a job, use `distroless/static`.

So today the only thing in this repository that applies these files is
`tests/rls.sh`, and a deployment applies them with `goose … up` run as a separate
step before the service starts.

This is a known gap rather than a design, and it has a consequence worth stating:
**a database that has not been migrated fails at the first query, not at boot.**
`goose status` after `up` is not a second step, it is the only thing that
distinguishes "applied" from "checked" — a gate that runs migrations and reports
nothing cannot tell the two apart, and `identity`'s
`internal/platform/ci/migrate_test.go` exists because of exactly that.

```sh
DATABASE_URL=postgres://postgres@127.0.0.1:5432/pantry_dev \
  goose -dir migrations postgres "$DATABASE_URL" up
goose -dir migrations postgres "$DATABASE_URL" status
```

## The seventh migration is late on purpose

`00006_rls.sql` ends with a section headed "NO PUBLIC READ POLICY, and this is
a decision worth defending because it looks like an omission and it is not". It
then argues for exactly the policy it declines to write — "an edge is visible
exactly when its `target_id` service is visible" — and grants `pantry_public`
SELECT on the table without one. The table was therefore reachable and silent:
four rows, zero readable by the role the catalog reads as.

`00007` writes the statement the argument implies, and requires **both**
endpoints to be visible, which is strictly more than `00006` said. It also
carries the `grant pantry_public to pantry` that the directory had never
contained — see the migration's own header, which is the longer version of this.

It is a new file rather than an edit to `00006` for two reasons, both of which
are about reaching people who already have a database: goose records versions
rather than checksums, so an amended `00006` is silently skipped everywhere it
has been applied, and a Down that revokes a grant made by its own Up leaves state
behind. A new file reaches all of them on the next `up`, and its Down is a true
inverse.

## The `down` order is not reversible on its own

`goose down` walks files in reverse, so `00006`'s Down (disable RLS, drop
policies) runs before `00005`'s Down (drop the view, the trigger, the functions),
which is the dependency order and the only order that works: a view or a policy
whose function is already gone cannot be dropped.

What a partial rollback deliberately leaves behind:

- **`00006`'s Down does not revoke grants.** A Down that took `pantry_public`'s
  SELECT away would leave a role holding a grant it cannot use, which is a weaker
  state than either side of the migration. identity's `00016` Down takes the same
  position for the same reason.
- **`00003`'s Down does not drop the enum types.** `drop type` fails on a database
  where a later migration added a column using one, and a Down that fails leaves
  the database in the state the Down exists to undo. The types live in `pantry`,
  which `00001`'s Down drops with `cascade` — so a full rollback to empty *does*
  remove them, at one level, where the ordering is known.
- **`00001`'s Down does not drop the four roles.** Dropping a role takes with it
  every grant and membership pointing at it, in an order this directory does not
  control.

## The denial suite is NOT a migration directory, and never was

`tests/rls.sh` creates a disposable cluster, applies every file above, and runs
`tests/rls_checks.sh` against it — 77 assertions covering every role, every table
and every command, including the ones that must SUCCEED. Read
`tests/rls_checks.sh`'s header before changing it: every case is either a
statement that **must be refused** or a row that **must read a particular way**.

The placement is structural rather than cosmetic. An earlier version of this suite
was `migrations/assertions/isolation.sql`, a psql script whose **exit code** was
the assertion, with `assertions/assert_denied.sql` beside it for the helper
functions. Both lived in a **subdirectory** because goose's directory scan is **not
recursive** — a file under `migrations/` with a numeric prefix is a migration
whether it wants to be one or not, and the first version of that file was
`migrations/00007_isolation.sql`, which made goose refuse to run the whole
directory:

```
could not parse SQL migration file "migrations/isolation.sql":
  no filename separator '_' found
```

The suite moved to `tests/` and became shell rather than SQL, which also fixed a
failure the SQL version had: it printed `FAIL` and **exited 0**, so the gate
reported eleven failures and `==> ok` in the same run. An assertion that cannot
fail the process is a comment.

```sh
# what the gate runs, and what it needs
PANTRY_PG_BIN=/opt/homebrew/opt/postgresql@18/bin ./tests/rls.sh

# it finds postgres itself: $PANTRY_PG_BIN, then $PATH, then the common
# locations. Set the variable when yours is somewhere unusual.
```
