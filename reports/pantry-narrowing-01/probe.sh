#!/usr/bin/env bash
#
# reports/pantry-narrowing-01/probe.sh — WHICH MECHANISM refuses
# `update pantry.publishers set id = …` by a publisher, once
# publishers_publisher_update's own with check says `true`?
#
# WHY THIS FILE IS NOT A MUTATION. REPORT-pantry-rls-mutations-01.md §4 (B2) left
# this open: with `with check (true)` on publishers_publisher_update, a publisher
# could set `verified = false` but could NOT move `publishers.id` onto another
# publisher's value, and the report declined to name the mechanism. This file
# names it, and it is not the primary key.
#
# IT READS NOTHING AND WRITES NOTHING IN THE REPOSITORY. It stands up the same
# scratch cluster tests/rls.sh builds (`--serve --empty`), alters policies on THAT
# cluster, and deletes the cluster on exit. `migrations/` and `tests/` are never
# touched, so there is nothing to restore: this file cannot leave the tree dirty
# even if it is interrupted, which is why it is not built like mutations.sh is.
#
#   ./reports/pantry-narrowing-01/probe.sh          # run it
#   ./reports/pantry-narrowing-01/probe.sh --keep   # leave the cluster up to poke at
#
# ---------------------------------------------------------------------------
# THE ANSWER, SO A READER CAN CHECK THE CLAIM BEFORE READING THE EVIDENCE
# ---------------------------------------------------------------------------
# `publishers_publisher_select` — a **FOR SELECT** policy — is what refuses it.
#
# PostgreSQL applies a SELECT policy to an UPDATE whenever the statement reads a
# column of the relation (CREATE POLICY, "Per-Command Policies" / SELECT, and
# Table 300: the UPDATE row's SELECT/ALL-policy column reads "Filter existing
# row [a] & check new row [a]", footnote [a] = "If read access is required to
# either the existing or new row"). So a publisher's
#
#     update pantry.publishers set id = … where github_id = 101
#
# is filtered by publishers_publisher_select's USING on the old row AND checked by
# the same USING on the new row. publishers_publisher_update's own WITH CHECK is
# not what refuses it — with that clause set to `true` and to `true` again, the
# refusal is byte-for-byte the same error.
#
# `publishers_pkey` and `services_publisher_id_fkey` are a DIFFERENT barrier, and
# they are downstream: they only become visible once the policy is opened. Two
# SQLSTATEs, two worlds:
#
#     42501  new row violates row-level security policy for table "publishers"
#     23505  duplicate key value violates unique constraint "publishers_pkey"
#     23503  update or delete on table "publishers" violates foreign key
#            constraint "services_publisher_id_fkey" on table "services"
set -uo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)" || exit 2
cd "$REPO" || exit 2
KEEP=0
[ "${1:-}" = "--keep" ] && KEEP=1

SERVE_LOG="$(mktemp "${TMPDIR:-/tmp}/pantry-narrow-probe.XXXXXX")"
CLUSTER_PID=""
cleanup() {
  if [ -n "$CLUSTER_PID" ] && [ "$KEEP" -eq 0 ]; then
    kill "$CLUSTER_PID" 2>/dev/null
    for _ in 1 2 3 4 5 6 7 8 9 10; do kill -0 "$CLUSTER_PID" 2>/dev/null || break; sleep 1; done
    kill -9 "$CLUSTER_PID" 2>/dev/null
  fi
  [ "$KEEP" -eq 0 ] && rm -f "$SERVE_LOG"
  return 0
}
trap cleanup EXIT INT TERM

say()  { printf '%s\n' "$*"; }
rule() { say "------------------------------------------------------------------------"; }

# ---------------------------------------------------------------------------
# 1. the same scratch cluster the suite builds, so the answer is about THIS repo
# ---------------------------------------------------------------------------
say "pantry-narrowing-01 / probe — which barrier refuses a publishers.id move"
say "repo:   $REPO"
say "branch: $(git rev-parse --abbrev-ref HEAD) @ $(git rev-parse --short HEAD)"
say ""

# PANTRY_PG_BIN is authoritative, the documented candidates come next, and if none
# of them holds a psql this ABORTS. It does NOT fall through to a different server
# than the cluster below: a probe that answered about a Postgres the reader did not
# ask about is worse than no probe, and the first version of this file resolved
# psql with `./tests/rls.sh --print-pg-bin`, which prints a whole second cluster's
# startup banner into this file's stdout before it prints the path.
resolve_pg_bin() {
  local c
  for c in "${PANTRY_PG_BIN:-}" /opt/homebrew/opt/postgresql@18/bin \
           /opt/homebrew/opt/postgresql@17/bin /usr/local/opt/postgresql@18/bin \
           /usr/lib/postgresql/18/bin /usr/lib/postgresql/17/bin; do
    [ -n "$c" ] && [ -x "$c/psql" ] && { printf '%s' "$c"; return 0; }
  done
  command -v psql >/dev/null 2>&1 && { dirname "$(command -v psql)"; return 0; }
  return 1
}
PGBIN="$(resolve_pg_bin)" || {
  say "ABORT  no psql found. Set PANTRY_PG_BIN to the bin dir holding psql, pg_ctl"
  say "        and initdb. Refusing to answer about a server nobody named."
  exit 2
}
say "pg bin:  $PGBIN"

timeout 300 ./tests/rls.sh --serve --empty >"$SERVE_LOG" 2>&1 &
CLUSTER_PID=$!
URL=""
for _ in $(seq 1 60); do
  URL="$(sed -n 's#^postgres://postgres@127\.0\.0\.1:\([0-9]*\)/pantry?sslmode=disable$#\1#p' "$SERVE_LOG" | head -1)"
  [ -n "$URL" ] && break
  kill -0 "$CLUSTER_PID" 2>/dev/null || break
  sleep 1
done
if [ -z "$URL" ]; then
  say "ABORT  ./tests/rls.sh --serve --empty never printed a URL."
  tail -20 "$SERVE_LOG" | sed 's/^/        /'
  exit 2
fi
PORT="$URL"
say "cluster: 127.0.0.1:$PORT (scratch, --serve --empty, removed on exit)"
say ""

PSQL=("$PGBIN/psql" -X -t -A -P pager=off -v ON_ERROR_STOP=0 -v VERBOSITY=verbose
      -U postgres -h 127.0.0.1 -p "$PORT" -d pantry)
q() { "${PSQL[@]}" -c "$1" 2>&1; }

A1=00000000-0000-4000-8000-0000000000a1   # alpha
A2=00000000-0000-4000-8000-0000000000a2   # bravo
FREE=00000000-0000-4000-8000-0000000000ff # a uuid nobody holds

# ---------------------------------------------------------------------------
# 2. the fixtures, byte-for-byte the ones tests/rls_checks.sh seeds, so a reader
#    is looking at the same rows every D-tier check is written against
# ---------------------------------------------------------------------------
q "set role pantry_admin;
insert into pantry.publishers (id, github_id, github_login, is_first_party, verified) values
  ('$A1', 101, 'alpha',       false, true),
  ('$A2', 202, 'bravo',       false, true),
  ('00000000-0000-4000-8000-0000000000a3', 303, 'cafaye',      true,  true),
  ('00000000-0000-4000-8000-0000000000a4', 404, 'unverified',  false, false);
insert into pantry.services (id, name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, publisher_id, ingested_by) values
  ('00000000-0000-4000-8000-0000000000b1', 'alpha-api',    'go',   'api', 'third_party', 'published', '^1.0.0', '{}'::jsonb, repeat('a',64), '$A1', 'probe'),
  ('00000000-0000-4000-8000-0000000000b2', 'alpha-draft',  'go',   'api', 'third_party', 'draft',     '^1.0.0', '{}'::jsonb, repeat('b',64), '$A1', 'probe'),
  ('00000000-0000-4000-8000-0000000000b3', 'bravo-api',    'rust', 'cli', 'third_party', 'published', '~0.2.0', '{}'::jsonb, repeat('c',64), '$A2', 'probe'),
  ('00000000-0000-4000-8000-0000000000b4', 'bravo-hidden', 'rust', 'cli', 'third_party', 'unlisted',  '~0.2.0', '{}'::jsonb, repeat('d',64), '$A2', 'probe'),
  ('00000000-0000-4000-8000-0000000000b5', 'cafaye',       'go',   'api', 'first_party', 'published', '>=0.1.0','{}'::jsonb, repeat('e',64), null,     'probe');
insert into pantry.service_versions (id, service_id, tag, version_major, version_minor, version_patch) values
  ('00000000-0000-4000-8000-0000000000c1', '00000000-0000-4000-8000-0000000000b1', 'v1.2.3', 1, 2, 3),
  ('00000000-0000-4000-8000-0000000000c2', '00000000-0000-4000-8000-0000000000b5', 'v0.1.0', 0, 1, 0);
insert into pantry.service_compat (service_id, target_id, kind, version_range, dependency) values
  ('00000000-0000-4000-8000-0000000000b1', '00000000-0000-4000-8000-0000000000b5', 'requires',       '^0.1.0', 'required'),
  ('00000000-0000-4000-8000-0000000000b1', '00000000-0000-4000-8000-0000000000b3', 'conflicts_with', '~1.0.0', 'soft'),
  ('00000000-0000-4000-8000-0000000000b5', '00000000-0000-4000-8000-0000000000b3', 'requires',       '~0.2.0', 'required'),
  ('00000000-0000-4000-8000-0000000000b1', '00000000-0000-4000-8000-0000000000b2', 'requires',       '^0.1.0', 'required'),
  ('00000000-0000-4000-8000-0000000000b2', '00000000-0000-4000-8000-0000000000b5', 'requires',       '^0.1.0', 'required');" \
  | grep -E '^(ERROR|FATAL)' && { say "ABORT  the fixtures did not land."; exit 2; }

# ---------------------------------------------------------------------------
# 3. helpers
# ---------------------------------------------------------------------------
# `q_pub` is one psql -c string, which PostgreSQL runs as ONE implicit
# transaction. That is not a detail: pantry.begin_publisher() sets the identity
# with set_config(..., true), which is transaction-local, so a statement that is
# not in the same transaction as the set reads no identity at all. tests/rls.sh's
# `run` does the same thing and its header says why.
q_pub() { "${PSQL[@]}" -c "set role pantry_publisher; select pantry.begin_publisher('$1'); $2" 2>&1 \
            | grep -Ev '^SET$|^ begin_publisher|^-+\(1 row\)|^-+$|^$|^BEGIN$|^COMMIT$|^ROLLBACK$'; }

# as_publisher <sql> — one line of verdict, with the SQLSTATE kept, because
# `violates row-level security policy` and `duplicate key value` are different
# worlds and collapsing them is the mistake this file exists to correct.
#
# The BEGIN/COMMIT/ROLLBACK filter is not decoration. Without it the first line of
# a `begin; update …; rollback;` probe is `BEGIN`, and the probe prints BEGIN as
# the verdict for a statement that was in fact refused — the same
# plausible-wrong-answer shape REPORT-pantry-rls-mutations-01.md §8 recorded.
verdict() { printf '    %-46s -> %s\n' "$1" "$(q_pub "$A1" "$2" | head -1 | sed 's/^ *//')"; }

# reset_alpha — WORLD 5 changes columns a later statement keys on, and a probe
# that silently reports UPDATE 0 because its own earlier statement moved the row
# is a probe reporting its own bookkeeping instead of the database.
reset_alpha() {
  q "update pantry.publishers set github_id=101, github_login='alpha', is_first_party=false where id='$A1';" >/dev/null
}

# policies <select-qual> <update-using> <update-check> — the whole experiment is
# one variable at a time, so the catalog is printed before every statement.
policies() {
  q "drop policy if exists publishers_publisher_select on pantry.publishers;
     drop policy if exists publishers_publisher_update on pantry.publishers;" >/dev/null
  if [ "$1" != "-" ]; then
    q "create policy publishers_publisher_select on pantry.publishers
        for select to pantry_publisher, pantry using ($1);" >/dev/null
  fi
  if [ "$2" != "-" ]; then
    q "create policy publishers_publisher_update on pantry.publishers
        for update to pantry_publisher, pantry using ($2) with check ($3);" >/dev/null
  fi
  printf '    catalog: '
  q "select coalesce(string_agg(polcmd::text||'  '||pg_get_expr(polqual,polrelid)
                               ||'  check='||coalesce(pg_get_expr(polwithcheck,polrelid),'-'), ' ;; ' order by polcmd), '(none)')
       from pg_policy where polrelid='pantry.publishers'::regclass
         and polroles && array[(select oid from pg_roles where rolname='pantry_publisher')];"
}

# ---------------------------------------------------------------------------
# 4. WORLD 0 — the untouched repository, and the packet's hypothesis already dead
# ---------------------------------------------------------------------------
rule
say "WORLD 0 — masters' policies, nothing altered. bravo owns two services, so a"
say "         foreign key CANNOT be what refuses its id move."
rule
policies "id = (select pantry.current_publisher_id())" \
         "id = (select pantry.current_publisher_id())" \
         "id = (select pantry.current_publisher_id())"
say "    (as bravo, who owns bravo-api and bravo-hidden)"
printf '    %-46s -> %s\n' "id -> a FREE uuid (no PK/FK collision)" \
  "$(q_pub "$A2" "update pantry.publishers set id='$FREE' where id='$A2';" | head -1 | sed 's/^ *//')"
say "    => 42501 row-level security, not a constraint. publishers_pkey is not the barrier."

# ---------------------------------------------------------------------------
# 5. WORLD 1 vs WORLD 2 — the single-variable experiment
# ---------------------------------------------------------------------------
rule
say "WORLD 1 — publishers_publisher_select USING (true), UPDATE USING (true) WITH CHECK (true)"
say "          Every publisher expression on the table is the literal true. The"
say "          policy cannot refuse anything. What is left?"
rule
policies "true" "true" "true"
verdict "own row, non-key column"    "update pantry.publishers set github_login='w1' where github_id=101;"
verdict "verified = false"           "update pantry.publishers set verified=false where github_id=101;"
verdict "id -> a FREE uuid"          "update pantry.publishers set id='$FREE' where github_id=101;"
verdict "id -> bravo (collides)"     "update pantry.publishers set id='$A2' where github_id=101;"
say "    => CONSTRAINTS, and two of them, in two different directions. Both are"
say "       downstream of the policy: neither is reachable until the policy is opened."

rule
say "WORLD 2 — IDENTICAL UPDATE policy (USING (true) WITH CHECK (true)). The only"
say "          change is publishers_publisher_select's USING, back to master's."
rule
policies "id = (select pantry.current_publisher_id())" "true" "true"
verdict "own row, non-key column"    "update pantry.publishers set github_login='w2' where github_id=101;"
verdict "verified = false"           "update pantry.publishers set verified=false where github_id=101;"
verdict "id -> a FREE uuid"          "update pantry.publishers set id='$FREE' where github_id=101;"
verdict "id -> bravo (collides)"     "update pantry.publishers set id='$A2' where github_id=101;"
say "    => POLICY. Same UPDATE policy, same statement, 42501. The FOR SELECT policy is"
say "       the barrier, and the UPDATE policy's own WITH CHECK (true) is not."

# ---------------------------------------------------------------------------
# 6. the same mechanism the other way round: the SELECT policy also FILTERS
# ---------------------------------------------------------------------------
rule
say "WORLD 3 — no SELECT policy at all, UPDATE USING (true) WITH CHECK (true)."
say "          If only the UPDATE policy gated the rows, this would update."
rule
policies "-" "true" "true"
verdict "own row, non-key column"    "update pantry.publishers set github_login='w3' where github_id=101;"
verdict "id -> a FREE uuid"          "update pantry.publishers set id='$FREE' where github_id=101;"
say "    the plan the planner built, which is the whole claim in one object:"
q_pub "$A1" "explain (verbose, costs off) update pantry.publishers set github_login='w3' where github_id=101;" \
  | sed 's/^/      /'
say "    => One-Time Filter: false. Postgres folded the scan away: no rows are"
say "       updatable without a SELECT policy, because the statement reads a column."

# ---------------------------------------------------------------------------
# 7. the documented condition: no column read, no SELECT policy, no barrier
# ---------------------------------------------------------------------------
rule
say "WORLD 4 — SELECT USING (id = current), UPDATE USING (true) WITH CHECK (true),"
say "          and a statement that reads NO column of pantry.publishers."
say "          CREATE POLICY footnote [a]: the SELECT policy applies 'if read access"
say "          is required to either the existing or new row'."
rule
policies "id = (select pantry.current_publisher_id())" "true" "true"
verdict "id -> FREE, WHERE github_id = 101 (reads)"   "update pantry.publishers set id='$FREE' where github_id=101;"
verdict "id -> FREE, no WHERE at all (reads nothing)" "begin; update pantry.publishers set id='$FREE'; rollback;"
say "      the plan for the no-WHERE form — note there is no filter on the scan:"
q_pub "$A1" "explain (verbose, costs off) update pantry.publishers set id='$FREE';" | sed 's/^/      /'
say "    => POLICY when the statement reads a column; CONSTRAINT when it does not."
say "       The barrier is a property of the STATEMENT, not only of the schema."

# ---------------------------------------------------------------------------
# 8. the migration's own claim about github_id / github_login, on an untouched tree
# ---------------------------------------------------------------------------
rule
say "WORLD 5 — masters' policies, untouched. migrations/00006_rls.sql:104 says the"
say "          WITH CHECK 'is what stops a publisher from moving a row OUT of its own"
say "          account by updating github_id/github_login'."
rule
policies "id = (select pantry.current_publisher_id())" \
         "id = (select pantry.current_publisher_id())" \
         "id = (select pantry.current_publisher_id())"
reset_alpha
verdict "github_login -> 'alpha-renamed' (free)"  "update pantry.publishers set github_login='alpha-renamed' where id='$A1';"; reset_alpha
verdict "github_id -> 999 (free, unclaimed)"      "update pantry.publishers set github_id=999 where id='$A1';";         reset_alpha
verdict "github_id -> 202 (bravo's, collides)"    "update pantry.publishers set github_id=202 where id='$A1';";         reset_alpha
verdict "is_first_party -> true"                  "update pantry.publishers set is_first_party=true where id='$A1';"; reset_alpha
say "    => three of four are PERMITTED. The fourth is a UNIQUE CONSTRAINT (23505"
say "       publishers_github_id_key), not a policy. The comment names a barrier that"
say "       is not on this table."

# ---------------------------------------------------------------------------
# 9. the tree is the tree
# ---------------------------------------------------------------------------
rule
say "migrations/ and tests/ — untouched by this file, by construction:"
git diff --quiet master -- migrations/ tests/ \
  && say "  git diff --quiet master -- migrations/ tests/  ->  CLEAN" \
  || say "  DIRTY (this probe does not write to the repository; investigate)"
[ -z "$(git status --porcelain -- migrations/ tests/)" ] \
  && say "  git status --porcelain -- migrations/ tests/    ->  empty"
rule
if [ "$KEEP" -eq 1 ]; then
  say "cluster left running on 127.0.0.1:$PORT (--keep). Ctrl-C or kill $CLUSTER_PID."
else
  say "cluster on 127.0.0.1:$PORT removed on exit."
fi