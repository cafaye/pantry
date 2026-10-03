#!/usr/bin/env bash
#
# reports/pantry-publisher-rewrite-01/mutations.sh — can the ten checks this
# packet added be made to fail by breaking the barrier they are about?
#
# WHY THIS FILE IS SEPARATE FROM THE FIX, and the distinction is executable rather
# than a matter of care. `migrations/` and `tests/` in this branch carry intended
# changes: `00008_publisher_identity_immutable.sql`, the corrected comments in
# `00002`/`00006`, `migrations/README.md`, and D18..D23 / E13..E15 / F10 in
# `tests/rls_checks.sh`. A recipe that also MUTATES those same files has two
# different things it can be doing to them, and a transcript that does not say
# which is which cannot be read afterwards. So:
#
#   the FIX     is the committed diff against master.  `intended_diff()` below
#               names it, file by file, and checks it is still exactly that.
#   a MUTATION  is an uncommitted edit on top of that, reverted from HEAD after
#               every single breakage and followed by a full green re-run.
#
# `restore()` checks out from **HEAD**, not from master — the same line in
# REPORT-pantry-rls-mutations-01's recipe is `git checkout master`, and copying it
# here would delete the fix this packet is about. Everything else follows the
# shape of that recipe, which is the point: `replace_once` exists because a `sed`
# that matched nothing produces a green suite, and a green suite produced by a
# mutation that landed nowhere reads as a pass.
#
#   ./reports/pantry-publisher-rewrite-01/mutations.sh              # control + all five
#   ./reports/pantry-publisher-rewrite-01/mutations.sh --phase control
#   ./reports/pantry-publisher-rewrite-01/mutations.sh --phase part1   # M1, M2
#   ./reports/pantry-publisher-rewrite-01/mutations.sh --phase part2   # M3, M4, M5
#   ./reports/pantry-publisher-rewrite-01/mutations.sh --list
#
# ---------------------------------------------------------------------------
# THE FIVE, AND WHY EACH EXISTS RATHER THAN BEING A VARIANT OF THE ONE BEFORE
# ---------------------------------------------------------------------------
# The barrier is one clause, so "remove it" is one experiment and running it five
# times would be theatre. These five are chosen so that each TIER this packet
# touched is red on its own at least once, which is the only way to claim the tier
# is holding something:
#
#   M1  `with check (false)` -> `with check (true)`.
#       The barrier deleted. D18..D23 and F10 go red.
#
#   M2  `with check (false)` -> `with check (id = current_publisher_id())`,
#       which is 00006's clause, verbatim.
#       This is the interesting one and the reason it is not redundant with M1: a
#       barrier that pins `id` IS a barrier, it refuses every statement these
#       checks make, and it is what shipped. If M2 were green, the ten checks
#       would be asserting "the policy exists" rather than "the policy pins the
#       identity" — which is precisely the mistake REPORT-pantry-rls-mutations-01
#       §4 (B2) declined to name.
#
#   M3  `alter table pantry.publishers no force row level security;` added to
#       00008's Up.
#       Nothing about the policy changes. The OWNER stops being subject to it.
#       D18..D23 stay GREEN and F10 goes red — which is the whole argument for
#       filing the owner's check in F and not in D, measured rather than asserted.
#
#   M4  `publishers_admin_update`'s `with check (true)` -> `with check (false)`
#       in 00006.
#       The over-refusal: the pin applied to the wrong set of roles. D18..D23 and
#       F10 stay GREEN and E13..E15 go red, because the fleet's own rename, verify
#       and promote stop working. A security fix is not finished when the attack
#       fails; it is finished when the attack fails and the three legitimate
#       writes still succeed, and only E can say the second half.
#
#   M5  the `create policy` in 00008's Up deleted outright — no UPDATE policy on
#       `publishers` at all.
#
#       THIS ONE REVERSED A CONCLUSION, so the reasoning is kept. It was written
#       expecting the security to break. It does not: nothing becomes writable. What
#       breaks is the ATTRIBUTION, and measured, with no policy in place:
#
#         set role pantry_publisher;
#         select pantry.begin_publisher('…a1');
#         update pantry.publishers set github_id = 999 where id = '…a1';
#           UPDATE 0
#         psql exit code: 0
#
#       Not an error. RLS with no matching UPDATE policy FILTERS every row out
#       rather than raising, so the statement succeeds having changed nothing.
#       D18..D22 and F10 are `deny` checks, so they go red — correctly, because
#       `deny` asserts the MECHANISM as well as the outcome, and `UPDATE 0` is
#       silent in a way that names no rule and is indistinguishable from "your
#       WHERE matched nothing".
#
#       That is the real argument for keeping an explicit `with check (false)`
#       instead of deleting the policy. Both close the hole. Only one turns a
#       silent filter into a 42501 that names `publishers`. A caller that cannot
#       tell "refused" from "matched nothing" will eventually retry, or log
#       success, or file a bug against the wrong thing — and the security-relevant
#       difference is not whether the write happened but whether anything can say
#       why it did not.
#
#       D23 is the sixth check and the only one that is not a `deny`, and this is
#       the mutation that shows what it is for: D23 reads the ROW, and under M5 the
#       row is untouched, so D23 stays GREEN. It cannot tell the difference either.
#       Six checks, five mechanisms-visible.
set -uo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)" || exit 2
cd "$REPO" || exit 2

M8="migrations/00008_publisher_identity_immutable.sql"
M6="migrations/00006_rls.sql"
SUITE="./tests/rls.sh"

# Which files a mutation may touch, and therefore which must be byte-identical to
# HEAD after every revert. HEAD, not master — see the header.
#
# Declared BEFORE the exit trap, because the trap reads it. An unbound array under
# `set -u` is itself an error, so a trap that fires in the four lines before this
# assignment would abort without printing the one message that explains why.
SUBJECT_DIRS=(migrations tests)

# PANTRY_PG_BIN is honoured by tests/rls.sh itself; this only forwards it.
export PANTRY_PG_BIN="${PANTRY_PG_BIN:-/opt/homebrew/opt/postgresql@18/bin}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/pantry-pw-mutations.XXXXXX")" || exit 2

# ---------------------------------------------------------------------------
# THE TRAP, AND WHY THIS FILE NEEDS ONE
# ---------------------------------------------------------------------------
# `restore()` runs on the paths `one_breakage` knows about. An error it does NOT
# know about — a typo in a reporting helper, an unbound variable under `set -u` —
# unwinds straight past it, and the mutation stays in `migrations/`. That is not
# a cosmetic failure: a planted `with check (true)` looks exactly like this
# packet's fix minus its only safety clause, and the next person to read the tree
# cannot tell a reverted mutation from a reverted commit.
#
# It happened here. The first run of this file died on `$frag` where `read` had
# assigned `_frag`, four lines after M1's verdict printed and before its restore,
# and left `migrations/00008_publisher_identity_immutable.sql` carrying
# `with check (true)`. `set -uo pipefail` turned a one-character slip into a dirty
# tree, and only `git status` caught it.
#
# So the exit path re-checks and reverts unconditionally, and says so loudly,
# because a number printed above a crash is not a result.
on_exit() {
  local rc=$? left
  if ! tree_matches_head; then
    printf '\n'
    rule
    say "ABORT  this recipe exited with a mutation still planted."
    git --no-pager diff --stat HEAD -- "${SUBJECT_DIRS[@]}" | sed 's/^/        /'
    git checkout HEAD -- "${SUBJECT_DIRS[@]}" 2>/dev/null
    if tree_matches_head; then
      say "        reverted from HEAD now: ${SUBJECT_DIRS[*]} byte-identical again."
      say "        EVERY NUMBER ABOVE IS VOID. A breakage that was not restored is"
      say "        not a mutation test, and a recipe that died mid-flight proves"
      say "        nothing about the checks it was measuring."
    else
      say "        AND THE REVERT FAILED. Run, by hand:"
      say "          git checkout HEAD -- ${SUBJECT_DIRS[*]}"
    fi
    rc=1
  fi
  left="$(tree_untracked)"
  if [ -n "$left" ]; then
    printf '\n'
    say "note: untracked files remain under ${SUBJECT_DIRS[*]}, and this recipe will"
    say "      not delete them — `git checkout` cannot, and `git clean` on migrations/"
    say "      should be a thing a person types:"
    printf '%s\n' "$left" | sed 's/^/        /'
  fi
  rm -rf "$WORK"
  exit $rc
}
trap on_exit EXIT INT TERM

# THE FIX, AS A LIST. `intended_diff()` asserts this is exactly the diff against
# master, so "my change" and "the mutation" can never be confused: a mutation that
# was not reverted shows up here as an EXTRA file, and a fix that was reverted by
# mistake shows up as a MISSING one.
INTENDED_FILES="migrations/00002_publishers.sql
migrations/00006_rls.sql
migrations/00008_publisher_identity_immutable.sql
migrations/README.md
tests/rls_checks.sh"

PLANTED=0; PLANT_FAILED=0; REDS=0; NOT_RED=0
RESTORED=0; RESTORE_FAILED=0; CONTROLS_GREEN=0; PROBLEMS=0

PHASE="all"
case "${1:-}" in
  --phase) PHASE="${2:-all}" ;;
  --phase=*) PHASE="${1#--phase=}" ;;
  --list) PHASE="list" ;;
  "") ;;
  *) echo "mutations.sh: unknown argument '$1'. --phase control|part1|part2|all | --list" >&2; exit 2 ;;
esac

say()   { printf '%s\n' "$*"; }
rule()  { say "------------------------------------------------------------------------"; }
head2() { say ""; say "== $*"; }

# ---------------------------------------------------------------------------
# replace_once <label> <file> <old> <new>  — exact, and exactly once
# ---------------------------------------------------------------------------
# The count assertion is the whole point. A replacement that matched nothing
# produces a green suite, and a green suite produced by a mutation that never
# applied is worse than no run at all: it reads as a pass.
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
# present in the file the suite is about to read. A count of 0 is a legitimate
# negative confirmation: the line is gone, and its neighbours are not.
grep_confirm() {
  local file="$1" needle="$2" want="$3" label="$4" note="${5:-}" got
  got=$(grep -Fc -- "$needle" "$file" 2>/dev/null || true)
  got=${got:-0}
  if [ "$got" != "$want" ]; then
    say "ABORT  $label: expected $want line(s) matching [$needle] in ${file#$REPO/}, found $got"
    PROBLEMS=$((PROBLEMS+1)); return 1
  fi
  say "        grep-confirmed: $got line(s) match [$needle] in ${file#$REPO/}$note"
  return 0
}

# block_confirm <file> <multi-line text> <want> <label> [note]
# `grep -c` counts LINES, so a multi-line needle never matches one and a
# multi-line confirmation built on it silently confirms nothing. This is literal
# containment, read from disk.
block_confirm() {
  local file="$1" text="$2" want="$3" label="$4" note="${5:-}" got
  got=$(TEXT="$text" FILE="$file" python3 -c '
import os
print(open(os.environ["FILE"], encoding="utf-8").read().count(os.environ["TEXT"]))')
  if [ "$got" != "$want" ]; then
    say "ABORT  $label: expected $want occurrence(s) of the mutated block in ${file#$REPO/}, found $got"
    PROBLEMS=$((PROBLEMS+1)); return 1
  fi
  say "        grep-confirmed: $got occurrence(s) of the mutated block in ${file#$REPO/}$note"
  return 0
}

# tree_matches_head — every TRACKED file under the subject dirs is byte-identical
# to the commit that contains the fix.
#
# TRACKED ONLY, and the second half of this used to also demand that
# `git status --porcelain` be empty, which conflated two different questions and
# could never recover from either. An untracked file under `migrations/` is not a
# mutation — `git checkout HEAD -- migrations` will not remove it, so requiring
# the tree to be free of it means the restore check can report FAILURE after a
# revert that in fact succeeded. That is how the first run of this trap said "AND
# THE REVERT FAILED" about a file it had correctly restored: a stray `.bak` beside
# the migration, from an editor, keeping a boolean false forever.
#
# So the two are separate functions. `tree_untracked` reports the leftovers by
# name instead of folding them into a verdict.
tree_matches_head() {
  local d
  for d in "${SUBJECT_DIRS[@]}"; do
    git diff --quiet HEAD -- "$d" || return 1
  done
  return 0
}

# tree_untracked — untracked files under the subject dirs, if any. Not this
# file's to delete: `git clean` on `migrations/` is a command that should need a
# human to type it. Named and left.
tree_untracked() {
  local d
  for d in "${SUBJECT_DIRS[@]}"; do
    git status --porcelain -- "$d" 2>/dev/null | grep '^??' || true
  done
  return 0
}

# intended_diff — the fix is still exactly the fix.
intended_diff() {
  local want got
  got="$(git diff --name-only master -- "${SUBJECT_DIRS[@]}" | sort)"
  want="$(printf '%s\n' "$INTENDED_FILES" | sort)"
  if [ "$got" != "$want" ]; then
    say "ABORT  the FIX is not what this recipe expects. Expected exactly:"
    printf '%s\n' "$want" | sed 's/^/          /'
    say "        Found:"
    printf '%s\n' "$got" | sed 's/^/          /'
    PROBLEMS=$((PROBLEMS+1)); return 1
  fi
  say "        the fix, unchanged: $(printf '%s' "$got" | grep -c . ) files differ from master —"
  printf '%s\n' "$got" | sed 's/^/          /'
  return 0
}

restore() {
  local label="$1"
  # HEAD, NOT master. master would delete 00008 and the ten checks.
  git checkout HEAD -- "${SUBJECT_DIRS[@]}" 2>/dev/null
  if tree_matches_head; then
    RESTORED=$((RESTORED+1))
    say "        reverted from HEAD; ${SUBJECT_DIRS[*]} byte-identical (git diff --quiet HEAD)"
    return 0
  fi
  RESTORE_FAILED=$((RESTORE_FAILED+1))
  say "ABORT  $label: the tree is still dirty after the revert:"
  git --no-pager diff --stat HEAD -- "${SUBJECT_DIRS[@]}" | sed 's/^/        /'
  PROBLEMS=$((PROBLEMS+1)); return 1
}

# ---------------------------------------------------------------------------
# run_suite <label> — one ./tests/rls.sh, its results TSV kept and parsed
# ---------------------------------------------------------------------------
# Reading the TSV is how this script names WHICH check went red and WHAT refused
# it, rather than reporting "red" — a count with no name attached is not a result.
RUN_LOG=""; RUN_RC=0; RESULTS=""; RUN_TOTAL=0; RUN_PASS=0; RUN_FAIL=0; RUN_SKIP=0
run_suite() {
  local label="$1" kept line
  RUN_LOG="$WORK/$label.log"
  timeout 600 "$SUITE" --keep >"$RUN_LOG" 2>&1
  RUN_RC=$?
  kept=$(sed -n 's/^kept: //p' "$RUN_LOG" | head -1)
  line=$(sed -n 's/^checks: \([0-9]*\) run, \([0-9]*\) passed, \([0-9]*\) failed, \([0-9]*\) skipped.*/\1 \2 \3 \4/p' "$RUN_LOG" | head -1)
  if [ -z "$line" ]; then
    say "ABORT  $label: tests/rls.sh printed no 'checks:' line (exit $RUN_RC)."
    say "        A suite that never ran is not a green suite and is not a red one."
    tail -20 "$RUN_LOG" | sed 's/^/        /'
    PROBLEMS=$((PROBLEMS+1)); RUN_FAIL=-1; RESULTS=""; return 1
  fi
  # shellcheck disable=SC2086
  set -- $line
  RUN_TOTAL=$1; RUN_PASS=$2; RUN_FAIL=$3; RUN_SKIP=$4
  say "        tests/rls.sh -> $RUN_TOTAL run, $RUN_PASS passed, $RUN_FAIL failed, $RUN_SKIP skipped (exit $RUN_RC)"
  # The TSV is COPIED OUT before the cluster's workdir goes. Reading it after
  # `rm -rf` classifies an empty string as "nothing failed", which is how a run
  # with reds prints "NOT RED AT ALL" on its way to calling a widening
  # undetectable. Every verdict below comes from this copy.
  if [ -n "$kept" ] && [ -f "$kept/results.tsv" ]; then
    cp "$kept/results.tsv" "$WORK/$label.results.tsv"
    RESULTS="$WORK/$label.results.tsv"
  else
    RESULTS=""
    say "ABORT  $label: tests/rls.sh kept no results.tsv, so no red can be named."
    PROBLEMS=$((PROBLEMS+1)); [ -n "$kept" ] && rm -rf "$kept"; return 1
  fi
  rm -rf "$kept"
  return 0
}

# red_names — the NAMES of the reds, which is the part worth reading.
red_names() {
  [ -n "$RESULTS" ] || { say "          (no results)"; return 0; }
  awk -F'\t' '$2=="FAIL" {print "          " $1}' "$RESULTS"
}

# is_ours <name> — is this one of the TEN checks this packet added?
# D18..D23, E13..E15, F10.
is_ours() {
  printf '%s' "$1" | grep -Eq '^(D1[89]|D2[0-3]|E1[345]|F10) '
}

# tier_of <name> — the tier a check name belongs to.
tier_of() {
  case "$1" in
    D*) printf 'D' ;;
    E*) printf 'E' ;;
    F*) printf 'F' ;;
    *)  printf '?' ;;
  esac
}

# new_reds — which of THIS PACKET's ten checks went red, and what each said.
new_reds() {
  [ -n "$RESULTS" ] || return 0
  while IFS=$'\t' read -r name status frag obs; do
    [ "$status" = "FAIL" ] || continue
    if is_ours "$name"; then say "          $name  <-  $frag"; fi
  done <"$RESULTS"
}

# collateral_reds — reds in checks this packet did NOT add.
#
# These are reported and named, but they are kept OUT of the verdict. The verdict
# is "did the ten new checks catch this", and a pre-existing check going red for a
# knock-on reason does not strengthen or weaken that answer — it is a separate
# observation about blast radius. E2 is the one that shows up: with the barrier
# gone, a publisher's rename lands, and `pantry_admin` then reads a different
# login list. That the hole reaches outside the publisher's own row is worth
# knowing, and it is not what M1 is testing.
collateral_reds() {
  [ -n "$RESULTS" ] || return 0
  local found=0 name status
  while IFS=$'\t' read -r name status _frag _obs; do
    [ "$status" = "FAIL" ] || continue
    if ! is_ours "$name"; then say "          COLLATERAL (not one of ours): $name"; found=1; fi
  done <"$RESULTS"
  [ "$found" -eq 1 ] || say "          (no collateral reds)"
  return 0
}

# tier_reds — which tiers THIS PACKET'S checks went red in, comma-separated.
#
# `-F'\t'` on BOTH awks. The first one's `$1` is the whole check name; without it
# on the second, `$1` was the first whitespace-token — "F10" — and every regex in
# this function asked for a trailing space that a one-token field does not have.
# The result was a verdict of `other:F10` for a check that had in fact gone red,
# which is the failure mode a recipe exists to prevent: a red reported as a
# mismatch, and a mismatch read as "the check does not work".
tier_reds() {
  [ -n "$RESULTS" ] || { echo "?"; return 0; }
  local name status out=""
  while IFS=$'\t' read -r name status _frag _obs; do
    [ "$status" = "FAIL" ] || continue
    is_ours "$name" || continue
    out="$out $(tier_of "$name")"
  done <"$RESULTS"
  [ -n "$out" ] || { echo "NONE"; return 0; }
  printf '%s' "$out" | tr ' ' '\n' | grep -v '^$' | sort -u | paste -sd, -
}

# ---------------------------------------------------------------------------
# THE MUTATIONS
# ---------------------------------------------------------------------------

# M1 — the barrier deleted. `with check (false)` -> `with check (true)`.
m1() {
  replace_once "M1" "$M8" \
'  -- bypasses the boundary it wrote (D31), and `tests/rls.sh` F10 executes it as
  -- the table owner, because the D tier structurally cannot see that exemption.
  with check (false);' \
'  -- bypasses the boundary it wrote (D31), and `tests/rls.sh` F10 executes it as
  -- the table owner, because the D tier structurally cannot see that exemption.
  with check (true);' || return 1
  block_confirm "$M8" 'with check (true);' 1 "M1" "  [the barrier is gone]" || return 1
  block_confirm "$M8" 'with check (false);' 0 "M1" "  [and 00008 no longer contains it anywhere]" || return 1
  say "        M1: with check (false) -> with check (true). The barrier deleted.  [predicts D, F]"
}

# M2 — the barrier narrowed to the clause that shipped. 00006's own `with check`.
m2() {
  replace_once "M2" "$M8" '  with check (false);' \
'  with check (id = (select pantry.current_publisher_id()));' || return 1
  block_confirm "$M8" '  with check (id = (select pantry.current_publisher_id()));
' 1 "M2" "  [00006's clause, back in the Up]" || return 1
  block_confirm "$M8" '  with check (false);' 0 "M2" || return 1
  say "        M2: with check (false) -> 00006's 'id = current_publisher_id()'."
  say "            A barrier that pins the PRIMARY KEY. It refuses every statement"
  say "            these checks make, and it is what actually shipped.  [predicts D, F]"
}

# M3 — FORCE dropped from publishers. The policy is untouched.
m3() {
  replace_once "M3" "$M8" \
'drop policy if exists publishers_publisher_update on pantry.publishers;

create policy publishers_publisher_update
  on pantry.publishers
  for update
  to pantry_publisher, pantry
  -- THE `using` CLAUSE STAYS' \
'alter table pantry.publishers no force row level security;

drop policy if exists publishers_publisher_update on pantry.publishers;

create policy publishers_publisher_update
  on pantry.publishers
  for update
  to pantry_publisher, pantry
  -- THE `using` CLAUSE STAYS' || return 1
  grep_confirm "$M8" 'alter table pantry.publishers no force row level security;' 1 "M3" || return 1
  block_confirm "$M8" '  with check (false);' 1 "M3" "  [the policy clause is UNCHANGED — only FORCE moved]" || return 1
  say "        M3: 'no force row level security' on publishers. Policy byte-identical."
  say "            The owner stops being subject to it; the publisher role is not the"
  say "            owner and never was.  [predicts F ONLY — this is the D-blindness proof]"
}

# M4 — the over-refusal. The pin applied to the wrong set of roles: the admin's
# own update policy is narrowed to the same `false`.
m4() {
  replace_once "M4" "$M6" \
'create policy publishers_admin_update
  on pantry.publishers for update to pantry_admin
  using (true) with check (true);' \
'create policy publishers_admin_update
  on pantry.publishers for update to pantry_admin
  using (true) with check (false);' || return 1
  grep_confirm "$M6" 'using (true) with check (false);' 1 "M4" || return 1
  block_confirm "$M8" '  with check (false);' 1 "M4" "  [00008 is untouched — this is 00006's admin policy]" || return 1
  say "        M4: publishers_admin_update's 'with check (true)' -> 'false'."
  say "            The fleet's own rename, OAuth verify and promote stop working."
  say "            00008 is untouched.  [predicts E ONLY — the over-refusal check]"
}

# M5 — the policy deleted outright. The honest control on M1's verdict.
m5() {
  delete_once "M5" "$M8" \
'create policy publishers_publisher_update
  on pantry.publishers
  for update
  to pantry_publisher, pantry
  -- THE `using` CLAUSE STAYS, and it is the half of the rule that is still
  -- doing work: it is what makes this "the publisher may write to its OWN row"
  -- rather than "the publisher may write to every row and be refused", and it is
  -- the half that scopes `pantry`'"'"'s maintenance reads to one publisher. `false`
  -- alone would be an equally strong statement and a less informative one.
  using (id = (select pantry.current_publisher_id()))
  -- THE BARRIER. `false` refuses every resulting row, so every UPDATE by
  -- `pantry_publisher` and by the owner `pantry` fails with SQLSTATE 42501
  -- "new row violates row-level security policy for table \"publishers\"" — a
  -- POLICY refusal, attributable to this clause, rather than a unique-index
  -- collision that attributes it to arithmetic about somebody else'"'"'s row.
  --
  -- Both roles are named, and that is inherited from `00006` rather than
  -- chosen here. It is what makes the owner'"'"'s maintenance obey the same rule as
  -- everybody else'"'"'s instead of being the one identity in the schema that
  -- bypasses the boundary it wrote (D31), and `tests/rls.sh` F10 executes it as
  -- the table owner, because the D tier structurally cannot see that exemption.
  with check (false);
' || return 1
  block_confirm "$M8" '  with check (false);' 0 "M5" "  [the barrier clause is gone from the file entirely]" || return 1
  block_confirm "$M8" 'create policy publishers_publisher_update
  on pantry.publishers
  for update' 1 "M5" "  [ONE left, and it is 00008's Down — which is correct: the Down must recreate 00006's policy]" || return 1
  grep_confirm "$M8" 'drop policy if exists publishers_publisher_update' 2 "M5" "  [Up and Down both still drop it]" || return 1
  say "        M5: 00008's Up create policy DELETED — no UPDATE policy on publishers at all."
  say "            Nothing becomes writable: RLS with no matching UPDATE policy FILTERS"
  say "            rather than raising, so the answer is 'UPDATE 0' at exit 0 and the"
  say "            denial is unnameable.  [predicts D,F — the MECHANISM, not the security]"
}

# ---------------------------------------------------------------------------
# one_breakage <id> <plant fn> <predicted tiers> <what it was written for>
# ---------------------------------------------------------------------------
one_breakage() {
  local id="$1" plant="$2" predicted="$3" written_for="$4"
  head2 "$id  —  $written_for"
  PLANTED=$((PLANTED+1))
  if ! "$plant"; then
    PLANT_FAILED=$((PLANT_FAILED+1))
    say "ABORT  $id: the mutation did not apply exactly once. NOT running the suite —"
    say "        a green over a mutation that landed nowhere reads as a pass."
    restore "$id"; return 1
  fi
  run_suite "$id" || { restore "$id"; return 1; }
  local got; got="$(tier_reds)"
  say "        tiers that went red (ours): $got   (predicted: $predicted)"
  new_reds
  say "        knock-on reds outside this packet:"
  collateral_reds
  if [ "$got" = "$predicted" ]; then
    REDS=$((REDS+1))
    say "        VERDICT: as predicted. $predicted"
  else
    NOT_RED=$((NOT_RED+1))
    say "        VERDICT: MISMATCH. predicted [$predicted], got [$got]"
    PROBLEMS=$((PROBLEMS+1))
  fi
  restore "$id" || return 1
  # Re-run green after every revert. A breakage that is red and never restored is
  # not a mutation test, it is vandalism, and the only cheap proof that the
  # restore took is to make the suite say so again.
  run_suite "$id-restored" || return 1
  if [ "$RUN_FAIL" -ne 0 ]; then
    say "ABORT  $id: the suite is still red AFTER the revert. The tree did not come back."
    red_names "$id-restored"
    PROBLEMS=$((PROBLEMS+1)); return 1
  fi
  CONTROLS_GREEN=$((CONTROLS_GREEN+1))
  say "        restored: $RUN_TOTAL run, $RUN_PASS passed, 0 failed — green again"
  return 0
}

do_control() {
  head2 "CONTROL — untouched tree, with this packet's fix in place"
  intended_diff || return 1
  if ! tree_matches_head; then
    say "ABORT  the working tree differs from HEAD before the recipe started."
    git --no-pager diff --stat HEAD | sed 's/^/        /'
    PROBLEMS=$((PROBLEMS+1)); return 1
  fi
  run_suite "control-$(date +%s%N)" || return 1
  if [ "$RUN_FAIL" -ne 0 ]; then
    say "ABORT  the control is NOT green ($RUN_FAIL failed). If the control is not"
    say "        green, stop: breakages measured against a red baseline measure nothing."
    red_names "control"; PROBLEMS=$((PROBLEMS+1)); return 1
  fi
  CONTROLS_GREEN=$((CONTROLS_GREEN+1))
  say "        control GREEN: $RUN_TOTAL run, $RUN_PASS passed, $RUN_FAIL failed, $RUN_SKIP skipped"
  return 0
}

# ONE FUNCTION PER MUTATION, named for the M-id and nothing else.
#
# These were four `do_partN` functions until the fifth mutation was added and the
# numbering slipped by one, so `--phase part5` called a function that did not
# exist, the shell said `do_part5: command not found`, and the recipe carried on
# and printed `ok` — having planted zero mutations. Two separate bugs, and the
# second is the one that matters: nothing about a crash looks like a pass until
# you check that anything ran.
run_m1() { one_breakage M1 m1 "D,F" "the barrier deleted"; }
run_m2() { one_breakage M2 m2 "D,F" "the barrier narrowed to 00006's clause"; }
run_m3() { one_breakage M3 m3 "F" "FORCE dropped — the D-blindness proof"; }
run_m4() { one_breakage M4 m4 "E" "the over-refusal — the pin on the wrong roles"; }
run_m5() { one_breakage M5 m5 "D,F" "the policy deleted — the security holds, the attribution does not"; }

do_final() {
  head2 "FINAL CONTROL — after every revert"
  intended_diff || return 1
  tree_matches_head || { say "ABORT  migrations/ or tests/ is not byte-identical to HEAD."; PROBLEMS=$((PROBLEMS+1)); return 1; }
  run_suite "final-$(date +%s%N)" || return 1
  if [ "$RUN_FAIL" -ne 0 ]; then
    say "ABORT  the final control is NOT green. The recipe did not restore the tree."
    red_names "final"; PROBLEMS=$((PROBLEMS+1)); return 1
  fi
  CONTROLS_GREEN=$((CONTROLS_GREEN+1))
  say "        final control GREEN: $RUN_TOTAL run, $RUN_PASS passed, $RUN_FAIL failed, $RUN_SKIP skipped"
  return 0
}

say "pantry-publisher-rewrite-01 / mutations — can the ten new checks be made to fail?"
say "repo:   $REPO"
say "branch: $(git rev-parse --abbrev-ref HEAD) @ $(git rev-parse --short HEAD)"
say "note:   the FIX lives in this branch's commits. Mutations are uncommitted edits on"
say "        top of it, reverted from HEAD after each one. restore() checks out HEAD,"
say "        never master."

if [ "$PHASE" = "list" ]; then
  rule
  say "phase control  the untouched tree, fix in place"
  say "phase part1    M1 with check (false) -> (true)            predicts D,F"
  say "               M2 -> 00006's 'id = current_publisher_id'  predicts D,F"
  say "phase part2    M3 no force row level security             predicts F"
  say "phase part3    M4 publishers_admin_update check (true)->(false)  predicts E"
  say "phase part4    M5 00008's create policy deleted           predicts D,F (silent UPDATE 0)"
  exit 0
fi

rule
do_control || exit 1
[ "$PHASE" = "control" ] && { rule; say "phase control done."; exit 0; }

case "$PHASE" in
  all)     run_m1; run_m2; run_m3; run_m4; run_m5 ;;
  part1)   run_m1; run_m2 ;;
  part2)   run_m3 ;;
  part3)   run_m4 ;;
  part4)   run_m5 ;;
  *) echo "unknown phase '$PHASE'" >&2; exit 2 ;;
esac
do_final || exit 1

rule
say "mutations planted:      $PLANTED   (failed to apply: $PLANT_FAILED)"
say "verdicts as predicted:  $REDS      (mismatched: $NOT_RED)"
say "green controls:         $CONTROLS_GREEN"
say "reverts verified:       $RESTORED  (failed: $RESTORE_FAILED)"
say "problems:               $PROBLEMS"
say ""
# `PLANTED -gt 0` IS THE CONDITION AND IT IS NOT OPTIONAL. It was left out of the
# first version of this recipe, which meant a run that planted nothing satisfied
# `REDS -eq PLANTED` at 0 == 0 and printed `ok` — so a phase whose mutation
# function did not exist (`do_part5: command not found`, which the shell reports
# to stderr and this summary never read) ended in a green verdict. The exact
# failure this file exists to prevent, one level up: a pass manufactured by
# nothing having been measured.
if [ "$PLANTED" -eq 0 ] && [ "$PHASE" != "control" ]; then
  say "NOT ok — ZERO mutations were planted, so nothing was measured."
  say "         A summary that says ok because an empty run compared equal to"
  say "         itself is the defect, not the result."
  exit 1
fi
if [ "$PROBLEMS" -eq 0 ] && [ "$REDS" -eq "$PLANTED" ] && [ "$RESTORE_FAILED" -eq 0 ] \
   && [ "$PLANT_FAILED" -eq 0 ]; then
  say "ok — the checks hold: every breakage applied exactly once, went red in the tier"
  say "     it was predicted to break, was reverted, and the suite said so again."
  exit 0
fi
say "NOT ok — read the ABORT lines above before believing any of this."
exit 1