#!/usr/bin/env bash
#
# tests/rls.sh — the RLS suite, against a real PostgreSQL.
#
# WHY THIS IS A SHELL SCRIPT AND NOT A TEST IN tests/*.rs. The Rust tier cannot
# reach a database without a driver in `Cargo.toml`, and this packet is forbidden
# from touching `Cargo.toml`; a Go test would need `pgx` added to `go.mod` and this
# suite's subject is SQL, not Go. `tests/` is cargo's integration-test directory,
# which compiles `tests/*.rs` and ignores everything else, so a `.sh` and a `.sql`
# beside them are inert to the Rust build — verified: `bin/prime` counts test
# binaries with `find tests -maxdepth 1 -name '*.rs'`, so this file does not move
# that number either.
#
# IT STARTS ITS OWN POSTGRES. It does not connect to a developer's, it does not
# read a `PANTRY_DATABASE_URL`, and it needs no container. A suite that needs a
# provisioned database is a suite that does not run on the machine that has to fix
# the database, which is the whole reason this defect survived a careful review:
# nobody had a command that said "the service role cannot read its own schema".
#
#   ./tests/rls.sh                    # a scratch cluster, applied, asserted, removed
#   PANTRY_PG_BIN=/opt/homebrew/opt/postgresql@18/bin ./tests/rls.sh
#   ./tests/rls.sh --keep             # leave the cluster's data dir behind to poke at
#   ./tests/rls.sh --list             # print every check name without running anything
#   ./tests/rls.sh --print-pg-bin     # print the resolved server bin dir and exit
#   ./tests/rls.sh --serve [--empty]  # stand a migrated (seeded) database up and wait
#
# EVERY CHECK EMITS ONE ROW, and a failure names the check, the statement and the
# error the database actually answered. Nothing here infers a permission from a
# grant list: every denial below is a statement that was EXECUTED, and the suite
# asserts on the SQLSTATE and on the message, so it distinguishes the two barriers
# — `permission denied for table` (Postgres rejected it before RLS was consulted)
# from `violates row-level security policy` (the policy refused it).
#
# A MISSING POSTGRES IS A COUNTED SKIP WITH A NAMED REASON, printed on stdout as
# `skip: <reason>` and counted in the `skips:` line, because a skip that is only
# visible in a diff is the silent pass this repository has been bitten by before
# (see the `SKIP`/`--nocapture` argument in `bin/prime`).
set -uo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
KEEP=0
LIST_ONLY=0
PRINT_PG_BIN=0
SERVE=0
SERVE_EMPTY=0
for arg in "$@"; do
  case "$arg" in
    --keep) KEEP=1 ;;
    --list) LIST_ONLY=1 ;;
    --print-pg-bin) PRINT_PG_BIN=1 ;;
    --serve) SERVE=1 ;;
    --empty) SERVE_EMPTY=1 ;;
    -h|--help) sed -n '2,55p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "tests/rls.sh: unknown argument '$arg'. --keep | --list | --print-pg-bin | --serve [--empty] | --help" >&2; exit 2 ;;
  esac
done

# ---------------------------------------------------------------------------
# THE POSTGRES BINARIES, AND THE SKIP THAT SAYS SO
#
# `PANTRY_PG_BIN` IS AUTHORITATIVE WHEN IT IS SET. A hint that is silently ignored
# when it is wrong is worse than no hint: the caller asked for one Postgres and got
# another, and a suite that ran against a different server than the one it was
# pointed at reports its results as if they were the ones asked for. So a set value
# that is not a directory holding `pg_ctl`, `psql` and `initdb` is a named skip
# with the three paths it looked for, not a fall-through to whatever is installed.
# ---------------------------------------------------------------------------
pg_bin_error=""
PGBIN=""

# Checked in the PARENT rather than inside `find_pg_bin`, because a `$( … )`
# capture runs in a subshell and a variable assigned in there does not come back —
# which is how the first version of this printed the generic "no binaries found"
# for a `PANTRY_PG_BIN` that was set and wrong, naming a directory the caller
# could see and a message that did not mention it.
if [[ -n "${PANTRY_PG_BIN:-}" ]]; then
  if [ -x "$PANTRY_PG_BIN/pg_ctl" ] && [ -x "$PANTRY_PG_BIN/psql" ] && [ -x "$PANTRY_PG_BIN/initdb" ]; then
    PGBIN="$PANTRY_PG_BIN"
  else
    pg_bin_error="$PANTRY_PG_BIN"
  fi
fi

find_pg_bin() {
  local c
  for c in /opt/homebrew/opt/postgresql@18/bin \
           /opt/homebrew/opt/postgresql@17/bin \
           /opt/homebrew/opt/postgresql@16/bin \
           /usr/local/opt/postgresql@18/bin \
           /usr/local/opt/postgresql@17/bin \
           /usr/lib/postgresql/18/bin \
           /usr/lib/postgresql/17/bin \
           /usr/lib/postgresql/16/bin; do
    [ -x "$c/pg_ctl" ] && [ -x "$c/psql" ] && [ -x "$c/initdb" ] && { echo "$c"; return 0; }
  done
  if command -v pg_ctl >/dev/null 2>&1 && command -v psql >/dev/null 2>&1; then
    dirname "$(command -v pg_ctl)"; return 0
  fi
  local c
  if command -v brew >/dev/null 2>&1; then
    for c in $(ls -d /opt/homebrew/opt/postgresql@* 2>/dev/null | sort -r) $(ls -d /usr/local/opt/postgresql@* 2>/dev/null | sort -r); do
      [ -x "$c/bin/pg_ctl" ] && { echo "$c/bin"; return 0; }
    done
  fi
  return 1
}

if [[ "$LIST_ONLY" -eq 1 ]]; then
  # One line per named check. The count is one short of what a run reports,
  # because `A0` — the fixture seed — runs once per ownership shape and the two
  # runs share one name. A list that disagreed with the run's own count by an
  # unexplained one would be worse than no list.
  grep -E "^# CHECK " "$REPO/tests/rls_checks.sh" | sed 's/^# CHECK //'
  exit 0
fi

# Only when the caller did not point us somewhere. The `if` is around the fallback
# rather than after it, because a set-but-wrong PANTRY_PG_BIN must NOT fall through
# to whatever is installed — which is exactly what the first version of this did,
# and it is how a wrong hint produced a full green run against a server nobody had
# named.
if [[ -z "$PGBIN" && -z "$pg_bin_error" ]]; then
  PGBIN="$(find_pg_bin)"
fi
if [[ -z "$PGBIN" ]]; then
  # NOT an exit-1: the suite has nothing to assert against, and a suite that
  # cannot run must not be reported as one that failed. It must not be reported
  # as one that passed either — hence the counted row on stdout and a non-zero
  # exit only when the caller asked for a hard requirement (PANTRY_RLS_REQUIRED,
  # which is how CI turns "no database here" into a red instead of a skip).
  if [[ -n "$pg_bin_error" ]]; then
    echo "skip: PANTRY_PG_BIN is set to $pg_bin_error and that directory does not hold"
    echo "      pg_ctl, psql and initdb. It is not falling back to an installed"
    echo "      Postgres — a suite that quietly ran against a different server than"
    echo "      the one it was pointed at would report its results as if they were."
  else
    echo "skip: no PostgreSQL server binaries found — set PANTRY_PG_BIN to the bin"
    echo "      directory holding pg_ctl, psql and initdb (Homebrew:"
    echo "      /opt/homebrew/opt/postgresql@18/bin)."
  fi
  if [[ "${PANTRY_RLS_REQUIRED:-0}" == "1" ]]; then
    echo "tests/rls.sh: PANTRY_RLS_REQUIRED=1 and there is no postgres to run against." >&2
    exit 3
  fi
  exit 0
fi
export PATH="$PGBIN:$PATH"

# ---------------------------------------------------------------------------
# A FREE PORT, and a scratch cluster that is always removed
# ---------------------------------------------------------------------------
WORK="$(mktemp -d "${TMPDIR:-/tmp}/pantry-rls.XXXXXX")"
DATA="$WORK/data"
RESULTS="$WORK/results.tsv"
: >"$RESULTS"

cleanup() {
  local status=$?
  if [[ -d "$DATA" ]]; then
    "$PGBIN/pg_ctl" -D "$DATA" -m immediate stop >/dev/null 2>&1
  fi
  if [[ "$KEEP" -eq 1 ]]; then
    echo "kept: $WORK"
  else
    rm -rf "$WORK"
  fi
  return $status
}
trap cleanup EXIT

PORT="${PANTRY_RLS_PORT:-55470}"
while lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; do
  PORT=$((PORT + 1))
  [[ "$PORT" -gt 55520 ]] && { echo "tests/rls.sh: no free port in 55470-55520" >&2; exit 4; }
done

# `--print-pg-bin` is a MACHINE-FACING mode: it answers with one line on stdout
# and everything else on stderr, so a caller can read stdout and get the path
# without stripping banners. Every other mode is human-facing and keeps its
# narration on stdout where a person running the suite reads it.
say() {
  if [[ "$PRINT_PG_BIN" -eq 1 ]]; then printf '%s\n' "$*" >&2; else printf '%s\n' "$*"; fi
}
say "postgres: $("$PGBIN/postgres" --version)"
say "cluster:  $DATA on 127.0.0.1:$PORT"

if ! "$PGBIN/initdb" -D "$DATA" -U postgres --no-sync -E UTF8 >"$WORK/initdb.log" 2>&1; then
  say "initdb failed:"; tail -20 "$WORK/initdb.log"; exit 5
fi
if ! "$PGBIN/pg_ctl" -D "$DATA" -l "$WORK/server.log" \
      -o "-p $PORT -k $WORK -c listen_addresses=127.0.0.1" -w start >/dev/null 2>&1; then
  say "the scratch cluster did not start on port $PORT:"; tail -20 "$WORK/server.log"; exit 5
fi

# `-t -A` AND NOT `-q`, and the reason is worth the paragraph because both halves
# were wrong on the first run of this suite:
#
#   `-q` suppresses psql's COMMAND TAGS — `INSERT 0 1`, `UPDATE 0`, `DELETE 0` —
#         which are the only evidence that a statement did what it was asked to do.
#   `-t -A` suppresses the column header and the `(N rows)` footer, which are
#         evidence of nothing, and makes one row one line so an exact row set can be
#         compared as a comma-joined string.
#
# So: tags in, headers out. Getting this backwards produced 26 failures on that
# first run, every one of them the harness and none of them the schema — which is
# why the `rowset` helper filters `SET` and blank lines explicitly rather than
# trusting the output to be clean.
PSQL=("$PGBIN/psql" -X -t -A -h 127.0.0.1 -p "$PORT" -U postgres -v ON_ERROR_STOP=1 -v VERBOSITY=verbose)

# db <name> — one statement, no ON_ERROR_STOP surprises.
db()  { timeout 60 "${PSQL[@]}" -d postgres -c "$1" >/dev/null 2>&1; }
sql() { timeout 90 "${PSQL[@]}" -d "$1" -c "$2" 2>&1; }
# run <db> <sql> — the statement, verbatim, with the server's own answer.
run() { timeout 90 "${PSQL[@]}" -d "$1" -c "$2" 2>&1; }

# ---------------------------------------------------------------------------
# THE UP SECTIONS, GOOSE-STYLE. --goose-up and --goose-down delimit, and
# --goose StatementBegin/End are comments to goose and noise to psql, so they go.
# Derived from the files by name so a seventh migration is covered the day it
# lands rather than the day somebody remembers to add it.
# ---------------------------------------------------------------------------
UP="$WORK/up.sql"
{
  for f in "$REPO"/migrations/[0-9]*.sql; do
    echo "-- ===== $(basename "$f") ====="
    awk '
      /^-- \+goose Up/        { up = 1; next }
      /^-- \+goose Down/      { up = 0 }
      /^-- \+goose Statement/ { next }
      up                     { print }
    ' "$f"
  done
} >"$UP"

MIGRATIONS="$(ls "$REPO"/migrations/[0-9]*.sql | wc -l | tr -d ' ')"
say "migrations: $MIGRATIONS files, Up sections only"

# ---------------------------------------------------------------------------
# THE DOWN SECTIONS — AND WHY THEY ARE NOT SIMPLY "THE OTHER HALF OF $UP"
#
# This suite has always applied only the Up half, and the Down half had no check
# at all anywhere in the repository. The tier that used to check it,
# `bin/prime-db`, was superseded by this file and the property went with it
# without anybody deciding it should. The property is worth having: **a Down that
# does not name what its Up created is a deploy nobody can undo**, and the way it
# usually goes wrong is silently — an `Up` gains a constraint in a later commit
# and its `Down` keeps dropping the table as it was before the constraint.
#
# WHY 00001 IS EXCLUDED, and this is the whole reason the naive version of this
# check is vacuous:
#
#     00001_roles.sql's Down is `drop schema if exists pantry cascade;`
#
# That is a CASCADE. Run it and every object any earlier Down forgot is removed
# anyway, so "the schema is empty afterwards" would be true whether or not
# 00002 through 00007's Downs named their own objects. A check that cannot fail
# on the defect it exists for is a comment, and the suite's header already says
# `migrations/assertions/isolation.sql` was retired for printing `FAIL` and
# exiting 0 — the same disease, smaller.
#
# So `DOWN` is 00007 down to 00002, in reverse, and the schema 00001 created is
# left standing. **If that leaves anything behind, this check is red**, which is
# the only version of it worth running. The cascade is still 00001's Down and
# still correct — this is about whether the check can see, not about whether the
# rollback works. Both are asked: `reapply` below puts the schema back from
# `00002` upward, which is the question an operator actually has.
#
# WHY PSQL AND NOT `goose reset`. The old tier shelled out to goose, which makes
# the fleet's migration runner a declared dependency of a gate whose `external`
# block does not name it — and goose on this machine is a `go install` artefact in
# mise's Go bin directory, NOT something `mise install` provides, so a clean
# machine would go red for a reason the declaration had disclaimed. The property
# under test is the SECTIONS, not the runner: given these Down sections in this
# order, does the schema come back empty? psql answers that and adds no
# requirement.
# ---------------------------------------------------------------------------
DOWN="$WORK/down.sql"
{
  # Reverse order, and derived from the filenames so an eighth migration is
  # covered the day it lands. `ls -r` rather than a glob, because the shell does
  # not reverse globs and a file that is silently omitted here is a Down that is
  # silently never checked.
  for f in $(ls -r "$REPO"/migrations/[0-9]*.sql | grep -v '/00001_'); do
    echo "-- ===== $(basename "$f") (Down) ====="
    awk '
      /^-- \+goose Up/        { up = 0 }
      /^-- \+goose Down/      { up = 1; next }
      /^-- \+goose Statement/ { next }
      up                     { print }
    ' "$f"
  done
} >"$DOWN"
say "down:       $(grep -c '^-- ===== ' "$DOWN") migrations, 00001 excluded (its Down is a CASCADE)"

REUP="$WORK/reapply.sql"
{
  # 00001 IS back in this one, and it can be: its Up is guarded
  # (`create schema if not exists`, and every `create role` inside an exception
  # block that skips an existing role), so running it twice is a no-op rather
  # than an error. That guard is now load-bearing for this check, which is
  # worth stating here because the day somebody removes it, this file goes red
  # with `role "pantry" already exists` and the cause is three migrations back.
  for f in "$REPO"/migrations/[0-9]*.sql; do
    echo "-- ===== $(basename "$f") (re-apply) ====="
    awk '
      /^-- \+goose Up/        { up = 1; next }
      /^-- \+goose Down/      { up = 0 }
      /^-- \+goose Statement/ { next }
      up                     { print }
    ' "$f"
  done
} >"$REUP"

# ---------------------------------------------------------------------------
# `--print-pg-bin`: ONE PLACE THAT KNOWS WHERE POSTGRES IS ON THIS MACHINE
#
# The Go suites need a PostgreSQL and they do not get to have their own idea of
# where one is. `find_pg_bin` above already encodes that knowledge — including
# the `PANTRY_PG_BIN` hint being authoritative — and a second copy in Go would be
# a second answer to a question whose whole point is that there is one. So this
# prints the resolved directory and exits, and `internal/pantrydb`'s harness
# shells out to it.
#
# It prints the SAME counted-skip shape on failure, because "there is no Postgres
# here" is not an error in the RLS suite and must not become one in a caller
# either: a Go test that cannot find a database has to say so in a way a summary
# counts, not in a way a CI log hides.
# ---------------------------------------------------------------------------
if [[ "$PRINT_PG_BIN" -eq 1 ]]; then
  if [[ -n "$pg_bin_error" ]]; then
    echo "skip: PANTRY_PG_BIN is set to $pg_bin_error and that directory does not hold" >&2
    echo "      pg_ctl, psql and initdb." >&2
    exit 0
  fi
  if [[ -z "$PGBIN" ]]; then
    echo "skip: no PostgreSQL server binaries found — set PANTRY_PG_BIN" >&2
    exit 0
  fi
  echo "$PGBIN"
  exit 0
fi

# ---------------------------------------------------------------------------
# `--serve`: the SAME harness, standing a database up for the Go suites
#
# `internal/pantrydb` and `internal/httpapi` need a real PostgreSQL with the
# migrations applied and a seeded catalog, because the thing they prove is that
# a query over the real schema answers — a mocked driver would prove that the
# mock agrees with itself. Rather than a second harness (a Go one, a Docker one),
# this is the first one with two more entry points: it finds the server, starts
# the cluster, and applies the migrations exactly as the assertions below run
# against, and then it applies `tests/seed.sql`, prints a connection URL on one
# line, and waits for SIGTERM.
#
#   ./tests/rls.sh --serve            # migrated + seeded, prints the URL
#   ./tests/rls.sh --serve --empty    # migrated, NOT seeded
#
# `--empty` is the middle case of this repository's most important distinction: a
# database that answered and found nothing is not the same answer as a database
# that could not be reached, and the only way to test that honestly is to have
# both available at once.
#
# `00006_rls.sql`'s fixtures are NOT applied here and the reason is that they are
# shaped for `pgx`-free assertions (abstract `'{}'::jsonb` manifests, fixed uuids
# the checks read as literals). `tests/seed.sql` is the catalog a client would
# render. Two seeds, two subjects, one harness.
# ---------------------------------------------------------------------------
if [[ "$SERVE" -eq 1 ]]; then
  SERVED=pantry
  db "drop database if exists $SERVED;" >/dev/null 2>&1
  db "create role pantry_migrator login createrole noinherit;" >/dev/null 2>&1
  db "create database $SERVED owner pantry_migrator;" >/dev/null 2>&1

  SERVE_LOG="$WORK/apply-$SERVED.log"
  if ! timeout 180 "${PSQL[@]}" -U pantry_migrator -d "$SERVED" -f "$UP" >"$SERVE_LOG" 2>&1; then
    say ""
    say "FAIL  migrations do not apply to an empty database (--serve)"
    grep -E "ERROR|FATAL" "$SERVE_LOG" | head -5 | sed 's/^/      /'
    exit 1
  fi

  # NO PROVISIONING HAPPENS HERE ANY MORE, and the absence used to be a hole.
  #
  # This harness used to run `grant pantry_public to pantry` by hand, under a
  # comment that named it as "THE ONE PROVISIONING STATEMENT THE MIGRATIONS DO
  # NOT CONTAIN" and pointed at the data report that found it. That comment was
  # accurate and it was the wrong place for the statement to live: a grant the
  # test suite must perform before the code under test can start is a step
  # missing from the thing that defines the database, and the only reason it
  # went unnoticed is that this file supplied it. Anyone building from
  # `migrations/` with `goose up` and nothing else got
  # `permission denied to set role`.
  #
  # `migrations/00007_roles_and_compat_read.sql` owns it now, and this harness
  # has nothing left to do here — which is the shape of a correct fixture: it
  # applies the migrations and then uses the database, and does not repair it in
  # between. `tests/rls_checks.sh` asserts the membership exists, so a
  # regression is caught here rather than at the first read path start.
  if [[ "$SERVE_EMPTY" -eq 0 ]]; then
    if ! timeout 60 "${PSQL[@]}" -d "$SERVED" -f "$REPO/tests/seed.sql" >"$WORK/seed.log" 2>&1; then
      say ""
      say "FAIL  tests/seed.sql does not apply to a freshly migrated database"
      sed 's/^/      /' "$WORK/seed.log" | head -10
      exit 1
    fi
    say "seeded:  tests/seed.sql"
  else
    say "seeded:  no (--empty: the catalog is genuinely empty, and the database answered)"
  fi

  # The URL, on its own line, on stdout, and nothing else on stdout — a caller
  # reading one line must not have to strip banners.
  echo "postgres://postgres@127.0.0.1:$PORT/$SERVED?sslmode=disable"
  say "serving: press ctrl-c, or send SIGTERM, to stop and remove the cluster"

  # Block until interrupted, then fall through to the EXIT trap, which stops the
  # server and removes the data directory. There is no `--keep` interaction to
  # reason about: a serving harness that leaked a cluster on exit would leave a
  # postmaster behind on a machine three other sessions are working on.
  trap 'exit 0' INT TERM
  while true; do sleep 1; done
fi

# ---------------------------------------------------------------------------
# SHAPE 1 — "tables owned by pantry the way a real deployment has them"
#
# The migrations are applied by `pantry_migrator`, which holds CREATEROLE and is
# NOT a superuser, because `00001_roles.sql` needs `create role` — measured: the
# apply fails with `permission denied to create role` when run as `pantry`. That
# is a real deployment's shape, and it is the shape in which a schema is owned by
# a role that is not the service's.
#
# THE TABLES ARE THEN HANDED TO `pantry` AND THE SCHEMA IS DELIBERATELY NOT. That
# is the point of the harness, and getting it wrong makes the whole suite
# vacuous: a schema's OWNER holds USAGE implicitly, not through any grant, so a
# run in which `pantry` owns the schema would pass whether or not this directory
# grants it. The suite below is green only when the grants in `00006_rls.sql`
# actually supply the privilege.
# ---------------------------------------------------------------------------
SHAPE1=regshape1
db "drop database if exists $SHAPE1;" >/dev/null 2>&1
db "create role pantry_migrator login createrole noinherit;" >/dev/null 2>&1
db "create database $SHAPE1 owner pantry_migrator;" >/dev/null 2>&1

APPLY_LOG="$WORK/apply-$SHAPE1.log"
if ! timeout 180 "${PSQL[@]}" -U pantry_migrator -d "$SHAPE1" -f "$UP" >"$APPLY_LOG" 2>&1; then
  say ""
  say "FAIL  migrations do not apply to an empty database (shape 1: pantry_migrator)"
  grep -E "ERROR|FATAL" "$APPLY_LOG" | head -5 | sed 's/^/      /'
  exit 1
fi
say "applied: $MIGRATIONS migrations as pantry_migrator (exit 0)"

if ! sql "$SHAPE1" "alter table pantry.publishers owner to pantry;
                   alter table pantry.services owner to pantry;
                   alter table pantry.service_versions owner to pantry;
                   alter table pantry.service_compat owner to pantry;" | grep -q ERROR; then
  sql "$SHAPE1" "alter table pantry.publishers owner to pantry;
                 alter table pantry.services owner to pantry;
                 alter table pantry.service_versions owner to pantry;
                 alter table pantry.service_compat owner to pantry;" >/dev/null
fi

# ---------------------------------------------------------------------------
# SHAPE 2 — the same migrations with `pantry` owning the schema as well
#
# Both shapes have to be green, and they are green for different reasons, which is
# what makes the fix deployment-independent rather than a fix for one operator's
# provisioning. Shape 2's service-role checks are the ones a schema owner would
# otherwise be unable to fail.
# ---------------------------------------------------------------------------
SHAPE2=regshape2
db "drop database if exists $SHAPE2;" >/dev/null 2>&1
db "create database $SHAPE2 owner pantry_migrator;" >/dev/null 2>&1
APPLY2="$WORK/apply-$SHAPE2.log"
if ! timeout 180 "${PSQL[@]}" -U pantry_migrator -d "$SHAPE2" -f "$UP" >"$APPLY2" 2>&1; then
  say ""
  say "FAIL  migrations do not apply to an empty database (shape 2: pantry_migrator, owner-shape)"
  grep -E "ERROR|FATAL" "$APPLY2" | head -5 | sed 's/^/      /'
  exit 1
fi
sql "$SHAPE2" "alter schema pantry owner to pantry;
               alter table pantry.publishers owner to pantry;
               alter table pantry.services owner to pantry;
               alter table pantry.service_versions owner to pantry;
               alter table pantry.service_compat owner to pantry;" >/dev/null
say "applied: $MIGRATIONS migrations as pantry_migrator, objects handed to pantry (exit 0)"

# ---------------------------------------------------------------------------
# SHAPE 3 — the round trip: up, all the way down, and up again
#
# Three applies and two questions. The applies are allowed to fail LOUDLY here,
# before any check runs, because a Down section that raises is a different
# failure from a Down section that succeeds and forgets something, and the first
# is worth stopping for.
#
#   1. UP      -> the schema as a fresh deployment gets it
#   2. DOWN    -> 00007..00002 in reverse, 00001 held back (see above)
#   3. RE-UP   -> every migration again, 00001 included
#
# Step 3 is not redundant with step 2. It is the question an operator has: after
# a rollback, does the next deploy work? A Down that leaves one constraint behind
# passes step 2 without complaint and fails step 3 with `already exists`, and
# that is the shape of the bug worth catching.
#
# ONE HAZARD, STATED BECAUSE IT IS REAL AND IT IS NOT CONTAINED HERE:
# **`00007`'s Down is `revoke pantry_public from pantry`, and role membership is
# CLUSTER-WIDE, not per-database.** Running this shape therefore mutates state
# that shapes 1 and 2 also depend on. It is restored by the re-apply in step 3,
# which is why the whole shape is built before `tests/rls_checks.sh` is sourced
# and every check runs against the restored cluster. If step 3 ever fails, the
# membership checks I1-I3 will go red underneath it with a message about roles
# rather than about a migration — so step 3's failure exits here, loudly, and
# does not get to be confusing.
# ---------------------------------------------------------------------------
SHAPE3=regshape3
db "drop database if exists $SHAPE3;" >/dev/null 2>&1
db "create database $SHAPE3 owner pantry_migrator;" >/dev/null 2>&1

UP3="$WORK/apply-$SHAPE3-up.log"
if ! timeout 180 "${PSQL[@]}" -U pantry_migrator -d "$SHAPE3" -f "$UP" >"$UP3" 2>&1; then
  say ""
  say "FAIL  migrations do not apply to an empty database (shape 3)"
  grep -E "ERROR|FATAL" "$UP3" | head -5 | sed 's/^/      /'
  exit 1
fi

DOWN3="$WORK/apply-$SHAPE3-down.log"
if ! timeout 180 "${PSQL[@]}" -U pantry_migrator -d "$SHAPE3" -f "$DOWN" >"$DOWN3" 2>&1; then
  say ""
  say "FAIL  a -- +goose Down section does not run (shape 3)"
  grep -E "ERROR|FATAL" "$DOWN3" | head -5 | sed 's/^/      /'
  exit 1
fi
say "down:      00007..00002 in reverse, exit 0"

REUP3="$WORK/apply-$SHAPE3-reup.log"
if ! timeout 180 "${PSQL[@]}" -U pantry_migrator -d "$SHAPE3" -f "$REUP" >"$REUP3" 2>&1; then
  say ""
  say "FAIL  the migrations do not re-apply after a full rollback (shape 3)"
  grep -E "ERROR|FATAL" "$REUP3" | head -5 | sed 's/^/      /'
  exit 1
fi
say "re-applied: $MIGRATIONS migrations after the rollback (exit 0)"

# ---------------------------------------------------------------------------
# THE POST-DOWN STATE, FROZEN — because the emptiness assertion has to be taken
# WHILE IT IS TRUE, and shape 3 is three applies later by the time a check runs.
#
# The alternative was to capture the observation into a variable and compare it
# in `rls_checks.sh`, which reads the same and is worse: the value would have
# been formatted by this file and asserted by that one, so a change to either
# half's idea of the answer could not fail. A second database costs two applies
# and lets the checks query the real catalog while it is in the state they are
# about.
#
# It is the SAME applies shape 3 does — deliberately, not by sharing a variable:
# if these two ever diverge, one of them is testing something else and the
# difference should be visible as a green check with the wrong subject rather
# than as a shared fixture that quietly changed meaning.
# ---------------------------------------------------------------------------
DOWNSTATE=regdown
db "drop database if exists $DOWNSTATE;" >/dev/null 2>&1
db "create database $DOWNSTATE owner pantry_migrator;" >/dev/null 2>&1

if ! timeout 180 "${PSQL[@]}" -U pantry_migrator -d "$DOWNSTATE" -f "$UP" >"$WORK/apply-$DOWNSTATE-up.log" 2>&1; then
  say ""
  say "FAIL  migrations do not apply to an empty database (post-down state)"
  grep -E "ERROR|FATAL" "$WORK/apply-$DOWNSTATE-up.log" | head -5 | sed 's/^/      /'
  exit 1
fi
if ! timeout 180 "${PSQL[@]}" -U pantry_migrator -d "$DOWNSTATE" -f "$DOWN" >"$WORK/apply-$DOWNSTATE-down.log" 2>&1; then
  say ""
  say "FAIL  a -- +goose Down section does not run (post-down state)"
  grep -E "ERROR|FATAL" "$WORK/apply-$DOWNSTATE-down.log" | head -5 | sed 's/^/      /'
  exit 1
fi
say "post-down: $DOWNSTATE, frozen after 00007..00002 and before the re-apply"

# THE ONE THING PUT BACK BY HAND, AND IT IS NOT THE SCHEMA.
#
# `00007`'s Down is `revoke pantry_public from pantry`, and role membership is
# CLUSTER-WIDE. So building `$DOWNSTATE` revoked a grant that every check below —
# and every check against shapes 1 and 2 — depends on, and the first version of
# this file took I1 and I2 down with it, correctly reporting `0 members` against
# a cluster the harness itself had broken.
#
# Shape 3 does not have this problem: its re-apply puts the grant back. This one
# cannot, because staying rolled back IS the state the J checks are about.
#
# So the grant goes back here, immediately, and **only the grant** — the schema
# stays exactly as the Down sections left it, which is the whole point of the
# database. Membership and schema are different scopes and the fix respects that:
# one cluster-wide statement, no DDL, nothing that J1 can see.
#
# That the repair is a hand-written grant is itself the finding, and it is the
# same one `00001` already recorded: **a grant the test suite must perform is a
# step missing from the thing that defines the database.** Here it is missing
# from the ROLLBACK rather than from the provisioning, which is a smaller
# version of the same disease and is now named rather than absorbed.
db "grant pantry_public to pantry;" >/dev/null 2>&1

# ---------------------------------------------------------------------------
# WHAT A FRESH MIGRATE LOOKS LIKE, MEASURED — so J3 and J4 compare against a
# baseline rather than against a list somebody wrote down.
#
# NOT `dblink`. Comparing two databases from inside one is the obvious tool and
# it costs an extension this harness does not have and does not want: the RLS
# suite's entire claim is that it needs nothing but PostgreSQL server binaries.
# So the baseline is measured here, in this file, and interpolated into the
# checks as a literal — derived at run time from a real migration rather than
# typed into a test, which is the property that actually matters. An eighth
# migration lands and both sides move together.
# ---------------------------------------------------------------------------
BASE_TABLES="$(sql "$SHAPE1" "select coalesce(string_agg(tablename, ',' order by tablename), '<none>')
                                from pg_tables where schemaname = 'pantry';")"
BASE_POLICIES="$(sql "$SHAPE1" "select count(*) from pg_policies where schemaname = 'pantry';")"
say "baseline:  $BASE_POLICIES policies over $(printf '%s' "$BASE_TABLES" | tr ',' '\n' | wc -l | tr -d ' ') tables, from a fresh migrate"

# ===========================================================================
# THE CHECK RUNNER
# ===========================================================================
PASS=0; FAIL=0; SKIPN=0
FAILED_NAMES=()

record() { printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >>"$RESULTS"; }

first_line() { printf '%s' "$1" | head -1 | tr -d '\n'; }
oneline()    { printf '%s' "$1" | tr '\n' ' ' | tr -s ' ' | sed 's/^ *//;s/ *$//'; }

# ok <name> <db> <sql> [expect-in-output]
# The statement ran and exited 0, and — when a fragment is given — the fragment is
# in the output. Used for the positives AND for the row counts, because an
# assertion that a role "sees its own row" has to name the row it saw.
ok() {
  local name="$1" dbn="$2" stmt="$3" frag="${4:-}"
  local out rc
  out="$(run "$dbn" "$stmt")"; rc=$?
  if [[ $rc -ne 0 ]]; then
    record "$name" FAIL "exit=$rc" "$(oneline "$out")"; FAIL=$((FAIL+1)); FAILED_NAMES+=("$name")
    say "FAIL  $name"
    say "        $(oneline "$out")"
    return 1
  fi
  if [[ -n "$frag" ]] && ! printf '%s' "$out" | grep -qF -- "$frag"; then
    record "$name" FAIL "missing: $frag" "$(oneline "$out")"; FAIL=$((FAIL+1)); FAILED_NAMES+=("$name")
    say "FAIL  $name"
    say "        expected output to contain: $frag"
    say "        got: $(oneline "$out")"
    return 1
  fi
  local observed; observed="$(oneline "$out" | sed 's/^ *//')"
  record "$name" ok "$frag" "$observed"; PASS=$((PASS+1))
  return 0
}

# deny <name> <db> <sqlstate> <message-fragment> <sql>
# The statement ran, exited NON-ZERO, and the database said the two things the
# check is about: which SQLSTATE, and which of the two barriers refused. A grant
# boundary answers `permission denied for table`; a policy boundary answers
# `violates row-level security policy`. This is the difference between "the public
# role has no write grant" and "the public role has a write grant and a policy
# stops it", and it is the difference this repository's own header says it wants.
deny() {
  local name="$1" dbn="$2" state="$3" frag="$4" stmt="$5"
  local out rc
  out="$(run "$dbn" "$stmt")"; rc=$?
  if [[ $rc -eq 0 ]]; then
    record "$name" FAIL "statement SUCCEEDED" "$(oneline "$out")"; FAIL=$((FAIL+1)); FAILED_NAMES+=("$name")
    say "FAIL  $name"
    say "        the statement was allowed. output: $(oneline "$out")"
    return 1
  fi
  if ! printf '%s' "$out" | grep -qE "$state"; then
    record "$name" FAIL "wrong sqlstate, wanted $state" "$(oneline "$out")"; FAIL=$((FAIL+1)); FAILED_NAMES+=("$name")
    say "FAIL  $name"
    say "        wanted SQLSTATE $state, got: $(oneline "$out")"
    return 1
  fi
  if ! printf '%s' "$out" | grep -qF -- "$frag"; then
    record "$name" FAIL "refused by the wrong barrier, wanted '$frag'" "$(oneline "$out")"
    FAIL=$((FAIL+1)); FAILED_NAMES+=("$name")
    say "FAIL  $name"
    say "        SQLSTATE matched but the refusal came from somewhere else: $(oneline "$out")"
    return 1
  fi
  record "$name" ok "$state" "$(oneline "$out" | sed 's/^ *//')"; PASS=$((PASS+1))
  return 0
}

# rowset <name> <db> <sql> <expected-csv>
# The strongest form: an exact row set. "the publisher sees only its own rows" is
# not `count(*) = 2` — it is the two names, and NOT the third one.
#
# The filter is not tidiness. Every statement in this suite begins with a `set role`
# or a `begin_publisher/1` call, and psql prints a `SET` tag and an empty line for
# the void function, so a row set captured without filtering carries two rows of
# scaffolding into the expectation.
rowset() {
  local name="$1" dbn="$2" stmt="$3" want="$4"
  local got
  got="$(run "$dbn" "$stmt" \
    | grep -v '^SET$' | grep -v '^$' | grep -v '^-*$' | grep -v '^(1 row' \
    | sed 's/^ *//;s/ *$//' | paste -sd, -)"
  if [[ "$got" == "$want" ]]; then
    record "$name" ok "$want" "$got"; PASS=$((PASS+1)); return 0
  fi
  record "$name" FAIL "wanted: $want" "got: $got"; FAIL=$((FAIL+1)); FAILED_NAMES+=("$name")
  say "FAIL  $name"
  say "        wanted: $want"
  say "        got:    ${got:-<no rows>}"
  return 1
}

# source: the fixtures, the shape-1 database, and every check.
# shellcheck disable=SC1091
source "$REPO/tests/rls_checks.sh"

# ---------------------------------------------------------------------------
# THE SUMMARY. Counts first, names second, and the exit status is FAIL>0.
# ---------------------------------------------------------------------------
say ""
say "----------------------------------------------------------------"
say "checks: $((PASS+FAIL)) run, $PASS passed, $FAIL failed, $SKIPN skipped"
if [[ "$FAIL" -gt 0 ]]; then
  say "failed:"
  for n in "${FAILED_NAMES[@]}"; do say "  - $n"; done
  say ""
  say "postgres: $("$PGBIN/postgres" --version)"
  say "workdir:  ${WORK}"
  exit 1
fi
say "skips: $SKIPN"
say "postgres: $("$PGBIN/postgres" --version)"
say "ok"
exit 0