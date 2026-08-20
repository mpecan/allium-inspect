#!/usr/bin/env bash
set -euo pipefail

# Coverage freshness gate — sub-second, runs inside `just check`.
#
# This does NOT measure coverage. It asserts only that coverage *was* measured
# recently and passed: `just coverage-check` writes a receipt naming the commit
# it measured and the percentage it found, and this fails once HEAD has drifted
# more than INSPECT_COVERAGE_MAX_COMMITS Rust-touching commits past it.
#
# Why not run the measurement itself here: it needs a separately instrumented
# build of the whole workspace, which is minutes on top of a `check` that aims
# to stay under one. Bounding the drift is the enforcement that actually fires —
# the measurement stays out of the hot path, and the commit that exceeds the
# bound is blocked until someone runs it.
#
# So `check` says *coverage was measured recently and cleared the floor*, not
# *coverage passes now*. The bound is what keeps that gap small.

. "$(dirname "$0")/common.sh"

here="$(dirname "$0")"
MAX_COMMITS="${INSPECT_COVERAGE_MAX_COMMITS:-10}"
RECEIPT="${INSPECT_COVERAGE_RECEIPT:-$here/coverage-receipt.txt}"
FLOOR_FILE="${INSPECT_COVERAGE_FLOOR_FILE:-$here/coverage-floor.txt}"

require_int "$MAX_COMMITS" "INSPECT_COVERAGE_MAX_COMMITS"
[ -f "$RECEIPT" ] || fail "coverage receipt not found: $RECEIPT — run 'just coverage-check'"
[ -f "$FLOOR_FILE" ] || fail "coverage floor file not found: $FLOOR_FILE"

FLOOR="$(receipt_line "$FLOOR_FILE")"
require_int "$FLOOR" "coverage floor in $FLOOR_FILE"

read -r sha pct measured_at <<<"$(receipt_line "$RECEIPT")"
[ -n "${sha:-}" ] && [ -n "${pct:-}" ] && [ -n "${measured_at:-}" ] ||
    fail "coverage receipt $RECEIPT is malformed — run 'just coverage-check'"
require_int "$pct" "coverage receipt percentage"
require_sha "$sha" "coverage receipt commit"

# A receipt is a claim that coverage was measured *and passed*. Re-checked here
# because the receipt is a file: a bad merge or a truncated write can leave a
# shape check-coverage.sh would never have produced. It is also what makes the
# ratchet bite — raising the floor immediately invalidates an older receipt that
# only cleared the previous one.
[ "$pct" -ge "$FLOOR" ] ||
    fail "coverage receipt records ${pct}%, below the current ${FLOOR}% floor — run 'just coverage-check'"

git cat-file -e "$sha^{commit}" 2>/dev/null ||
    fail "coverage receipt names commit $sha, which is not in this repository"

drift="$(rust_commits_since "$sha")"
if [ "$drift" -gt "$MAX_COMMITS" ]; then
    fail "coverage was last measured $drift Rust-touching commits ago (limit $MAX_COMMITS)
       Receipt: ${pct}% at ${sha:0:12} on $measured_at
       Run 'just coverage-check' to measure and refresh it."
fi

echo "Coverage freshness: ${pct}% at ${sha:0:12}, $drift/$MAX_COMMITS Rust commits of drift"
