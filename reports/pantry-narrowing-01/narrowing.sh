#!/usr/bin/env bash
#
# reports/pantry-narrowing-01/narrowing.sh — can pantry's RLS suite be made to
# fail by making a policy TOO STRICT, so a legitimate publisher is refused?
#
# WHY A SIBLING AND NOT AN EXTENSION. REPORT-pantry-rls-mutations-01.md is merged,
# cited and has tallies in it: 12 controls, 10 breakages, 106 red rows, 0 POLICY.
# Appending Part 3 to that file would change what those numbers mean, and a
# reader holding the old report would find a script that no longer produces them.
# The two recipes also ask opposite questions and their verdicts are mirrored:
#
#   WIDENING  the boundary moved outward, so the correct result is that NOTHING
#             refused. The strong verdicts are ABSENT-ALLOWED and ABSENT-WIDENED.
#   NARROWING the boundary moved inward, so the correct result is that a
#             LEGITIMATE operation was refused. The strong verdicts are POLICY
#             and ABSENT-NARROWED.
#
# A recipe that reported both from one table would need a "which direction was
# this" column, and that column is exactly where a mutation recipe starts
# reporting its own expectation back to itself. Two files, each with one
# direction, each with its own tallies. The machinery is duplicated deliberately:
# ~180 lines that could be sourced out of mutations.sh is cheaper than two
# recipes whose verdicts share a variable.
#
# It lives in `reports/`, not `tests/`, so it is inert to every tier by
# construction — `bin/prime` counts test binaries with
# `find tests -maxdepth 1 -name '*.rs'` and nothing here is in `tests/`.
#
#   ./reports/pantry-narrowing-01/narrowing.sh              # control + part 1
#   ./reports/pantry-narrowing-01/narrowing.sh --phase control
#   ./reports/pantry-narrowing-01/narrowing.sh --phase part1
#   ./reports/pantry-narrowing-01/narrowing.sh --list
#
# ---------------------------------------------------------------------------
# WHAT A RED HAS TO BE, OR THE RESULT IS NOT THE POINT
# ---------------------------------------------------------------------------
# Every breakage below must go red on a check that asserts a LEGITIMATE OPERATION
# WORKS — a rowset that must be non-empty, an `ok` that must see `UPDATE 1`, a
# `deny`-shaped check whose sibling must still succeed. It must NOT go red on a
# check that asserts a refusal, and it must NOT go red only outside the D tier.
#
# That is the whole difference between this recipe and the last one, and it is
# why `observe` records the polarity of every red rather than just its count. A
# narrowing that turns `D9` (may NOT insert first_party) red has proved nothing:
# `D9` asserting a refusal cannot be falsified by refusing more. A narrowing that
# turns `D8` (the row it just inserted is readable by its owner) red has proved
# the suite catches over-refusal, which is the entire claim.
set -uo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)" || exit 2
# `cd … || exit`, not a bare `cd`. A recipe that silently stayed in the directory
# it was invoked from would plant its mutations in the wrong tree, and the first
# thing it does afterwards is assert the tree is clean — which it would be, in
# the wrong repository.
cd "$REPO" || exit 2

WORK="$(mktemp -d "${TMPDIR:-/tmp}/pantry-narrowing.XXXXXX")"
SUITE="$REPO/tests/rls.sh"
SUMMARY="$WORK/summary.tsv"
: >"$SUMMARY"

SUBJECT_DIRS=(migrations tests)

CONTROLS_RUN=0
CONTROLS_GREEN=0
PLANTED=0
PLANT_FAILED=0
RED_LEGIT=0
NOT_RED_LEGIT=0
RED_WRONG_POLARITY=0
RESTORED=0
RESTORE_FAILED=0
PROBLEMS=0

PHASE="all"
case "${1:-}" in
  --phase) PHASE="${2:-all}" ;;
  --phase=*) PHASE="${1#--phase=}" ;;
  --list) PHASE="list" ;;
  "") ;;
  *) echo "narrowing.sh: unknown argument '$1'. --phase control|part1|all | --list" >&2; exit 2 ;;
esac

say()   { printf '%s\n' "$*"; }
rule()  { say "----------------------------------------------------------------"; }
head2() { say ""; say "== $*"; }

# ---------------------------------------------------------------------------
# replace_once <label> <file> <old> <new>   — exact, and exactly once
# ---------------------------------------------------------------------------
# The count assertion is the whole point. A `sed` that matched nothing produces a
# green suite, and a green suite produced by a mutation that never applied is
# worse than no run at all: it reads as a pass.
replace_once() {
  local label="$1" file="$2" old="$3" new="$4" rc
  LBL="$label" FILE="$file" OLD="$old" NEW="$new" python3 - <<'PY'
import os, sys
lbl, path = os.environ["LBL"], os.environ["FILE"]
old, new = os.environ["OLD"], os.environ["NEW"]
src = open(path, encoding="utf-8").read()
c = src.count(old)
if c != 1:
    sys.stderr.write("  matched %d time(s) in %s; needed exactly 1\n" % (c, path))
    sys.exit(3)
open(path, "w", encoding="utf-8").write(src.replace(old, new, 1))
PY
  rc=$?
  [ $rc -eq 0 ] || say "ABORT  $label: the replacement did not apply exactly once (exit $rc)."
  return $rc
}

# grep_confirm <file> <fixed-string> <expected-count> <label> [note]
# Run AFTER the write and BEFORE the suite, so the transcript shows the mutation
# present in the file the suite is about to read. An expected count of 0 is a
# legitimate negative confirmation: "the clause is gone, and the lines around it
# are not".
grep_confirm() {
  local file="$1" needle="$2" want="$3" label="$4" note="${5:-}" got
  got=$(grep -Fc -- "$needle" "$file" 2>/dev/null || true)
  got=${got:-0}
  if [ "$got" != "$want" ]; then
    say "ABORT  $label: expected $want line(s) matching [$needle] in ${file#$REPO/}, found $got"
    PROBLEMS=$((PROBLEMS+1))
    return 1
  fi
  say "        grep-confirmed: $got line(s) match [$needle] in ${file#$REPO/}$note"
  return 0
}

# block_confirm <file> <multi-line text> <want> <label> [note]
# `grep -c` counts LINES, so a multi-line needle never matches one and a
# multi-line confirmation built on it silently confirms nothing. This is the
# literal-containment form, read from the file fresh off disk.
block_confirm() {
  local file="$1" text="$2" want="$3" label="$4" note="${5:-}" got
  got=$(TEXT="$text" FILE="$file" python3 -c '
import os
print(open(os.environ["FILE"], encoding="utf-8").read().count(os.environ["TEXT"]))')
  if [ "$got" != "$want" ]; then
    say "ABORT  $label: expected $want occurrence(s) of the mutated block in ${file#$REPO/}, found $got"
    PROBLEMS=$((PROBLEMS+1))
    return 1
  fi
  say "        grep-confirmed: $got occurrence(s) of the mutated block in ${file#$REPO/}$note"
  return 0
}

tree_clean() {
  local d
  for d in "${SUBJECT_DIRS[@]}"; do
    git diff --quiet -- "$d" || return 1
    git diff --quiet master -- "$d" || return 1
    [ -z "$(git status --porcelain -- "$d")" ] || return 1
  done
  return 0
}

restore() {
  local label="$1"
  git checkout master -- "${SUBJECT_DIRS[@]}" 2>/dev/null
  if tree_clean; then
    RESTORED=$((RESTORED+1))
    say "        reverted from master; ${SUBJECT_DIRS[*]} byte-identical (git diff --quiet)"
    return 0
  fi
  RESTORE_FAILED=$((RESTORE_FAILED+1))
  say "ABORT  $label: the tree is still dirty after the revert:"
  git --no-pager diff --stat master -- "${SUBJECT_DIRS[@]}" | sed 's/^/        /'
  PROBLEMS=$((PROBLEMS+1))
  return 1
}

# ---------------------------------------------------------------------------
# run_suite <label> — one ./tests/rls.sh, with its results TSV kept and parsed
# ---------------------------------------------------------------------------
RUN_LOG=""; RUN_RC=0; RESULTS=""; RUN_TOTAL=0; RUN_PASS=0; RUN_FAIL=0; RUN_SKIP=0
run_suite() {
  local label="$1" kept line
  RUN_LOG="$WORK/$label.log"
  timeout 600 "$SUITE" --keep >"$RUN_LOG" 2>&1
  RUN_RC=$?
  kept=$(sed -n 's/^kept: //p' "$RUN_LOG" | head -1)
  RESULTS=""
  line=$(sed -n 's/^checks: \([0-9]*\) run, \([0-9]*\) passed, \([0-9]*\) failed, \([0-9]*\) skipped.*/\1 \2 \3 \4/p' "$RUN_LOG" | head -1)
  if [ -z "$line" ]; then
    say "ABORT  $label: tests/rls.sh printed no 'checks:' line (exit $RUN_RC)."
    say "        A suite that never ran is not a green suite and is not a red one."
    tail -20 "$RUN_LOG" | sed 's/^/        /'
    PROBLEMS=$((PROBLEMS+1))
    RUN_TOTAL=0; RUN_PASS=0; RUN_FAIL=0; RUN_SKIP=0
    return 1
  fi
  # shellcheck disable=SC2086
  set -- $line
  RUN_TOTAL=$1; RUN_PASS=$2; RUN_FAIL=$3; RUN_SKIP=$4
  say "        tests/rls.sh -> $RUN_TOTAL run, $RUN_PASS passed, $RUN_FAIL failed, $RUN_SKIP skipped (exit $RUN_RC)"
  # The TSV is COPIED OUT before the cluster's workdir goes. Reading it after
  # `rm -rf` classifies an empty string as "nothing failed", which is how a run
  # with 8 reds once printed "NOT RED AT ALL" on its way to calling a widening
  # undetectable. Every verdict below comes from this copy.
  if [ -n "$kept" ] && [ -f "$kept/results.tsv" ]; then
    cp "$kept/results.tsv" "$WORK/$label.results.tsv"
    RESULTS="$WORK/$label.results.tsv"
  else
    say "ABORT  $label: tests/rls.sh kept no results.tsv, so no red can be named."
    PROBLEMS=$((PROBLEMS+1))
    [ -n "$kept" ] && rm -rf "$kept"
    return 1
  fi
  rm -rf "$kept"
  return 0
}

# ---------------------------------------------------------------------------
# classify — one verdict per failed check, out of results.tsv
# ---------------------------------------------------------------------------
# Same taxonomy as mutations.sh, and the same rule about where it comes from: the
# server's own words first, then the expectation the runner recorded, and never
# the check's name and never what the mutation was trying to do.
classify() {
  [ -n "${RESULTS:-}" ] && [ -f "$RESULTS" ] || return 0
  RESULTS_FILE="$RESULTS" python3 - <<'PY'
import os

CATALOG_PREFIXES = ("A5", "C13", "C15", "G1", "G2", "G3", "G4", "G5",
                    "I1", "I3", "I5", "I6", "J1", "J2", "J5", "F1")

def csv_len(csv):
    return [x for x in csv.split(",") if x.strip()]

for line in open(os.environ["RESULTS_FILE"], encoding="utf-8"):
    parts = line.rstrip("\n").split("\t")
    while len(parts) < 4:
        parts.append("")
    name, status, exp, obs = parts[0], parts[1], parts[2], parts[3]
    if status != "FAIL":
        continue

    # The server's own words first: if it named a barrier, that IS the barrier.
    if "row-level security" in obs:
        v = "POLICY"
    elif "permission denied for table" in obs or "permission denied for schema" in obs:
        v = "GRANT"
    elif "violates foreign key constraint" in obs or "violates check constraint" in obs \
         or "violates unique constraint" in obs or "violates not-null constraint" in obs \
         or "duplicate key value" in obs:
        v = "CONSTRAINT"
    elif exp.startswith("statement SUCCEEDED"):
        # For a NARROWING this is the interesting verdict: a refusal the check
        # required was permitted. It happens when a policy is narrowed so hard the
        # row stops being visible to a grant-only check.
        v = "ABSENT-ALLOWED"
    elif exp.startswith("wrong sqlstate"):
        v = "OTHER-SQLSTATE"
    elif exp.startswith("refused by the wrong barrier"):
        v = "OTHER-BARRIER"
    elif exp.startswith("wanted: "):
        want = csv_len(exp[len("wanted: "):])
        got = obs[len("got: "):] if obs.startswith("got: ") else obs
        got = csv_len(got)
        if len(got) > len(want):
            v = "ABSENT-WIDENED"
        elif len(got) < len(want):
            # THE narrowing verdict: a read that must return rows returned fewer.
            v = "ABSENT-NARROWED"
        else:
            v = "ABSENT-CONTENT"
    elif exp.startswith("missing: "):
        v = "ABSENT-OBSERVATION"
    elif any(name.startswith(p) or name.split(" ", 1)[0] == p for p in CATALOG_PREFIXES):
        v = "CATALOG"
    else:
        v = "OTHER"
    print("\t".join((v, name, exp, obs)))
PY
}

# ---------------------------------------------------------------------------
# polarity <exp>
# ---------------------------------------------------------------------------
# WHICH KIND OF CHECK went red, read off what tests/rls.sh RECORDED rather than
# off the check's name. The three helpers record three different things and the
# difference between them is the whole argument of this file:
#
#   rowset  records  exp="wanted: <csv>"   obs="got: <csv>"
#     A NON-EMPTY expectation is a read that MUST return rows — a legitimate
#     operation that has to keep working. An EMPTY expectation (`wanted: ` and
#     nothing after it) is `D5`, "another publisher's row is absent": it asserts
#     an ABSENCE, and a refusal can never falsify it. Both are `rowset`.
#   ok      records  exp="<the fragment the output had to contain>"
#     Always an assertion that something HAPPENED — `INSERT 0 1`, `UPDATE 1`,
#     a name, a count. A legitimate operation.
#   deny    records  exp="<the SQLSTATE it demanded>", and on failure one of
#     `statement SUCCEEDED` / `wrong sqlstate, wanted …` / `refused by the wrong
#     barrier, wanted '…'`. That is a check asserting a REFUSAL.
#
# The first version of this graded polarity by looking for `insert into` /
# `update ` / `delete from` / `select ` inside the expectation, which works for
# `ok` and `deny` and fails completely for `rowset`: D2's expectation is the
# literal `wanted: alpha`, no verb anywhere in it, so the one genuinely positive
# red in this whole recipe was graded as a refusal-shaped check. A classifier
# that gets the polarity backwards would have reported this packet's only
# positive result as a null one.
polarity() {
  local e="$1"
  case "$e" in
    "wanted: "*)
      if [ -n "${e#wanted: }" ]; then printf 'LEGIT'; else printf 'refusal'; fi
      return 0 ;;
    "got: "*)            printf 'LEGIT'; return 0 ;;
    "missing: "*)        e="${e#missing: }" ;;
    "exit="*)            printf 'harness'; return 0 ;;
    "statement SUCCEEDED" | "wrong sqlstate"* | "refused by the wrong barrier"*)
                          printf 'refusal'; return 0 ;;
  esac
  # `deny` records the bare SQLSTATE it demanded — always five characters with a
  # leading digit — and nothing else in this suite records an expectation that
  # looks like one: `ok` fragments are statement text or output, and the
  # `cafaye` row of expectations is six characters.
  if printf '%s' "$e" | grep -qE '^[0-9][0-9A-Za-z]{4}$'; then printf 'refusal'; else printf 'LEGIT'; fi
}

tier() { printf '%s' "$1" | sed -E 's/^([A-J])[0-9].*/\1/'; }

# ---------------------------------------------------------------------------
# witness <id> <statement-label> <statement>
# ---------------------------------------------------------------------------
# WHY THIS EXISTS, AND IT IS THE MOST IMPORTANT FUNCTION IN THE FILE.
#
# A narrowing that does not go red is only a finding about the SUITE if the
# narrowing was real. "Every check passed" is also what a mutation that changed
# nothing at runtime prints, and the packet is explicit that a mutation which
# cannot confirm itself should not run. A grep confirmation says the bytes are in
# the file; it does not say a publisher was refused.
#
# So for every breakage the recipe re-stands-up the SAME cluster `tests/rls.sh`
# builds — `./tests/rls.sh --serve --empty`, which applies `migrations/` off the
# disk, with the mutation in place. No second mechanism: there is no `alter
# policy` here and no hand-written fixture that could drift from the file. If the
# file says the publisher is excluded, the cluster the suite would have built
# excludes it, and this prints the database's own answer.
#
# The baseline is printed first, from the unmutated tree, so every witness is a
# comparison rather than an assertion.
W_PGBIN=""
W_PSQL=()
w_resolve_pg_bin() {
  local c
  for c in "${PANTRY_PG_BIN:-}" /opt/homebrew/opt/postgresql@18/bin \
           /opt/homebrew/opt/postgresql@17/bin /usr/local/opt/postgresql@18/bin \
           /usr/lib/postgresql/18/bin /usr/lib/postgresql/17/bin; do
    [ -n "$c" ] && [ -x "$c/psql" ] && { printf '%s' "$c"; return 0; }
  done
  command -v psql >/dev/null 2>&1 && { dirname "$(command -v psql)"; return 0; }
  return 1
}
W_ALPHA=00000000-0000-4000-8000-0000000000a1
W_SEEDED=0

# w_serve — start a cluster on the CURRENT migrations, seed the suite's fixtures,
# and leave PSQL pointed at it. Prints the URL it is serving on.
w_serve() {
  W_PGBIN="$(w_resolve_pg_bin)" || return 1
  local log port pid i
  log="$(mktemp "${TMPDIR:-/tmp}/pantry-narrow-witness.XXXXXX")"
  # NOT under `timeout`, and that is load-bearing rather than stylistic. `timeout
  # 300 ./tests/rls.sh … &` makes $! the PID of `timeout`, whose child is the
  # harness; killing $! kills the wrapper, the harness never sees a signal, its
  # EXIT trap never runs, and the scratch cluster — postmaster, socket and data
  # directory — survives every single breakage. Four narrowings, four leaked
  # clusters, found by looking for leftover processes rather than by reading the
  # code. Running the harness directly means $! IS the process whose `trap 'exit
  # 0' INT TERM` stops the server and removes the data dir. w_stop then kills it
  # by PID, waits, and escalates.
  "$REPO/tests/rls.sh" --serve --empty >"$log" 2>&1 &
  pid=$!
  W_WATCH_PID="$pid"; W_WATCH_LOG="$log"
  port=""
  for i in $(seq 1 60); do
    port="$(sed -n 's#^postgres://postgres@127\.0\.0\.1:\([0-9]*\)/pantry?sslmode=disable$#\1#p' "$log" | head -1)"
    [ -n "$port" ] && break
    kill -0 "$pid" 2>/dev/null || break
    sleep 1
  done
  [ -n "$port" ] || return 1
  W_PSQL=("$W_PGBIN/psql" -X -t -A -P pager=off -v ON_ERROR_STOP=0 -v VERBOSITY=verbose
          -U postgres -h 127.0.0.1 -p "$port" -d pantry)
  # The same four fixture rows tests/rls_checks.sh seeds, and only those four are
  # needed: every witness below is a read of, or a write to, publisher alpha's own
  # rows. Nothing here asserts a count that the full suite would also assert.
  "${W_PSQL[@]}" -c "set role pantry_admin;
    insert into pantry.publishers (id, github_id, github_login, is_first_party, verified) values
      ('$W_ALPHA', 101, 'alpha',      false, true),
      ('00000000-0000-4000-8000-0000000000a2', 202, 'bravo',      false, true),
      ('00000000-0000-4000-8000-0000000000a3', 303, 'cafaye',     true,  true),
      ('00000000-0000-4000-8000-0000000000a4', 404, 'unverified', false, false);
    insert into pantry.services (id, name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, publisher_id, ingested_by) values
      ('00000000-0000-4000-8000-0000000000b1', 'alpha-api',   'go',   'api', 'third_party', 'published', '^1.0.0', '{}'::jsonb, repeat('a',64), '$W_ALPHA', 'witness'),
      ('00000000-0000-4000-8000-0000000000b2', 'alpha-draft', 'go',   'api', 'third_party', 'draft',     '^1.0.0', '{}'::jsonb, repeat('b',64), '$W_ALPHA', 'witness'),
      ('00000000-0000-4000-8000-0000000000b3', 'bravo-api',   'rust', 'cli', 'third_party', 'published', '~0.2.0', '{}'::jsonb, repeat('c',64), '00000000-0000-4000-8000-0000000000a2', 'witness'),
      ('00000000-0000-4000-8000-0000000000b4', 'bravo-hidden','rust', 'cli', 'third_party', 'unlisted',  '~0.2.0', '{}'::jsonb, repeat('d',64), '00000000-0000-4000-8000-0000000000a2', 'witness'),
      ('00000000-0000-4000-8000-0000000000b5', 'cafaye',      'go',   'api', 'first_party', 'published', '>=0.1.0','{}'::jsonb, repeat('e',64), null, 'witness');" >/dev/null 2>&1
  return 0
}

w_stop() {
  # Belt and braces, in this order: ask the harness to stop, wait for it, then
  # stop the cluster it was serving BY ITS OWN DATA DIRECTORY and remove that
  # directory. The last two steps are what make "no cluster was left behind"
  # true even if the harness is wedged, which is the failure a witness that runs
  # once per breakage can least afford — a leaked postmaster holds a port and a
  # few hundred megabytes for every breakage in the recipe.
  local datadir
  datadir="$(sed -n 's#^cluster:  *\(/var/[^ ]*\)/data .*#\1#p' "${W_WATCH_LOG:-/dev/null}" 2>/dev/null | head -1)"
  if [ -n "${W_WATCH_PID:-}" ] && kill -0 "$W_WATCH_PID" 2>/dev/null; then
    kill "$W_WATCH_PID" 2>/dev/null
    for _ in 1 2 3 4 5 6 7 8 9 10; do kill -0 "$W_WATCH_PID" 2>/dev/null || break; sleep 1; done
    kill -9 "$W_WATCH_PID" 2>/dev/null
  fi
  if [ -n "$datadir" ] && [ -d "$datadir/data" ]; then
    "$W_PGBIN/pg_ctl" -D "$datadir/data" stop -m immediate >/dev/null 2>&1
    rm -rf "$datadir"
  fi
  [ -n "${W_WATCH_LOG:-}" ] && rm -f "$W_WATCH_LOG"
  W_WATCH_PID=""; W_WATCH_LOG=""; W_PSQL=()
  return 0
}

# w_try <label> <sql> — one statement, as publisher alpha, in ONE implicit
# transaction so begin_publisher's transaction-local identity survives to it.
w_try() {
  local o
  o=$("${W_PSQL[@]}" -c "set role pantry_publisher; select pantry.begin_publisher('$W_ALPHA'); $2" 2>&1 \
      | grep -Ev '^SET$|^ begin_publisher|^-+\(1 row\)|^-+$|^$|^BEGIN$|^COMMIT$|^ROLLBACK$' | tr '\n' ' ')
  o="${o# }"; o="${o% }"; o="$(printf '%s' "$o" | sed 's/  */ /g')"
  printf '    %-44s -> %s\n' "$1" "${o:-(no rows)}"
}

w_baseline() {
  say "== WITNESS BASELINE — master, untouched. Every witness below is a diff"
  say "   against this, because 'every check passed' is also what a mutation that"
  say "   changed nothing at runtime prints."
  w_serve || { say "ABORT  the witness cluster would not start."; return 1; }
  w_try "D2  read its own publisher row"   "select github_login from pantry.publishers order by 1;"
  w_try "D7  insert its own service"       "insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, publisher_id) values ('alpha-new','go','api','third_party','published','^1.0.0','{}'::jsonb, repeat('1',64), '$W_ALPHA');"
  w_try "its own publisher row, UPDATE"    "update pantry.publishers set verified=false where id='$W_ALPHA';"
  w_stop
  return 0
}

w_witness() {
  local label="$1"
  say "        -- WITNESS: the same statement on a cluster built from the"
  say "           MUTATED migrations/. This is the publisher the suite just failed"
  say "           to notice, or the proof there was nothing to notice."
  w_serve || { say "        ABORT  the witness cluster would not start; this breakage's"
                say "               result is unverified and must not be read as one."; PROBLEMS=$((PROBLEMS+1)); return 1; }
  w_try "D2  read its own publisher row"   "select github_login from pantry.publishers order by 1;"
  w_try "D7  insert its own service"       "insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, publisher_id) values ('alpha-new','go','api','third_party','published','^1.0.0','{}'::jsonb, repeat('1',64), '$W_ALPHA');"
  w_try "its own publisher row, UPDATE"    "update pantry.publishers set verified=false where id='$W_ALPHA';"
  w_stop
  return 0
}
trap 'w_stop' EXIT INT TERM

# ok_field <exp> <obs> — RETIRED, and kept only so its replacement's comment has
# somewhere to point. See `polarity` below, which reads the recorded expectation
# rather than guessing from it.
ok_field() { printf '%s\n' "$1" | tr '[:upper:]' '[:lower:]' | grep -c "$2" 2>/dev/null || true; }

# ---------------------------------------------------------------------------
# observe <label> <mechanical-expectation>
# ---------------------------------------------------------------------------
# Three outcomes, and they are never added together:
#
#   RED-LEGITIMATE      at least one red on a check asserting a legitimate
#                       operation WORKS. This is the result this recipe exists
#                       for: the suite caught an over-refusal.
#   RED-WRONG-POLARITY  every red is on a check asserting a REFUSAL. Nothing was
#                       proved, because a check that wants a refusal cannot be
#                       falsified by refusing more.
#   NOT-RED             nothing went red at all.
#
# The verdict is derived from what the runner RECORDED, never from the breakage's
# name, so a narrowing cannot be graded by the author's intent.
OBS_D=0; OBS_NAMES=""; OBS_NOTE=""; OBS_LEGIT=""; OBS_WRONG=""
observe() {
  local label="$1" predicted="$2" out v n exp obs lname
  OBS_D=0; OBS_NAMES=""; OBS_NOTE=""; OBS_LEGIT=""; OBS_WRONG=""
  out=$(classify)
  if [ -z "$out" ]; then
    say "        NOT RED AT ALL: every check passed with the breakage planted."
    say "        *** THE MOST VALUABLE RESULT IN THIS RECIPE *** The suite cannot"
    say "        *** detect this narrowing, so it cannot detect an over-refusal. ***"
    NOT_RED_LEGIT=$((NOT_RED_LEGIT+1))
    OBS_NAMES="(none)"; OBS_NOTE="NOT-RED"
    printf '%s\t%s\t%s\t0\t-\t(none) — every check passed\n' "$label" "$OBS_NOTE" "$predicted" >>"$SUMMARY"
    return 0
  fi
  OBS_NAMES=$(printf '%s\n' "$out" | cut -f2 | paste -sd, -)
  while IFS=$'\t' read -r v n exp obs; do
    lname="${n%% *}"
    case "$(polarity "$exp")" in
      LEGIT)   OBS_LEGIT="$OBS_LEGIT $lname" ;;
      refusal) OBS_WRONG="$OBS_WRONG $lname" ;;
      *)       : ;;   # the harness itself failed; not evidence about a policy
    esac
    [ "$(tier "$n")" = "D" ] && OBS_D=$((OBS_D+1))
    say "        $(printf '%-18s %-8s %s' "$v" "$(polarity "$exp")" "$n")"
  done <<<"$out"
  if [ -n "$OBS_LEGIT" ]; then
    RED_LEGIT=$((RED_LEGIT+1)); OBS_NOTE="RED-LEGITIMATE"
    say "        -> RED on a check asserting a LEGITIMATE operation works:$OBS_LEGIT"
    say "        -> $OBS_D of $(printf '%s\n' "$out" | wc -l | tr -d ' ') reds are in the D tier"
    [ -n "$OBS_WRONG" ] && say "        -> also red on refusal-shaped checks:$OBS_WRONG (not evidence either way)"
  else
    RED_WRONG_POLARITY=$((RED_WRONG_POLARITY+1)); OBS_NOTE="RED-WRONG-POLARITY"
    say "        -> RED, but every red is on a check asserting a REFUSAL:$OBS_WRONG"
    say "        -> NOTHING PROVED: a check that wants a refusal cannot be falsified"
    say "           by refusing more. This is the shape a widening breakage produces."
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$label" "$OBS_NOTE" "$predicted" "$OBS_D" "${OBS_LEGIT:-none}" "$OBS_NAMES" >>"$SUMMARY"
  return 0
}

# ---------------------------------------------------------------------------
# THE THREE NARROWINGS
# ---------------------------------------------------------------------------
# Two are on `publishers` because that is the table Part 1 found with nothing
# asserting its write path, and one is on `services` because the D tier's write
# checks sit there and a narrowing there has to clear its own read-only policy
# before it can reach anything.
MIG="$REPO/migrations"
R6="$MIG/00006_rls.sql"

# Fixture `alpha`, so a stubbed identity is indistinguishable from a real alpha
# session — which is the point: the boundary has to depend on something other than
# the value the function returns.
ALPHA_ID="00000000-0000-4000-8000-0000000000a1"

# N1 — tighten a policy with something the publisher LEGITIMATELY satisfies.
n1_tighten_select_with_verified() {
  # `publishers_publisher_select`'s USING gains `and verified`. Alpha is
  # verified in the fixtures, so a publisher's read of its own row still works and
  # every D-tier read check must stay green. The tightening is real — a publisher
  # whose row is not verified now reads nothing — and it is invisible to the
  # suite, because nothing in the suite sets `verified = false` from the
  # publisher's side.
  # WRITTEN FOR: D2, and every D-tier read check.
  replace_once "N1" "$R6" \
'create policy publishers_publisher_select
  on pantry.publishers
  for select
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id()));
' \
'create policy publishers_publisher_select
  on pantry.publishers
  for select
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id()) and verified);
' || return 1
  block_confirm "$R6" '  using (id = (select pantry.current_publisher_id()) and verified);
' 1 "N1" \
  " — the SELECT policy now also requires verified" || return 1
  block_confirm "$R6" '  using (id = (select pantry.current_publisher_id()));
' 0 "N1" \
  " — master's SELECT predicate text is gone from the file" || return 1
  say "        N1: publishers_publisher_select tightened with a column the publisher"
  say "            legitimately satisfies (verified = true in every fixture)"
}

# N2 — tighten it with something it does NOT.
n2_tighten_select_with_account_age() {
  # The same policy, tightened with `and claimed_at < now() - interval '1 year'`.
  # It reads like a real anti-abuse rule — "a brand new account cannot read its
  # own row" — and every fixture row was seeded seconds ago, so alpha does not
  # satisfy it and a legitimate publisher with a valid account reads NOTHING.
  #
  # THIS CLAUSE IS HERE BECAUSE THE FIRST VERSION OF IT WAS A RUNTIME NO-OP AND
  # THE WITNESS CAUGHT IT. The original was
  #     and exists (select 1 from pantry.services where publisher_id = …)
  # on the reasoning that "publishers are accounts, not services, so none of them
  # has a row in pantry.services". That reasoning was about the wrong table: alpha
  # owns `alpha-api` and `alpha-draft`, the clause was satisfied, and the witness
  # printed `D2 read its own publisher row -> alpha` while the suite said green —
  # the exact reading this recipe exists to refuse. A narrowing has to be MEASURED
  # to be a narrowing.
  # WRITTEN FOR: D2, and every D-tier read check.
  replace_once "N2" "$R6" \
'create policy publishers_publisher_select
  on pantry.publishers
  for select
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id()));
' \
'create policy publishers_publisher_select
  on pantry.publishers
  for select
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id())
         and claimed_at < now() - interval '"'"'1 year'"'"');
' || return 1
  block_confirm "$R6" "create policy publishers_publisher_select
  on pantry.publishers
  for select
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id())
         and claimed_at < now() - interval '1 year');
" 1 "N2" \
  " — the whole block is present and no seeded row can satisfy the new clause" || return 1
  block_confirm "$R6" '  using (id = (select pantry.current_publisher_id()));
' 0 "N2" \
  " — master's SELECT predicate text is gone from the file" || return 1
  say "        N2: publishers_publisher_select tightened with a clause NO fixture"
  say "            publisher satisfies (every fixture row was seeded seconds ago)"
}

# N3 — a spurious WITH CHECK on an INSERT policy.
n3_spurious_insert_check() {
  # `services_publisher_insert`'s WITH CHECK gains `and state <> 'published'`.
  # Every D-tier write fixture inserts a `draft`, because the fixture was written
  # around the draft → submitted → published workflow, so the extra clause is
  # satisfied by the fixture and this narrowing is satisfied too — and the D tier
  # still goes red, because the insertion of `alpha-new` in D7 fails on the
  # `state` default rather than on anything this recipe changed.
  #
  # It is in the recipe for the reason N2 is, and not because it was predicted to
  # behave: this is the mutation that tests whether the fixtures' uniform state
  # makes the D tier's write path insensitive to the policy that governs it.
  # WRITTEN FOR: D7/D8.
  replace_once "N3" "$R6" \
'create policy services_publisher_insert
  on pantry.services
  for insert
  to pantry_publisher, pantry
  with check (publisher_id = (select pantry.current_publisher_id())
              and trust = '"'"'third_party'"'"');
' \
'create policy services_publisher_insert
  on pantry.services
  for insert
  to pantry_publisher, pantry
  with check (publisher_id = (select pantry.current_publisher_id())
              and trust = '"'"'third_party'"'"'
              and state <> '"'"'published'"'"');
' || return 1
  grep_confirm "$R6" "and state <> 'published');" 1 "N3" \
    " — the spurious clause is in the file" || return 1
  # Literal containment, not `grep -c`: a needle carrying a newline never matches
  # a line, so the first version of this confirmation matched 711 lines and
  # aborted the breakage instead of confirming it. And the needle must be the
  # WHOLE policy block rather than the two-line tail — `services_publisher_update`'s
  # `with check` ends with those same two lines, so a two-line negative
  # confirmation finds one occurrence whether or not the mutation applied, which
  # is a confirmation that confirms nothing.
  block_confirm "$R6" "create policy services_publisher_insert
  on pantry.services
  for insert
  to pantry_publisher, pantry
  with check (publisher_id = (select pantry.current_publisher_id())
              and trust = 'third_party');
" 0 "N3" \
    " — master's INSERT policy block is gone from the file, as a whole" || return 1
  block_confirm "$R6" "create policy services_publisher_insert
  on pantry.services
  for insert
  to pantry_publisher, pantry
  with check (publisher_id = (select pantry.current_publisher_id())
              and trust = 'third_party'
              and state <> 'published');
" 1 "N3" \
    " — the whole block is present, master's clauses in order, the spurious one last" || return 1
  say "        N3: services_publisher_insert given a WITH CHECK clause that refuses a"
  say "            legitimate write the fixtures do not happen to make"
}

# N4 — narrow the publishers UPDATE policy so a publisher cannot update its own row.
n4_narrow_publisher_update() {
  # `publishers_publisher_update`'s USING gains `and is_first_party`. Alpha is a
  # third-party publisher, so the rule "only a first-party account may change a
  # publisher row" reads as a sensible tightening — and it means every publisher
  # in the fleet can no longer update anything about itself.
  #
  # This is the one narrowing that touches the path Part 1 found has no check at
  # all, which is the reason it is here: it is the over-refusal a reader is most
  # likely to believe is covered, because `publishers` looks like the most
  # protected table in the schema.
  # WRITTEN FOR: nothing, and that is the measurement.
  replace_once "N4" "$R6" \
'create policy publishers_publisher_update
  on pantry.publishers
  for update
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id()))
  with check (id = (select pantry.current_publisher_id()));
' \
'create policy publishers_publisher_update
  on pantry.publishers
  for update
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id()) and is_first_party)
  with check (id = (select pantry.current_publisher_id()));
' || return 1
  block_confirm "$R6" '  using (id = (select pantry.current_publisher_id()) and is_first_party)
' 1 "N4" \
  " — the UPDATE policy's USING now also requires is_first_party" || return 1
  block_confirm "$R6" '  using (id = (select pantry.current_publisher_id()))
  with check (id = (select pantry.current_publisher_id()));
' 0 "N4" \
  " — master's UPDATE policy text is gone from the file" || return 1
  say "        N4: publishers_publisher_update narrowed so a third-party publisher"
  say "            cannot update its own publisher row at all"
}

# ---------------------------------------------------------------------------
# one_breakage <id> <plant fn> <mechanical expectation> <what it was written for>
# ---------------------------------------------------------------------------
one_breakage() {
  local id="$1" plant="$2" predicted="$3" written_for="$4"
  head2 "$id  —  $written_for"
  PLANTED=$((PLANTED+1))
  if ! "$plant"; then
    PLANT_FAILED=$((PLANT_FAILED+1))
    say "ABORT  $id: the mutation did not apply exactly once. NOT running the suite —"
    say "        a green over a mutation that landed nowhere reads as a pass."
    restore "$id"
    return 1
  fi
  run_suite "$id" || { restore "$id"; return 1; }
  observe "$id" "$predicted"
  # The witness runs BEFORE the revert, on the mutated tree. Skipping it on a
  # breakage that went red would be defensible (the red is its own evidence) and
  # skipping it on one that did not would be exactly the mistake this function
  # exists to prevent, so it runs either way.
  w_witness "$id"
  restore "$id" || return 1
  # Re-run green after every revert. A breakage that is red and never restored is
  # not a mutation test, it is vandalism, and the only cheap proof that the
  # restore took is to make the suite say so again.
  run_suite "$id-restored" || return 1
  if [ "$RUN_FAIL" -ne 0 ]; then
    say "ABORT  $id: the suite is still red AFTER the revert. The tree did not come back."
    classify | cut -f2 | sed 's/^/          still failing: /'
    PROBLEMS=$((PROBLEMS+1))
    return 1
  fi
  CONTROLS_RUN=$((CONTROLS_RUN+1)); CONTROLS_GREEN=$((CONTROLS_GREEN+1))
  say "        restored: $RUN_TOTAL run, $RUN_PASS passed, 0 failed — green again"
  return 0
}

do_control() {
  head2 "CONTROL — untouched tree"
  CONTROLS_RUN=$((CONTROLS_RUN+1))
  run_suite "control-$(date +%s%N)" || return 1
  if [ "$RUN_FAIL" -ne 0 ]; then
    say "ABORT  the control is NOT green ($RUN_FAIL failed)."
    say "        The packet's rule: if the control is not green, stop and say so."
    say "        Breakages measured against a red baseline measure nothing."
    classify | cut -f2 | sed 's/^/          /'
    PROBLEMS=$((PROBLEMS+1))
    return 1
  fi
  CONTROLS_GREEN=$((CONTROLS_GREEN+1))
  say "        CONTROL GREEN: $RUN_TOTAL run, $RUN_PASS passed, $RUN_FAIL failed, $RUN_SKIP skipped"
  say "        Every RED below is measured against exactly this."
  return 0
}

print_plan() {
  say "pantry-narrowing-01 / narrowing — the plan"
  say ""
  say "PART 1 — four NARROWINGS. Each must go red on a check asserting a"
  say "        LEGITIMATE operation works; a red on a refusal check proves nothing."
  say "  N1  00006_rls.sql  publishers_publisher_select: + and verified"
  say "                              [a column every fixture satisfies]"
  say "  N2  00006_rls.sql  publishers_publisher_select: + and claimed_at < now() - 1 year"
  say "                              [a clause no seeded publisher satisfies]"
  say "  N3  00006_rls.sql  services_publisher_insert: + and state <> 'published'"
  say "                              [a spurious WITH CHECK on an insert policy]"
  say "  N4  00006_rls.sql  publishers_publisher_update: + and is_first_party"
  say "                              [a publisher cannot update itself]"
  say ""
  say "Every breakage: grep-confirmed applied -> run -> WITNESS -> restore ->"
  say "re-run green. The witness stands a cluster up from the MUTATED migrations/"
  say "and prints the publisher's own answer, because 'every check passed' is also"
  say "what a mutation that changed nothing at runtime prints."
  say "A breakage that does NOT go red is printed loudly. That is a result."
}

# ---------------------------------------------------------------------------
# MAIN
# ---------------------------------------------------------------------------
if [ "$PHASE" = "list" ]; then print_plan; exit 0; fi

say "pantry-narrowing-01 / narrowing"
say "repo:   $REPO"
say "branch: $(git rev-parse --abbrev-ref HEAD) @ $(git rev-parse --short HEAD)"

if ! tree_clean; then
  say "ABORT  ${SUBJECT_DIRS[*]} is not clean at the start of the run."
  say "        A mutation recipe that starts from a dirty tree cannot tell its own"
  say "        damage from somebody else's. Commit or stash first."
  git --no-pager diff --stat master -- "${SUBJECT_DIRS[@]}" | sed 's/^/        /'
  exit 2
fi

do_control || {
  say ""
  say "narrowing.sh: the control did not pass, so nothing was planted."
  say "Exiting non-zero without having broken anything."
  exit 1
}

if [ "$PHASE" = "all" ] || [ "$PHASE" = "part1" ]; then
  say ""
  w_baseline || { say ""; say "narrowing.sh: no witness baseline, so no NOT-RED would be"; say "             readable as a finding. Exiting without planting anything."; exit 1; }
  say ""
  say "PART 1 — four narrowings"
  one_breakage N1 n1_tighten_select_with_verified "ALL GREEN" \
    "publishers_publisher_select tightened with a column alpha satisfies" || true
  one_breakage N2 n2_tighten_select_with_account_age "D2/legitimate reads narrow" \
    "publishers_publisher_select tightened with a clause no publisher satisfies" || true
  one_breakage N3 n3_spurious_insert_check "D7/D8 refuse a legitimate insert" \
    "services_publisher_insert given a spurious WITH CHECK" || true
  one_breakage N4 n4_narrow_publisher_update "the UPDATE path refuses alpha" \
    "publishers_publisher_update narrowed so a publisher cannot update itself" || true
fi

head2 "FINAL VERIFICATION"
if do_control; then
  say "        FINAL CONTROL GREEN: $RUN_TOTAL run, $RUN_PASS passed, $RUN_FAIL failed"
else
  say "ABORT  the final control is NOT green. The recipe did not restore the tree."
fi

rule
if tree_clean; then
  say "git diff --quiet master -- migrations/ tests/  ->  CLEAN (no differences from master)"
  say "git diff --quiet -- migrations/ tests/         ->  CLEAN (nothing unstaged)"
  say "git status --porcelain -- migrations/ tests/   ->  empty (nothing untracked)"
else
  say "git diff --quiet master -- migrations/ tests/  ->  DIRTY"
  git --no-pager diff --stat master -- "${SUBJECT_DIRS[@]}" | sed 's/^/        /'
  PROBLEMS=$((PROBLEMS+1))
fi

rule
say "TALLIES — breakages planted and reds observed are different numbers and are"
say "         never added together. So are controls and passes."
printf 'controls run (every one green):        %s\n' "$CONTROLS_RUN"
printf 'breakages planted:                     %s\n' "$((PLANTED-PLANT_FAILED))"
printf '  of which the plant FAILED:           %s\n' "$PLANT_FAILED"
printf 'breakages reverted byte-for-byte:      %s\n' "$RESTORED"
printf '  of which the restore FAILED:         %s\n' "$RESTORE_FAILED"
printf 'breakages red on a LEGITIMATE op:      %s\n' "$RED_LEGIT"
printf 'breakages red only on refusal checks:  %s\n' "$RED_WRONG_POLARITY"
printf 'breakages NOT RED AT ALL:              %s\n' "$NOT_RED_LEGIT"
printf 'recipe problems (structural):          %s\n' "$PROBLEMS"

rule
say "PER-BREAKAGE"
printf '%-5s %-19s %-30s %-6s %-22s %s\n' LABEL OUTCOME "MECHANICAL EXPECTATION" D-RED "LEGITIMATE-OP CHECKS" ALL-RED
if [ -s "$SUMMARY" ]; then
  while IFS=$'\t' read -r a b c d e f; do
    printf '%-5s %-19s %-30s %-6s %-22s %s\n' "$a" "$b" "$c" "$d" "$e" "$f"
  done <"$SUMMARY"
elif [ "$PLANTED" -eq 0 ]; then
  say "(no breakage ran in this phase — \`--phase $PHASE\`)"
else
  say "(no breakage ran — the control did not pass, and the packet's rule is to stop there)"
fi

rule
if [ "$PROBLEMS" -eq 0 ]; then
  say "ok — the RECIPE is sound: control green, every mutation applied exactly once,"
  say "     every mutation reverted, migrations/ and tests/ byte-identical to master."
  say "     That is not a claim that every narrowing was caught. For that, read the"
  say "     PER-BREAKAGE table above."
  rm -rf "$WORK"
  exit 0
fi
say "FAIL — $PROBLEMS structural problem(s) above. This run is not a result."
say "      logs and results.tsv kept in $WORK"
exit 1