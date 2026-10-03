#!/usr/bin/env bash
#
# reports/pantry-rls-mutations-01/mutations.sh — can pantry's RLS suite be made to
# fail by breaking the policies it is about?
#
# WHY THIS FILE EXISTS. REPORT-registry-pantry-schema-02.md §4 recorded four
# mutations, all four red. Three of them removed a GRANT (a schema USAGE, a
# revoke, a table grant) and the fourth flipped a literal as a control. Every one
# of them is a **privilege** mutation. None of them removed, narrowed or widened a
# **policy**, and the policies are the isolation question: D2, D5, D10, D12 and
# the publisher tier are what "one publisher cannot read another's rows" is *made
# of*.
#
# So "4 of 4 red" in that report is evidence about grants and is silent about the
# policies. This file is the six policy breakages that were missing, and it is
# written so the proof can fail.
#
# IT IS A SHELL SCRIPT BECAUSE tests/rls.sh IS ONE. That suite stands up its own
# PostgreSQL from binaries, needs no container and reads no PANTRY_DATABASE_URL.
# A third instrument — a Go test tier, a kit adapter — would be a second number
# about a boundary this suite already measures against a real database, and a
# second number is not a stronger one. This file adds no build-time surface at
# all: `bin/prime` counts test binaries with
# `find tests -maxdepth 1 -name '*.rs'`, and nothing here is in `tests/`.
#
#   ./reports/pantry-rls-mutations-01/mutations.sh              # control + part 1 + part 2
#   ./reports/pantry-rls-mutations-01/mutations.sh --phase control
#   ./reports/pantry-rls-mutations-01/mutations.sh --phase part1
#   ./reports/pantry-rls-mutations-01/mutations.sh --phase part2
#   ./reports/pantry-rls-mutations-01/mutations.sh --list       # print the plan, run nothing
#
# ---------------------------------------------------------------------------
# THE FOUR VERDICTS, AND WHY TWO IS NOT ENOUGH
# ---------------------------------------------------------------------------
# tests/rls_checks.sh asserts refusals with `deny`, which requires the SQLSTATE
# *and* the message, precisely so a suite cannot pass with all policies
# `using (false)` and all grants missing. That distinction is the value here, and
# this file refuses to collapse it:
#
#   GRANT   `permission denied for table` — Postgres refused at aclcheck time,
#           before RLS was consulted. Says NOTHING about policies. A mutation
#           that only ever lands here has proven the privilege list is doing its
#           job and has proved nothing about isolation.
#
#   POLICY  `violates row-level security policy` — the policy itself refused.
#           This is the barrier D9/D10/D11/D16/F5/F6/F9 name, and the only one
#           that is evidence about a predicate.
#
#   ABSENT  Neither. The statement was ALLOWED, or a read returned rows it must
#           not. For a WIDENING mutation this is the *expected and strongest*
#           outcome: the boundary did not refuse, so there is nothing to name.
#           Printing it as "the policy refused" would be a lie, and a reader who
#           believes it is exactly the reader this packet is written for.
#
#   CATALOG A fact about pg_roles / pg_class changed. Real, and not barrier
#           evidence — a role's attribute is not a refusal.
#
# The verdict is derived from what the runner RECORDED (the expectation it held
# and the server's own words), never from the check's name and never from what
# the mutation was trying to do. A recipe that decided in advance which barrier a
# breakage "should" hit would be reporting its own expectation back to itself.
#
# ---------------------------------------------------------------------------
# WHAT THIS FILE WILL NOT DO
# ---------------------------------------------------------------------------
# * It never edits a check to make a breakage red, and it never adds a check.
#   B6 and M4 flip one expected literal and revert it; that is the control the
#   packet asked for, and it is a mutation of an assertion rather than a change
#   to it. Both are restored and the tree is verified afterwards.
# * It never leaves `migrations/` or `tests/` different from master. Every
#   mutation is reverted from git and verified twice: `git diff --quiet master --
#   migrations/ tests/` and the final control run.
# * It never counts a green run as a pass for a breakage. A breakage that went
#   green — or that went red only outside the D tier — is printed as NOT RED,
#   loudly, and is not hidden in the exit status, because "the script exited 0"
#   must never be readable as "every breakage was caught".
#
# THE EXIT STATUS IS ABOUT THE RECIPE, NOT ABOUT THE BREAKAGES. It is 0 when the
# control was green, every mutation applied exactly once (grep-confirmed), every
# mutation was reverted byte-for-byte, and the final control was green. Whether a
# given breakage produced a D-tier red is *data*, printed in the tables below and
# argued in REPORT-pantry-rls-mutations-01.md.
set -uo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/pantry-rls-mut.XXXXXX")"
SUITE="$REPO/tests/rls.sh"
SUMMARY="$WORK/summary.tsv"
: >"$SUMMARY"

# Which files this recipe is allowed to touch, and therefore which files must be
# byte-identical to master at the end. Named in one place so the restore check and
# the "do not touch" rule cannot drift apart.
SUBJECT_DIRS=(migrations tests)

CONTROLS_RUN=0
CONTROLS_GREEN=0
PLANTED=0
PLANT_FAILED=0
RED_D=0
NOT_RED_D=0
RESTORED=0
RESTORE_FAILED=0
PROBLEMS=0

PHASE="all"
case "${1:-}" in
  --phase) PHASE="${2:-all}" ;;
  --phase=*) PHASE="${1#--phase=}" ;;
  --list) PHASE="list" ;;
  "") ;;
  *) echo "mutations.sh: unknown argument '$1'. --phase control|part1|part2|all | --list" >&2; exit 2 ;;
esac

say()   { printf '%s\n' "$*"; }
rule()  { say "----------------------------------------------------------------"; }
head2() { say ""; say "== $*"; }

# ---------------------------------------------------------------------------
# replace_once <label> <file> <old> <new>   — exact, and exactly once
# ---------------------------------------------------------------------------
# The count assertion is the whole point. A `sed` that matched nothing produces a
# green suite, and a green suite produced by a mutation that never applied is
# worse than no run at all: it reads as a pass. So anything but exactly one match
# aborts the breakage rather than running the suite.
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

delete_once() { replace_once "$1" "$2" "$3" ""; }

# grep_confirm <file> <fixed-string> <expected-count> <label> [note]
# Run AFTER the write and BEFORE the suite, so the transcript shows the mutation
# present in the file the suite is about to read. This is the step the earlier
# recipe recorded as "mutated line found x1", and it is the reason its numbers can
# be read at all. An expected count of 0 is a legitimate negative confirmation:
# "the line is gone, and the lines around it are not".
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
# `--keep` leaves `$WORK/results.tsv` behind after the cluster is stopped, and
# that file is one row per check: name, status, expectation, observation. Reading
# it is how this script names WHICH check went red and WHAT refused it, rather
# than reporting "red" — a count with no name attached is not a result.
RUN_LOG=""; RUN_RC=0; RESULTS=""; RUN_TOTAL=0; RUN_PASS=0; RUN_FAIL=0; RUN_SKIP=0
run_suite() {
  local label="$1" kept line
  RUN_LOG="$WORK/$label.log"
  timeout 600 "$SUITE" --keep >"$RUN_LOG" 2>&1
  RUN_RC=$?
  kept=$(sed -n 's/^kept: //p' "$RUN_LOG" | head -1)
  RESULTS="$kept/results.tsv"
  line=$(sed -n 's/^checks: \([0-9]*\) run, \([0-9]*\) passed, \([0-9]*\) failed, \([0-9]*\) skipped.*/\1 \2 \3 \4/p' "$RUN_LOG" | head -1)
  if [ -z "$line" ]; then
    say "ABORT  $label: tests/rls.sh printed no 'checks:' line (exit $RUN_RC)."
    say "        A suite that never ran is not a green suite and is not a red one."
    tail -20 "$RUN_LOG" | sed 's/^/        /'
    PROBLEMS=$((PROBLEMS+1))
    RUN_TOTAL=0; RUN_PASS=0; RUN_FAIL=0; RUN_SKIP=0; RESULTS=""
    return 1
  fi
  # shellcheck disable=SC2086
  set -- $line
  RUN_TOTAL=$1; RUN_PASS=$2; RUN_FAIL=$3; RUN_SKIP=$4
  say "        tests/rls.sh -> $RUN_TOTAL run, $RUN_PASS passed, $RUN_FAIL failed, $RUN_SKIP skipped (exit $RUN_RC)"
  [ -n "$kept" ] && rm -rf "$kept"
  return 0
}

# ---------------------------------------------------------------------------
# classify — one verdict per failed check, out of results.tsv
# ---------------------------------------------------------------------------
classify() {
  [ -n "${RESULTS:-}" ] && [ -f "$RESULTS" ] || return 0
  RESULTS_FILE="$RESULTS" python3 - <<'PY'
import os

CATALOG_PREFIXES = ("A5", "C13", "C15", "G1", "G2", "G3", "G4", "G5",
                    "I1", "I3", "I5", "I6", "J1", "J2", "J5", "F1")

def csv_len(csv):
    return len([x for x in csv.split(",") if x.strip()])

for line in open(os.environ["RESULTS_FILE"], encoding="utf-8"):
    parts = line.rstrip("\n").split("\t")
    while len(parts) < 4:
        parts.append("")
    name, status, exp, obs = parts[0], parts[1], parts[2], parts[3]
    if status != "FAIL":
        continue

    # The server's own words first: if it named a barrier, that IS the barrier,
    # whatever the runner was hoping for.
    if "row-level security" in obs:
        v = "POLICY"
    elif "permission denied for table" in obs or "permission denied for schema" in obs:
        v = "GRANT"
    elif exp.startswith("statement SUCCEEDED"):
        # The denial did not happen. For a widening mutation this is the point.
        v = "ABSENT-ALLOWED"
    elif exp.startswith("wrong sqlstate"):
        v = "OTHER-SQLSTATE"
    elif exp.startswith("refused by the wrong barrier"):
        v = "OTHER-BARRIER"
    elif exp.startswith("wanted: "):
        want = exp[len("wanted: "):]
        got = obs[len("got: "):] if obs.startswith("got: ") else obs
        if csv_len(got) > csv_len(want):
            v = "ABSENT-WIDENED"
        elif csv_len(got) < csv_len(want):
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

# tier <check name> — A/B/C/D/E/F/G/H/I/J from the check's own name.
tier() { printf '%s' "$1" | sed -E 's/^([A-J])[0-9].*/\1/'; }

# observe <label> <written-for tier>
# Prints the verdict table and accounts for this breakage. RED_D counts
# breakages with at least one D-tier red; NOT_RED_D counts the rest. The two are
# kept apart on purpose and are never added together.
OBS_D=0; OBS_OTHER=0; OBS_NAMES=""; OBS_NOTE=""
observe() {
  local label="$1" predicted="$2" out v n
  out=$(classify)
  if [ -z "$out" ]; then
    say "        NOT RED AT ALL: every check passed with the breakage planted."
    NOT_RED_D=$((NOT_RED_D+1))
    OBS_NAMES="(none)"; OBS_NOTE="NOT-RED-AT-ALL"
    printf '%s\tNOT RED\t%s\t0\t0\t(none) — every check passed\n' "$label" "$predicted" >>"$SUMMARY"
    return 0
  fi
  OBS_NAMES=$(printf '%s\n' "$out" | cut -f2 | paste -sd, -)
  while IFS=$'\t' read -r v n _ _; do
    [ "$(tier "$n")" = "D" ] && OBS_D=$((OBS_D+1)) || OBS_OTHER=$((OBS_OTHER+1))
    say "        $(printf '%-18s %s' "$v" "$n")"
  done <<<"$out"
  if [ "$OBS_D" -gt 0 ]; then
    RED_D=$((RED_D+1)); OBS_NOTE="RED"
    say "        -> $OBS_D D-tier red, $OBS_OTHER elsewhere. D tier: $(printf '%s\n' "$out" | cut -f2 | grep -E '^D[0-9]' | paste -sd, -)"
  else
    NOT_RED_D=$((NOT_RED_D+1)); OBS_NOTE="NOT-RED-IN-D"
    say "        -> NOT RED IN THE D TIER. 0 publisher-isolation checks noticed."
    say "        -> every red was: $OBS_NAMES"
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$label" "$OBS_NOTE" "$predicted" "$OBS_D" "$OBS_OTHER" "$OBS_NAMES" >>"$SUMMARY"
  return 0
}

# ---------------------------------------------------------------------------
# THE SIX PART-1 BREAKAGES
# ---------------------------------------------------------------------------
# Each is a file, an exact replacement, and a statement of the check it was
# WRITTEN FOR. That last field is the point of the exercise: "red" is not a
# result; "red, and the check that went red is the one that asks whether another
# publisher's service is invisible by primary key" is a result.
MIG="$REPO/migrations"
R6="$MIG/00006_rls.sql"
R5="$MIG/00005_functions.sql"
R1="$MIG/00001_roles.sql"
CK="$REPO/tests/rls_checks.sh"

# The hardcoded publisher id in B3 is fixture `alpha`, so a stubbed identity is
# indistinguishable from a real alpha session — which is exactly the point: the
# boundary has to depend on something other than the value the function returns.
ALPHA_ID="00000000-0000-4000-8000-0000000000a1"

b1_drop_select_predicate() {
  # `publishers_publisher_select`'s `using` becomes `using (true)`. The policy
  # still exists, still names the same roles, still parses, and a publisher now
  # reads every publishers row.
  # WRITTEN FOR: D2 — "sees its own publisher row and no other".
  replace_once "B1" "$R6" \
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
  using (true);
' || return 1
  grep_confirm "$R6" 'to pantry_publisher, pantry
  using (true);' 1 "B1" || return 1
  say "        B1: publishers_publisher_select lost its predicate — using -> true"
}

b2_delete_with_check() {
  # `publishers_publisher_update`'s `with check` is deleted; `using` survives.
  # The packet predicts the update path opens. Postgres's documented behaviour
  # for an UPDATE policy with no WITH CHECK is to fall back to the USING
  # expression, so the honest expectation is that this stays green — which is a
  # result about the packet's reasoning and not about the suite. Measured, not
  # assumed.
  # WRITTEN FOR: the publishers update path, with the read predicate surviving.
  replace_once "B2" "$R6" \
'  using (id = (select pantry.current_publisher_id()))
  with check (id = (select pantry.current_publisher_id()));
' \
'  using (id = (select pantry.current_publisher_id()));
' || return 1
  grep_confirm "$R6" 'publishers_publisher_update' 1 "B2" \
    ", and no with check line remains under it" || return 1
  grep_confirm "$R6" '  with check (id = (select pantry.current_publisher_id()));' 0 "B2" || return 1
  say "        B2: publishers_publisher_update lost its with check — using survives"
}

b3_stub_current_publisher_id() {
  # The nastiest class: every policy is untouched, syntactically valid, and still
  # reads `id = (select pantry.current_publisher_id())`. The function just answers
  # a constant, so the boundary is gone while pg_policies is byte-for-byte what it
  # was. It also reaches the public read path, because `service_is_visible`
  # compares `publisher_id` against the same function.
  # WRITTEN FOR: D6 — "with no identity a publisher reads zero rows".
  replace_once "B3" "$R5" \
"  raw := current_setting('pantry.publisher_id', true);" \
"  raw := '$ALPHA_ID';" || return 1
  grep_confirm "$R5" "  raw := '$ALPHA_ID';" 1 "B3" || return 1
  grep_confirm "$R5" "current_setting('pantry.publisher_id', true)" 0 "B3" || return 1
  say "        B3: current_publisher_id() answers a constant regardless of session identity"
}

b4_unforce_services() {
  # `force row level security` comes off `services`, so the owner keeps its
  # exemption and `pantry` walks past its own policies.
  # WRITTEN FOR: NOT the D tier. `pantry_publisher` is not the table owner, so
  # nothing in D can see this — which is the finding. The tier that holds FORCE
  # down is F, plus I4 and J5.
  delete_once "B4" "$R6" "alter table pantry.services force  row level security;" || return 1
  grep_confirm "$R6" "alter table pantry.services force  row level security;" 0 "B4" \
    " — services is no longer FORCEd" || return 1
  grep_confirm "$R6" "alter table pantry.publishers force  row level security;" 1 "B4" \
    " — the other three tables still are" || return 1
  say "        B4: force row level security removed from pantry.services"
}

b5_drop_noinherit_publisher() {
  # `noinherit` dropped from `pantry_publisher` in 00001_roles.sql.
  #
  # BOTH SITES, and the reason is the first thing this breakage found.
  # `noinherit` is set twice: inside the `create role` in the `do $$ $$` block,
  # and again by the unconditional `alter role … noinherit` re-assert after it.
  # Removing only the `alter role` line is INERT on a fresh cluster — the role
  # does not exist yet, so it is created noinherit. Removing only the
  # `create role` token is undone by the re-assert a few lines later. A
  # single-site mutation of this fact is a green that means nothing.
  # WRITTEN FOR: C15 — "every pantry role is NOINHERIT" — and, more importantly,
  # any check that would show inheritance actually widening what the role can do.
  replace_once "B5a" "$R1" \
"    create role pantry_publisher nologin noinherit;" \
"    create role pantry_publisher nologin;" || return 1
  delete_once "B5b" "$R1" "alter role pantry_publisher noinherit;" || return 1
  grep_confirm "$R1" "create role pantry_publisher nologin;" 1 "B5" || return 1
  grep_confirm "$R1" "alter role pantry_publisher noinherit;" 0 "B5" || return 1
  say "        B5: noinherit dropped from pantry_publisher at both sites in 00001_roles.sql"
}

b6_flip_d1_literal() {
  # THE CONTROL. One expected literal in one existing D-tier assertion. This must
  # go red with exactly D1 named, and that is what proves breakages 1-5 were not
  # red for some ambient reason: if the harness were red in general, this would
  # come out red too and the other five would mean nothing. It also proves this
  # recipe's replace machinery can turn a check red at all, which is the half of
  # a mutation test that is easiest to assume.
  #
  # MUTATING A CHECK IS ALLOWED HERE, TEMPORARILY, AND ONLY HERE. The packet's
  # rule is "never mutate pantry's checks to make a breakage red". This does the
  # opposite: it breaks a check that is currently green, on purpose, and restores
  # it. Nothing is added and nothing is weakened; the tree is verified
  # byte-identical to master at the end.
  # WRITTEN FOR: D1 — "this suite's SET ROLE is subject to RLS".
  replace_once "B6" "$CK" \
'"alpha-api,alpha-draft"' \
'"alpha-api,alpha-draft,bravo-api"' || return 1
  grep_confirm "$CK" '"alpha-api,alpha-draft,bravo-api"' 1 "B6" || return 1
  grep_confirm "$CK" '"alpha-api,alpha-draft"' 0 "B6" || return 1
  say "        B6: D1's expected row set gained bravo-api  [CONTROL]"
}

# ---------------------------------------------------------------------------
# THE FOUR RECORDED MUTATIONS, re-run against THIS suite
# ---------------------------------------------------------------------------
# REPORT-registry-pantry-schema-02.md §4, verbatim in intent. They are recorded as
# red against 77 checks. This suite is neither 77 nor 84 — the report gives the
# measured count and where the delta came from — so the numbers that report quotes
# are numbers about a suite that no longer exists, and re-running is the only way
# to say anything true about the current one.
m1_drop_pantry_usage() {
  # drop `, pantry` from the schema usage grant
  replace_once "M1" "$R6" \
"grant usage on schema pantry to pantry_public, pantry_publisher, pantry_admin, pantry;" \
"grant usage on schema pantry to pantry_public, pantry_publisher, pantry_admin;" || return 1
  grep_confirm "$R6" "grant usage on schema pantry to pantry_public, pantry_publisher, pantry_admin;" 1 "M1" || return 1
  say "        M1: \`, pantry\` dropped from the schema USAGE grant   (recorded: 34 of 77 red)"
}

m2_revoke_public_usage() {
  # re-add `revoke all on schema pantry from pantry_public`, immediately after the
  # grant it undoes — the order the original defect had.
  replace_once "M2" "$R6" \
"grant usage on schema pantry to pantry_public, pantry_publisher, pantry_admin, pantry;
" \
"grant usage on schema pantry to pantry_public, pantry_publisher, pantry_admin, pantry;
revoke all on schema pantry from pantry_public;
" || return 1
  grep_confirm "$R6" "revoke all on schema pantry from pantry_public;" 1 "M2" || return 1
  say "        M2: revoke all on schema pantry from pantry_public re-added   (recorded: 18 of 77)"
}

m3_drop_admin_grant() {
  # delete the pantry_admin table grant
  replace_once "M3" "$R6" \
"grant select, insert, update, delete on
  pantry.services,
  pantry.service_versions,
  pantry.publishers,
  pantry.service_compat
  to pantry_admin;
" "" || return 1
  grep_confirm "$R6" "grant select, insert, update, delete on" 0 "M3" \
    " — no table grant with all four verbs remains" || return 1
  say "        M3: the pantry_admin table grant deleted   (recorded: 29 of 77 red)"
}

m4_flip_c1_literal() {
  # the original control: flip C1's expected row set
  replace_once "M4" "$CK" \
'"alpha-api,bravo-api,cafaye"' \
'"alpha-api,bravo-api,cafaye,alpha-draft"' || return 1
  grep_confirm "$CK" '"alpha-api,bravo-api,cafaye,alpha-draft"' 1 "M4" || return 1
  grep_confirm "$CK" '"alpha-api,bravo-api,cafaye"' 0 "M4" || return 1
  say "        M4: C1's expected row set gained alpha-draft  [CONTROL]  (recorded: 1 of 77)"
}

# ---------------------------------------------------------------------------
# one_breakage <id> <plant fn> <written-for tier> <what it was written for>
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

# ---------------------------------------------------------------------------
# THE CONTROL
# ---------------------------------------------------------------------------
do_control() {
  head2 "CONTROL — untouched tree"
  CONTROLS_RUN=$((CONTROLS_RUN+1))
  run_suite "control-$(date +%s%N)" || return 1
  if [ "$RUN_FAIL" -ne 0 ]; then
    say "ABORT  the control is NOT green ($RUN_FAIL failed)."
    say "        The packet's rule: if the control is not green, stop and say so."
    say "        Breakages measured against a red baseline measure nothing."
    say "        failing checks:"
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
  say "pantry-rls-mutations-01 — the plan"
  say ""
  say "PART 1 — six policy breakages"
  say "  B1  00006_rls.sql       publishers_publisher_select: using -> true       [for D2]"
  say "  B2  00006_rls.sql       publishers_publisher_update: with check DELETED   [update path]"
  say "  B3  00005_functions.sql current_publisher_id() -> a constant            [for D6]"
  say "  B4  00006_rls.sql       services: force row level security REMOVED       [F tier, not D]"
  say "  B5  00001_roles.sql     pantry_publisher: noinherit dropped, both sites   [for C15]"
  say "  B6  tests/rls_checks.sh D1's expected literal flipped        [CONTROL]   [for D1]"
  say ""
  say "PART 2 — the four recorded mutations, re-run against THIS suite"
  say "  M1  00006_rls.sql       \`, pantry\` dropped from the schema USAGE grant"
  say "  M2  00006_rls.sql       revoke all on schema pantry from pantry_public re-added"
  say "  M3  00006_rls.sql       the pantry_admin table grant deleted"
  say "  M4  tests/rls_checks.sh C1's expected literal flipped        [CONTROL]"
  say ""
  say "Every breakage: grep-confirmed applied -> run -> restore -> re-run green."
  say "Every red is reported with the check that went red AND the barrier that"
  say "refused, or — for a widening — the fact that nothing refused at all."
}

# ---------------------------------------------------------------------------
# MAIN
# ---------------------------------------------------------------------------
if [ "$PHASE" = "list" ]; then print_plan; exit 0; fi

say "pantry-rls-mutations-01"
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
  say "mutations.sh: the control did not pass, so nothing was planted."
  say "Exiting non-zero without having broken anything."
  exit 1
}

if [ "$PHASE" = "all" ] || [ "$PHASE" = "part1" ]; then
  say ""
  say "PART 1 — six policy breakages"
  one_breakage B1 b1_drop_select_predicate "D2" \
    "publishers_publisher_select loses its read predicate" || true
  one_breakage B2 b2_delete_with_check "update path" \
    "publishers_publisher_update loses its with check" || true
  one_breakage B3 b3_stub_current_publisher_id "D6" \
    "current_publisher_id() answers a constant" || true
  one_breakage B4 b4_unforce_services "F (not D)" \
    "force row level security removed from pantry.services" || true
  one_breakage B5 b5_drop_noinherit_publisher "C15" \
    "noinherit dropped from pantry_publisher" || true
  one_breakage B6 b6_flip_d1_literal "D1" \
    "D1's expected literal flipped  [CONTROL]" || true
fi

if [ "$PHASE" = "all" ] || [ "$PHASE" = "part2" ]; then
  say ""
  say "PART 2 — the four recorded mutations, against this suite"
  one_breakage M1 m1_drop_pantry_usage "GRANT" \
    "\`, pantry\` dropped from the schema USAGE grant" || true
  one_breakage M2 m2_revoke_public_usage "GRANT" \
    "revoke all on schema pantry from pantry_public re-added" || true
  one_breakage M3 m3_drop_admin_grant "GRANT" \
    "the pantry_admin table grant deleted" || true
  one_breakage M4 m4_flip_c1_literal "C1" \
    "C1's expected literal flipped  [CONTROL]" || true
fi

# ---------------------------------------------------------------------------
# FINAL VERIFICATION, and the tallies, separately.
# ---------------------------------------------------------------------------
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
printf 'breakages with >= 1 D-tier red:        %s\n' "$RED_D"
printf 'breakages with 0  D-tier red:          %s\n' "$NOT_RED_D"
printf 'recipe problems (structural):          %s\n' "$PROBLEMS"

rule
say "PER-BREAKAGE"
printf '%-5s %-16s %-14s %-8s %-13s %s\n' LABEL OUTCOME "WRITTEN-FOR" D-RED OTHER-RED CHECKS
if [ -s "$SUMMARY" ]; then
  while IFS=$'\t' read -r a b c d e f; do
    printf '%-5s %-16s %-14s %-8s %-13s %s\n' "$a" "$b" "$c" "$d" "$e" "$f"
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
  say "     That is not a claim that every breakage was caught. For that, read the"
  say "     PER-BREAKAGE table above."
  rm -rf "$WORK"
  exit 0
fi
say "FAIL — $PROBLEMS structural problem(s) above. This run is not a result."
say "      logs and results.tsv kept in $WORK"
exit 1