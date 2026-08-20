#!/usr/bin/env bash
set -euo pipefail

# Enforce the line-coverage floor from an lcov report, and write the receipt.
#
# Usage: scripts/check-coverage.sh [lcov-file]
#
# The floor lives in scripts/coverage-floor.txt and is raised by
# `just coverage-ratchet`, never by hand and never downward. On success this
# writes scripts/coverage-receipt.txt — the record that coverage was measured,
# at which commit, and what it found. `check-coverage-freshness.sh` reads that
# receipt inside `just check`, so this slow measurement is what clears the fast
# gate; without a receipt, commits are blocked once the drift bound is passed.
#
# lcov `DA:` records are `DA:<line>,<hit-count>[,<checksum>]`; a line counts as
# covered when the hit count is non-zero. The whole computation is one awk pass
# so there is no pipeline whose exit status or output shape can be misread —
# `grep -c | grep -v` pipes a *count* into a filter and makes the covered figure
# a constant 1, which is unpassable and unfailable at the same time.

. "$(dirname "$0")/common.sh"

here="$(dirname "$0")"
LCOV_FILE="${1:-lcov.info}"
FLOOR_FILE="${INSPECT_COVERAGE_FLOOR_FILE:-$here/coverage-floor.txt}"
RECEIPT="${INSPECT_COVERAGE_RECEIPT:-$here/coverage-receipt.txt}"
TARGET=95

[ -f "$LCOV_FILE" ] || fail "$LCOV_FILE not found — run 'just coverage' first"
[ -f "$FLOOR_FILE" ] || fail "coverage floor file not found: $FLOOR_FILE"

FLOOR="$(receipt_line "$FLOOR_FILE")"
require_int "$FLOOR" "coverage floor in $FLOOR_FILE"

read -r covered total < <(
    awk -F, '
        /^DA:/ { total++; if ($2 + 0 > 0) covered++ }
        END { printf "%d %d\n", covered + 0, total + 0 }
    ' "$LCOV_FILE"
)

[ "$total" -gt 0 ] || fail "no DA: records in $LCOV_FILE — coverage data is missing or empty"

pct=$((covered * 100 / total))
echo "Coverage: ${pct}% (${covered}/${total} lines), floor ${FLOOR}%, target ${TARGET}%"

[ "$pct" -ge "$FLOOR" ] || fail "coverage ${pct}% is below the ${FLOOR}% floor"
[ "$pct" -ge "$TARGET" ] || echo "WARN:  coverage ${pct}% is below the ${TARGET}% target"

# Receipt last, so a run that fails the floor leaves the old one in place and
# the freshness gate keeps counting drift against the last measurement that
# actually passed.
#
# A receipt names the commit it measured, so there has to be one. On an unborn
# branch there is not, and writing the receipt anyway would put the literal
# string "HEAD" where a commit belongs — a receipt that pins its own drift at
# zero forever and can never go stale.
sha="$(head_commit)"
[ -n "$sha" ] ||
    fail "coverage cleared the floor, but this branch has no commit to record it against.
       Commit first, then re-run — a receipt that names no commit can never go stale."

{
    echo "# Coverage receipt — written by 'just coverage-check'. Do not hand-edit."
    echo "# Read by scripts/check-coverage-freshness.sh, which runs inside 'just check'."
    echo "# Fields: <commit> <percent> <measured-at>"
    echo "$sha $pct $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$RECEIPT"
echo "Receipt written: $RECEIPT"
