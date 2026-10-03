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

  # THE ONE PROVISIONING STATEMENT THE MIGRATIONS DO NOT CONTAIN, and it is
  # load-bearing for the read path rather than incidental.
  #
  # `00001_roles.sql` creates `pantry` as LOGIN and `pantry_public` as NOLOGIN,
  # and it grants the GROUP role its table privileges — but it never grants the
  # LOGIN role MEMBERSHIP in the group. `NOINHERIT` is the point (a membership
  # must be taken with `set role`, never inherited silently), and taking one
  # requires membership. So on a database built purely from this directory,
  # `set role pantry_public` fails with `permission denied to set role`, and the
  # service's read path cannot start.
  #
  # This is stated here rather than worked around in Go, because the alternative
  # — connecting as `pantry` and relying on its own policies — reads ZERO rows:
  # with no identity, `current_publisher_id()` is NULL and every publisher
  # predicate compares against NULL. That is `FORCE` working, and it means the
  # membership is not a convenience, it is the only way the catalog can be read.
  # See `REPORT-registry-pantry-data-01.md` finding 2.
  sql "$SERVED" "grant pantry_public to pantry;" >/dev/null

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