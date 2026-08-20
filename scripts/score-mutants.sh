#!/usr/bin/env bash
set -euo pipefail

# Score a finished mutation run: enforce the survivor baseline, ratchet it, and
# write the receipt the debt gate reads.
#
# Separate from the run itself because the run takes tens of minutes and this
# takes milliseconds — and because this half is the half that decides. A gate
# that can only be exercised by a half-hour job is a gate nobody drives in both
# directions, and the sister repo's history is three gates that shipped
# green-forever for exactly that reason. Split out, it is driven by
# gates-selftest.sh against fixture results in a scratch directory.
#
#   score-mutants.sh <results-dir> <baseline-file> <receipt-file>
#
# <results-dir> holds cargo-mutants' caught.txt and missed.txt.
#
# The baseline ratchets downward only. A run with more survivors than the
# baseline fails; a run with fewer rewrites it. Accepting a rise has to be a
# deliberate edit of that file with a reason, not a number that drifts up
# unnoticed one commit at a time.

. "$(dirname "$0")/common.sh"

[ "$#" -eq 3 ] || fail "usage: score-mutants.sh <results-dir> <baseline-file> <receipt-file>"
out="$1"
baseline="$2"
receipt="$3"

caught_file="$out/caught.txt"
missed_file="$out/missed.txt"
[ -f "$caught_file" ] || fail "$caught_file not found — the run produced no results to score"

caught=$(wc -l < "$caught_file" | tr -d ' ')
survivors=0
[ -f "$missed_file" ] && survivors=$(wc -l < "$missed_file" | tr -d ' ')
total=$((caught + survivors))
[ "$total" -gt 0 ] || fail "the run scored 0 mutants — that is a broken run, not a clean one"

echo "Mutants: $caught caught, $survivors survived, $total total"

write_baseline() {
    printf '# Surviving mutants allowed. Ratchets downward only.\n%s\n' "$1" > "$baseline"
}

if [ -f "$baseline" ]; then
    allowed="$(receipt_line "$baseline")"
    require_int "$allowed" "survivor baseline in $baseline"
    if [ "$survivors" -gt "$allowed" ]; then
        echo "Survivors:"
        sed 's/^/  /' "$missed_file"
        fail "$survivors mutants survived, baseline allows $allowed
       Each survivor is a change to the code that no assertion noticed.
       Write the assertion, or raise the baseline deliberately with a reason."
    fi
    if [ "$survivors" -lt "$allowed" ]; then
        write_baseline "$survivors"
        echo "Survivor baseline lowered: $allowed -> $survivors"
    fi
else
    write_baseline "$survivors"
    echo "Survivor baseline established at $survivors"
fi

# See check-coverage.sh: a receipt naming no commit pins its own drift at zero.
sha="$(head_commit)"
[ -n "$sha" ] ||
    fail "the mutation run completed, but this branch has no commit to record it against.
       Commit first, then re-run."

{
    echo "# Mutation receipt — written by 'just mutants'. Do not hand-edit."
    echo "# Read by scripts/check-mutation-debt.sh, which runs inside 'just check'."
    echo "# Fields: <commit> <caught>/<total> <survivors> <measured-at>"
    echo "$sha $caught/$total $survivors $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$receipt"
echo "Receipt written: $receipt"
