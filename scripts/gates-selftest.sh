#!/usr/bin/env bash
set -euo pipefail

# Self-test for the gate scripts: every gate is driven in both directions.
#
# A gate that cannot fail is worse than no gate — it reports success and
# suppresses scrutiny. These shapes all shipped green for their whole lifetime
# in a sister repo:
#
#   find … | while read; do status=1; done   the body is a subshell; the
#                                            assignment is discarded, exit 0
#   covered=$(grep -c '^DA:' f | grep -v ,0) a *count* piped into a filter;
#                                            the value is always 1
#   grep -rql                                -q suppresses the list -l asks for
#
# So each gate here is given an input that must fail, and the failure is
# asserted; then it is given a clean input, and the pass is asserted. A gate
# change is not done until this has been seen to fail.
#
# Runs against scratch fixtures in a temp directory. Nothing here touches the
# repository's own receipts.

here="$(cd "$(dirname "$0")" && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

checks=0
failures=0
skipped=0

# Some gates cannot be exercised before the repository has a commit: a receipt
# names the commit it measured, so on an unborn branch there is nothing to
# stamp. Those checks are reported as skipped rather than quietly dropped —
# a self-test that silently tests less than it claims is the same failure mode
# as a gate that cannot fail.
skip() {
    skipped=$((skipped + 1))
    echo "skip: $1 (no commit on this branch yet)"
}

# Assert a command exits non-zero, and that its output names the reason.
expect_fail() {
    local what="$1" want="$2"; shift 2
    checks=$((checks + 1))
    local out status
    out="$("$@" 2>&1)" && status=0 || status=$?
    if [ "$status" -eq 0 ]; then
        echo "FAIL: $what — expected a non-zero exit, got 0"
        echo "$out" | sed 's/^/      /'
        failures=$((failures + 1))
    elif ! printf '%s' "$out" | grep -q "$want"; then
        echo "FAIL: $what — exited $status but did not mention '$want'"
        echo "$out" | sed 's/^/      /'
        failures=$((failures + 1))
    else
        echo "ok:   $what fails as it should"
    fi
}

expect_pass() {
    local what="$1"; shift
    checks=$((checks + 1))
    local out status
    out="$("$@" 2>&1)" && status=0 || status=$?
    if [ "$status" -ne 0 ]; then
        echo "FAIL: $what — expected exit 0, got $status"
        echo "$out" | sed 's/^/      /'
        failures=$((failures + 1))
    else
        echo "ok:   $what passes on clean input"
    fi
}

. "$here/common.sh"
head_sha="$(head_commit)"
stamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# --- coverage floor -------------------------------------------------------
printf '85\n' > "$scratch/floor.txt"

# An lcov report where 1 of 4 lines is covered: 25%, far below any floor.
cat > "$scratch/low.info" <<'LCOV'
SF:crates/inspect-model/src/lib.rs
DA:1,1
DA:2,0
DA:3,0
DA:4,0
end_of_record
LCOV
# 4 of 4 covered: 100%.
cat > "$scratch/high.info" <<'LCOV'
SF:crates/inspect-model/src/lib.rs
DA:1,1
DA:2,3
DA:3,1
DA:4,9
end_of_record
LCOV
# Well-formed lcov carrying no DA: records at all — the shape that a broken
# instrumentation run produces, and the one a naive gate scores as 0/0 and
# waves through.
cat > "$scratch/empty.info" <<'LCOV'
SF:crates/inspect-model/src/lib.rs
end_of_record
LCOV

export INSPECT_COVERAGE_FLOOR_FILE="$scratch/floor.txt"
export INSPECT_COVERAGE_RECEIPT="$scratch/coverage-receipt.txt"

expect_fail "coverage below the floor" "below the 85% floor" \
    bash "$here/check-coverage.sh" "$scratch/low.info"
expect_fail "coverage report with no DA: records" "coverage data is missing" \
    bash "$here/check-coverage.sh" "$scratch/empty.info"
expect_fail "coverage report that does not exist" "not found" \
    bash "$here/check-coverage.sh" "$scratch/absent.info"
if [ -n "$head_sha" ]; then
    expect_pass "coverage above the floor" \
        bash "$here/check-coverage.sh" "$scratch/high.info"

    # The passing run must have written a receipt; without it the freshness
    # gate has nothing to read and the whole chain is decorative.
    checks=$((checks + 1))
    if [ -s "$INSPECT_COVERAGE_RECEIPT" ]; then
        echo "ok:   a passing coverage run writes its receipt"
    else
        echo "FAIL: a passing coverage run wrote no receipt"
        failures=$((failures + 1))
    fi
else
    skip "coverage above the floor"
    skip "a passing coverage run writes its receipt"
fi

# The refusal itself is a behaviour worth asserting in both directions: with no
# commit the gate must decline to stamp a receipt rather than invent one.
if [ -z "$head_sha" ]; then
    expect_fail "coverage receipt on an unborn branch" "no commit to record it against" \
        bash "$here/check-coverage.sh" "$scratch/high.info"
fi

# --- coverage freshness ---------------------------------------------------
if [ -n "$head_sha" ]; then
    printf '# scratch\n%s 99 %s\n' "$head_sha" "$stamp" > "$scratch/fresh-receipt.txt"
    INSPECT_COVERAGE_RECEIPT="$scratch/fresh-receipt.txt" \
        expect_pass "freshness with a current receipt" \
        bash "$here/check-coverage-freshness.sh"

    # A receipt whose percentage is below the floor is not a claim that
    # coverage passed, however recent it is.
    printf '# scratch\n%s 10 %s\n' "$head_sha" "$stamp" > "$scratch/stale-pct.txt"
    INSPECT_COVERAGE_RECEIPT="$scratch/stale-pct.txt" \
        expect_fail "freshness with a sub-floor receipt" "below the current 85% floor" \
        bash "$here/check-coverage-freshness.sh"

    # `HEAD` is a revision this repo resolves, so a gate that verifies the
    # receipt with `git cat-file -e` alone accepts it and pins drift at zero
    # forever. It must be rejected for not being a full object id.
    printf '# scratch\nHEAD 99 %s\n' "$stamp" > "$scratch/head-receipt.txt"
    INSPECT_COVERAGE_RECEIPT="$scratch/head-receipt.txt" \
        expect_fail "freshness with a symbolic 'HEAD' commit" "not hexadecimal" \
        bash "$here/check-coverage-freshness.sh"

    INSPECT_COVERAGE_RECEIPT="$scratch/fresh-receipt.txt" INSPECT_COVERAGE_MAX_COMMITS="banana" \
        expect_fail "freshness with a non-numeric commit bound" "non-negative integer" \
        bash "$here/check-coverage-freshness.sh"
else
    skip "freshness against a real commit (4 checks)"
fi

expect_fail "freshness with no receipt at all" "not found" \
    env INSPECT_COVERAGE_RECEIPT="$scratch/absent.txt" bash "$here/check-coverage-freshness.sh"

unset INSPECT_COVERAGE_FLOOR_FILE INSPECT_COVERAGE_RECEIPT

# --- coverage ratchet -----------------------------------------------------
printf '85\n' > "$scratch/ratchet.txt"
INSPECT_COVERAGE_FLOOR_FILE="$scratch/ratchet.txt" \
    expect_pass "ratchet on a 100% measurement" \
    bash "$here/coverage-ratchet.sh" "$scratch/high.info"
checks=$((checks + 1))
raised="$(awk '!/^[[:space:]]*(#|$)/ { print; exit }' "$scratch/ratchet.txt")"
if [ "$raised" -gt 85 ]; then
    echo "ok:   the ratchet raised the floor 85 -> $raised"
else
    echo "FAIL: the ratchet left the floor at $raised after a 100% measurement"
    failures=$((failures + 1))
fi

# The direction that matters: a weak measurement must never lower the bar.
printf '90\n' > "$scratch/ratchet-down.txt"
INSPECT_COVERAGE_FLOOR_FILE="$scratch/ratchet-down.txt" \
    expect_pass "ratchet on a measurement below the floor" \
    bash "$here/coverage-ratchet.sh" "$scratch/low.info"
checks=$((checks + 1))
kept="$(awk '!/^[[:space:]]*(#|$)/ { print; exit }' "$scratch/ratchet-down.txt")"
if [ "$kept" -eq 90 ]; then
    echo "ok:   the ratchet refused to lower the floor"
else
    echo "FAIL: the ratchet lowered the floor 90 -> $kept"
    failures=$((failures + 1))
fi

# --- mutation debt --------------------------------------------------------
#
# Driven inside a scratch repository of its own. The escalation is a function of
# how much Rust has changed since a commit, so testing it against *this* working
# tree would make the result depend on whatever happens to be uncommitted — the
# gate would pass or fail for reasons unrelated to whether it works.

expect_fail "mutation debt with no receipt" "not found" \
    env INSPECT_MUTATION_RECEIPT="$scratch/absent.txt" bash "$here/check-mutation-debt.sh"

repo="$scratch/repo"
mkdir -p "$repo/src"
git -C "$repo" init -q
git -C "$repo" config user.email selftest@example.com
git -C "$repo" config user.name "Gate Self-Test"
printf 'fn main() {}\n' > "$repo/src/main.rs"
git -C "$repo" add src/main.rs
git -C "$repo" -c commit.gpgsign=false commit -qm "base"
base_sha="$(git -C "$repo" rev-parse HEAD)"

receipt="$scratch/repo-receipt.txt"
printf '# scratch\n%s 40/40 0 %s\n' "$base_sha" "$stamp" > "$receipt"

# Run a gate with the scratch repository as the working directory.
in_repo() {
    ( cd "$repo" && INSPECT_MUTATION_RECEIPT="$receipt" "$@" )
}

expect_pass "mutation debt with nothing changed since the receipt" \
    in_repo bash "$here/check-mutation-debt.sh"

# Twenty lines of new Rust, uncommitted. The gate has to see them: `just check`
# runs before the commit exists, so a metric that only counted committed lines
# would report zero for the change it is being asked about.
printf 'fn added() {\n%s}\n' "$(for _ in $(seq 1 18); do echo '    let _ = 1;'; done)" \
    > "$repo/src/added.rs"

expect_fail "mutation debt over the line budget, uncommitted" "Rust lines have changed" \
    in_repo env INSPECT_MUTATION_WARN_LINES=5 INSPECT_MUTATION_FAIL_LINES=10 \
    bash "$here/check-mutation-debt.sh"

checks=$((checks + 1))
warned="$(in_repo env INSPECT_MUTATION_WARN_LINES=5 INSPECT_MUTATION_FAIL_LINES=500 \
    bash "$here/check-mutation-debt.sh" 2>&1)" && status=0 || status=$?
if [ "$status" -eq 0 ] && printf '%s' "$warned" | grep -q "WARN"; then
    echo "ok:   mutation debt warns between the two thresholds without blocking"
else
    echo "FAIL: mutation debt did not warn between the thresholds (exit $status)"
    echo "$warned" | sed 's/^/      /'
    failures=$((failures + 1))
fi

# A docs-only change must not accrue mutation debt: nothing about the Rust the
# last run measured has moved.
git -C "$repo" add src/added.rs
git -C "$repo" -c commit.gpgsign=false commit -qm "add rust"
printf '# notes\n' > "$repo/README.md"
git -C "$repo" add README.md
git -C "$repo" -c commit.gpgsign=false commit -qm "docs"
printf '# scratch\n%s 40/40 0 %s\n' "$(git -C "$repo" rev-parse HEAD~1)" "$stamp" > "$receipt"
expect_pass "mutation debt ignores a commit that touches no Rust" \
    in_repo env INSPECT_MUTATION_MAX_COMMITS=0 bash "$here/check-mutation-debt.sh"

printf '# scratch\nHEAD 40/40 0 %s\n' "$stamp" > "$scratch/mutation-head.txt"
expect_fail "mutation debt with a symbolic 'HEAD' commit" "not hexadecimal" \
    env INSPECT_MUTATION_RECEIPT="$scratch/mutation-head.txt" bash "$here/check-mutation-debt.sh"

printf '# scratch\n%s\n' "$base_sha" > "$scratch/mutation-short.txt"
expect_fail "mutation debt with a truncated receipt" "malformed" \
    env INSPECT_MUTATION_RECEIPT="$scratch/mutation-short.txt" bash "$here/check-mutation-debt.sh"

# A warn threshold above the fail threshold is unsatisfiable: it can only ever
# warn, never block, so the gate would silently stop being one.
expect_fail "mutation debt with warn above fail" "must not exceed" \
    in_repo env INSPECT_MUTATION_WARN_LINES=900 INSPECT_MUTATION_FAIL_LINES=100 \
    bash "$here/check-mutation-debt.sh"

# A receipt naming a commit from somewhere else entirely.
printf '# scratch\n%s 40/40 0 %s\n' "0123456789abcdef0123456789abcdef01234567" "$stamp" \
    > "$scratch/mutation-elsewhere.txt"
expect_fail "mutation debt naming a commit this repository does not have" "not in this repository" \
    env INSPECT_MUTATION_RECEIPT="$scratch/mutation-elsewhere.txt" \
    bash "$here/check-mutation-debt.sh"

# --- file sizes -----------------------------------------------------------
INSPECT_FILE_SOFT=1 INSPECT_FILE_HARD=2 \
    expect_fail "file sizes against a 2-line hard limit" "hard limit" \
    bash "$here/check-file-sizes.sh"
expect_pass "file sizes at the real limits" bash "$here/check-file-sizes.sh"

# The gate must not pass by matching nothing. Run it where no Rust exists: if
# it reports success there, it would report success on a tree it never read.
mkdir -p "$scratch/empty-tree/crates" "$scratch/empty-tree/apps"
cp "$here/common.sh" "$here/check-file-sizes.sh" "$scratch/empty-tree/"
checks=$((checks + 1))
if (cd "$scratch/empty-tree" && bash ./check-file-sizes.sh >/dev/null 2>&1); then
    echo "FAIL: file sizes passed on a tree with no Rust sources"
    failures=$((failures + 1))
else
    echo "ok:   file sizes refuses a tree it found nothing in"
fi

# --- verdict --------------------------------------------------------------
echo
note=""
[ "$skipped" -gt 0 ] && note=" ($skipped skipped — no commit yet)"
if [ "$failures" -gt 0 ]; then
    echo "Gate self-test: $failures of $checks checks FAILED$note"
    exit 1
fi
echo "Gate self-test: all $checks checks passed$note"
