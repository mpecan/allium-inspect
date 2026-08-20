#!/usr/bin/env bash
set -euo pipefail

# File size budget: 500 source lines warns, 700 fails.
#
# Not a style preference. The two pure crates are held to 95% line coverage and
# to a mutation baseline, and both get harder per file as a file grows: a
# 900-line module has too many paths for its tests to be read against it. The
# budget is the pressure that keeps modules small enough to test exhaustively.
#
# Test modules are exempt (see `count_source_lines`) — a thorough test module is
# the goal, not a cost.

. "$(dirname "$0")/common.sh"

SOFT="${INSPECT_FILE_SOFT:-500}"
HARD="${INSPECT_FILE_HARD:-700}"
require_int "$SOFT" "INSPECT_FILE_SOFT"
require_int "$HARD" "INSPECT_FILE_HARD"

status=0
warned=0
checked=0

# Fed by process substitution, not a pipe. `find | while read` runs the body in
# a subshell, so `status=1` is assigned to a copy and discarded — the gate then
# always exits 0. That exact shape shipped green for a year in a sister repo.
while IFS= read -r file; do
    checked=$((checked + 1))
    lines="$(count_source_lines "$file")"
    if [ "$lines" -gt "$HARD" ]; then
        echo "FAIL: $file has $lines source lines (hard limit $HARD)"
        status=1
    elif [ "$lines" -gt "$SOFT" ]; then
        echo "WARN: $file has $lines source lines (soft limit $SOFT)"
        warned=$((warned + 1))
    fi
done < <(rust_files)

[ "$checked" -gt 0 ] || fail "no Rust sources found — the gate matched nothing and would pass regardless"

if [ "$status" -eq 0 ]; then
    echo "File sizes: $checked files, $warned over the ${SOFT}-line soft limit, none over ${HARD}"
fi
exit "$status"
