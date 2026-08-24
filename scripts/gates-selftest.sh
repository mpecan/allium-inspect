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

# --- mutation scoring -----------------------------------------------------
#
# The half of `just mutants` that decides. Driven here against fixture results
# rather than a real half-hour run, which is the whole reason it is a script of
# its own.
results="$scratch/mutants.out"
mkdir -p "$results"
score_baseline="$scratch/mutant-baseline.txt"
score_receipt="$scratch/mutation-receipt.txt"

# `seq 1 0` counts *down* on BSD, so the counts are written by hand.
fixture_results() {
    lines "$1" caught > "$results/caught.txt"
    lines "$2" survived > "$results/missed.txt"
}

lines() {
    local i=0
    while [ "$i" -lt "$1" ]; do
        i=$((i + 1))
        echo "src/lib.rs:$i: $2"
    done
}

score() { bash "$here/score-mutants.sh" "$results" "$score_baseline" "$score_receipt"; }

printf '2\n' > "$score_baseline"
fixture_results 10 5
expect_fail "mutation scoring above the survivor baseline" "baseline allows 2" score

fixture_results 13 2
if [ -n "$head_sha" ]; then
    expect_pass "mutation scoring at the survivor baseline" score
else
    skip "mutation scoring at the survivor baseline"
fi

fixture_results 15 0
if [ -n "$head_sha" ]; then
    expect_pass "mutation scoring below the survivor baseline" score
    checks=$((checks + 1))
    if [ "$(receipt_line "$score_baseline")" = "0" ]; then
        echo "ok:   the survivor baseline ratcheted 2 -> 0"
    else
        echo "FAIL: the survivor baseline did not ratchet down"
        failures=$((failures + 1))
    fi
    # And having ratcheted, it does not go back up.
    fixture_results 14 1
    expect_fail "mutation scoring after the baseline ratcheted" "baseline allows 0" score
else
    skip "mutation scoring below the survivor baseline"
fi

# A run that scored nothing is a broken run, not a clean sweep. This is the
# shape that ships green forever: no results, no survivors, no complaint.
rm -f "$score_baseline"
fixture_results 0 0
expect_fail "mutation scoring with nothing scored" "0 mutants" score

rm -f "$results/caught.txt" "$results/missed.txt"
expect_fail "mutation scoring with no results file at all" "no results to score" score

# --- third-party notices --------------------------------------------------
#
# The binary embeds the npm bundle, so this gate is the only thing standing
# between a new frontend dependency and its licence quietly not shipping.

INSPECT_THIRD_PARTY_OUT="$scratch/no-such-notice.txt" \
    expect_fail "third-party notice missing entirely" "is stale" \
    node "$here/third-party.mjs" --check

printf 'not the notice\n' > "$scratch/stale-notice.txt"
INSPECT_THIRD_PARTY_OUT="$scratch/stale-notice.txt" \
    expect_fail "third-party notice out of date" "is stale" \
    node "$here/third-party.mjs" --check

INSPECT_THIRD_PARTY_ALLOW="Apache-2.0" \
    expect_fail "a shipped dependency under an unreviewed licence" "unreviewed licence" \
    node "$here/third-party.mjs" --check

expect_pass "third-party notice matching the shipped tree" \
    node "$here/third-party.mjs" --check

# --- evidence -------------------------------------------------------------
#
# `allium-journey evidence seal` refuses a run whose pictures do not resolve,
# which makes it a gate, which means it has to be seen refusing. The case that
# matters is a *rename*: a journey renamed with the harness left pointing at the
# old name leaves every picture filed under a step that no longer exists, and a
# seal that shrugged would leave those steps reading as never covered — the one
# thing this whole feature exists to make impossible.
#
# Built rather than reused: the repository's own journeys are what the walk
# photographs, and a self-test that depended on them would fail for whoever
# next rewords a step.

# Built rather than skipped when absent. A self-test that quietly tests less
# than it claims is the same failure as a gate that cannot fail, and "the binary
# happened to be lying around" is not a condition to hang six checks on. It is
# already built by `just test`, which runs first, so this costs nothing in the
# ordinary case.
journey_bin="target/debug/allium-journey"
if [ ! -x "$journey_bin" ]; then
    cargo build -q -p allium-journey >&2
fi

if [ ! -x "$journey_bin" ]; then
    echo "FAIL: evidence gate — allium-journey could not be built"
    failures=$((failures + 1))
    checks=$((checks + 1))
else
    ev="$scratch/evidence"
    mkdir -p "$ev"
    cat > "$ev/one.journey" <<'JOURNEY'
journey Reading {
    goal: somebody reads a specification

    1. she points at it
        then set.status = reading
}
JOURNEY
    printf 'a picture\n' > "$ev/01.png"

    frame() {
        printf '{"step":"%s","image":"%s","caption":null,"passed":true,' "$1" "$2"
        printf '"taken_at":"2026-01-01T00:00:00Z","source":null}\n'
    }

    # A rename: the journey is `Reading`, the harness still says `Browsing`.
    frame "Browsing.1" "01.png" > "$ev/frames.jsonl"
    expect_fail "a frame naming a step no journey has" "Browsing.1" \
        "$journey_bin" evidence seal "$ev" "$ev" --at 2026-01-01T00:00:00Z

    # A picture the manifest would promise and nobody could open.
    frame "Reading.1" "99.png" > "$ev/frames.jsonl"
    expect_fail "a frame whose picture is not on disk" "99.png" \
        "$journey_bin" evidence seal "$ev" "$ev" --at 2026-01-01T00:00:00Z

    frame "Reading.1" "01.png" > "$ev/frames.jsonl"
    expect_pass "a run whose frames all resolve" \
        "$journey_bin" evidence seal "$ev" "$ev" --at 2026-01-01T00:00:00Z

    # And the reporting half, which is a separate gate with its own directions:
    # a marker naming a step that no longer exists must fail even though the
    # test carrying it still passes.
    printf '// journey: Renamed.1\n' > "$ev/marked.rs"
    expect_fail "a marker naming a step no journey has" "no such step" \
        "$journey_bin" evidence check "$ev" --journeys "$ev" --code "$ev"

    printf '// journey: Reading.1\n' > "$ev/marked.rs"
    expect_pass "a marker whose step was photographed" \
        "$journey_bin" evidence check "$ev" --journeys "$ev" --code "$ev"

    # The direction that pays for the source scan at all: a marker with no
    # picture behind it is a finding, not silence.
    cat >> "$ev/one.journey" <<'JOURNEY'

journey Browsing {
    goal: somebody looks at a graph

    1. she opens it
        then session.stale = false
}
JOURNEY
    printf '// journey: Browsing.1\n' > "$ev/marked.rs"
    expect_fail "a test that claims a step and shows nothing" "claimed" \
        "$journey_bin" evidence check "$ev" --journeys "$ev" --code "$ev"

    # A journey that declares how it should be shown, and a tag outside it. The
    # typo the declaration exists to catch: nothing fails without it, and the
    # spare axis it invents is invisible in a dropdown that grew an entry.
    cat > "$ev/one.journey" <<'JOURNEY'
journey Reading {
    goal: somebody reads a specification

    shows:
        theme: dark, light

    1. she points at it
        then set.status = reading
}
JOURNEY
    rm -f "$ev/marked.rs"
    frame "Reading.1" "01.png" > "$ev/frames.jsonl"
    "$journey_bin" evidence seal "$ev" "$ev" --at 2026-01-01T00:00:00Z >/dev/null

    tagged() {
        printf '{"step":"Reading.1","image":"01.png","caption":null,"passed":true,'
        printf '"taken_at":"2026-01-01T00:00:00Z","source":null,"tags":{"%s":"%s"}}\n' "$1" "$2"
    }

    tagged "them" "dark" > "$ev/frames.jsonl"
    "$journey_bin" evidence seal "$ev" "$ev" --at 2026-01-01T00:00:00Z >/dev/null
    expect_fail "a tag the journey does not ask for" "no such tag" \
        "$journey_bin" evidence check "$ev" --journeys "$ev"

    tagged "theme" "sepia" > "$ev/frames.jsonl"
    "$journey_bin" evidence seal "$ev" "$ev" --at 2026-01-01T00:00:00Z >/dev/null
    expect_fail "a value the journey does not ask for" "no such value" \
        "$journey_bin" evidence check "$ev" --journeys "$ev"

    tagged "theme" "dark" > "$ev/frames.jsonl"
    "$journey_bin" evidence seal "$ev" "$ev" --at 2026-01-01T00:00:00Z >/dev/null
    expect_pass "a tag the journey asks for" \
        "$journey_bin" evidence check "$ev" --journeys "$ev"

    # And the direction that must NOT fail: a declared value nobody has
    # photographed is a demand written before the thing it asks for, the same
    # as a step, and a gate failing on those would fail on every journey the
    # day it was written.
    expect_pass "a declared value nothing has answered yet" \
        "$journey_bin" evidence check "$ev" --journeys "$ev"
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
