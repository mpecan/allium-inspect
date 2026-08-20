#!/usr/bin/env bash
set -euo pipefail

# Raise the coverage floor toward the 95% target. Never lowers it.
#
# The floor starts at 85 and moves up only when a measurement clears it by the
# slack margin, so the bar tracks what the suite actually achieves without a
# single lucky run pinning a floor the next commit cannot meet. The cap is the
# target: past 95 the remaining lines are error paths whose cost to reach
# exceeds what asserting them buys.
#
# Ratcheting is a separate command, not a side effect of measuring. A floor that
# rises automatically inside `coverage-check` would move the bar in the middle
# of an unrelated run, and the commit that raised it would not be the commit
# that shows it.

. "$(dirname "$0")/common.sh"

here="$(dirname "$0")"
LCOV_FILE="${1:-lcov.info}"
FLOOR_FILE="${INSPECT_COVERAGE_FLOOR_FILE:-$here/coverage-floor.txt}"
TARGET=95
SLACK=2

[ -f "$LCOV_FILE" ] || fail "$LCOV_FILE not found — run 'just coverage' first"
[ -f "$FLOOR_FILE" ] || fail "coverage floor file not found: $FLOOR_FILE"

FLOOR="$(receipt_line "$FLOOR_FILE")"
require_int "$FLOOR" "coverage floor in $FLOOR_FILE"

read -r covered total < <(
    awk -F, '/^DA:/ { total++; if ($2 + 0 > 0) covered++ } END { printf "%d %d\n", covered + 0, total + 0 }' "$LCOV_FILE"
)
[ "$total" -gt 0 ] || fail "no DA: records in $LCOV_FILE"
pct=$((covered * 100 / total))

if [ "$FLOOR" -ge "$TARGET" ]; then
    echo "Coverage floor is already at the ${TARGET}% target — nothing to ratchet."
    exit 0
fi

candidate=$((pct - SLACK))
[ "$candidate" -le "$TARGET" ] || candidate="$TARGET"

if [ "$candidate" -le "$FLOOR" ]; then
    echo "Coverage ${pct}% does not clear the ${FLOOR}% floor by ${SLACK} points — floor unchanged."
    exit 0
fi

{
    echo "# The enforced line-coverage floor, in percent."
    echo "# Raised by 'just coverage-ratchet' when a measurement clears it by 2 points."
    echo "# Never lowered: dropping the floor to make a red run green defeats the gate."
    echo "$candidate"
} > "$FLOOR_FILE"
echo "Coverage floor raised: ${FLOOR}% -> ${candidate}% (measured ${pct}%, target ${TARGET}%)"
