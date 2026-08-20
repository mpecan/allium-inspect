#!/usr/bin/env bash
set -euo pipefail

# Mutation debt gate — sub-second, runs inside `just check`.
#
# Mutation testing is the strongest signal this repo has: coverage says a line
# ran, mutation asks whether any assertion would have noticed it changing. It is
# also minutes per run, so running it on every commit is not honest either — it
# would be skipped, and a gate people skip is a gate that cannot fail.
#
# So mutation is a decision, and this gate is the ceiling on how long that
# decision can be deferred. It measures how much Rust has changed since the last
# recorded mutation run and escalates:
#
#   under WARN lines            silent
#   WARN..FAIL lines            warns, naming the figure and the recipe
#   over FAIL lines             blocks, until 'just mutants' writes a receipt
#   over MAX_COMMITS commits    blocks, for the same reason
#
# Lines changed, not commits alone, because the two measure different risks: a
# hundred commits of renames carry less untested behaviour than one commit that
# adds an evaluator arm. The commit bound is a backstop for the case where a
# small, dense change sits unmutated for a long time.
#
# Deletions count alongside additions. Removing a branch changes what the suite
# covers just as surely as adding one, and a refactor that deletes half a module
# is exactly when the remaining assertions most deserve re-checking.

. "$(dirname "$0")/common.sh"

here="$(dirname "$0")"
RECEIPT="${INSPECT_MUTATION_RECEIPT:-$here/mutation-receipt.txt}"
WARN_LINES="${INSPECT_MUTATION_WARN_LINES:-250}"
FAIL_LINES="${INSPECT_MUTATION_FAIL_LINES:-500}"
MAX_COMMITS="${INSPECT_MUTATION_MAX_COMMITS:-20}"

require_int "$WARN_LINES" "INSPECT_MUTATION_WARN_LINES"
require_int "$FAIL_LINES" "INSPECT_MUTATION_FAIL_LINES"
require_int "$MAX_COMMITS" "INSPECT_MUTATION_MAX_COMMITS"
[ "$WARN_LINES" -le "$FAIL_LINES" ] ||
    fail "INSPECT_MUTATION_WARN_LINES ($WARN_LINES) must not exceed INSPECT_MUTATION_FAIL_LINES ($FAIL_LINES)"

[ -f "$RECEIPT" ] ||
    fail "mutation receipt not found: $RECEIPT — run 'just mutants' to establish the baseline"

read -r sha score survivors measured_at <<<"$(receipt_line "$RECEIPT")"
[ -n "${sha:-}" ] && [ -n "${score:-}" ] && [ -n "${survivors:-}" ] && [ -n "${measured_at:-}" ] ||
    fail "mutation receipt $RECEIPT is malformed — run 'just mutants'"
require_sha "$sha" "mutation receipt commit"
require_int "$survivors" "mutation receipt survivor count"

git cat-file -e "$sha^{commit}" 2>/dev/null ||
    fail "mutation receipt names commit $sha, which is not in this repository"

lines="$(rust_lines_changed_since "$sha")"
commits="$(rust_commits_since "$sha")"
require_int "$lines" "computed line delta"

summary="last run: $score caught, $survivors survivors at ${sha:0:12} on $measured_at"

if [ "$lines" -gt "$FAIL_LINES" ]; then
    fail "$lines Rust lines have changed since the last mutation run (limit $FAIL_LINES)
       $summary
       Run 'just mutants' — it is diff-scoped against that commit, so it costs
       minutes, not hours, and it refreshes this receipt."
fi

if [ "$commits" -gt "$MAX_COMMITS" ]; then
    fail "$commits Rust-touching commits since the last mutation run (limit $MAX_COMMITS)
       $summary
       Run 'just mutants' to refresh the receipt."
fi

if [ "$lines" -gt "$WARN_LINES" ]; then
    echo "WARN:  mutation debt: $lines Rust lines changed since ${sha:0:12} (blocks at $FAIL_LINES)"
    echo "       $summary — 'just mutants' clears it."
    exit 0
fi

echo "Mutation debt: $lines/$FAIL_LINES lines, $commits/$MAX_COMMITS commits since ${sha:0:12}"
