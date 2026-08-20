#!/usr/bin/env bash
set -euo pipefail

# Run a mutation pass and refresh the receipt the debt gate reads.
#
# Scoped to the diff against the previous receipt's commit, not the whole tree.
# A full pass over both pure crates is tens of minutes; the diff-scoped pass
# asks the question that is actually open — "would the tests notice the code I
# just wrote changing?" — in minutes. `INSPECT_MUTANTS_BASE=<rev>` overrides the
# base, and `INSPECT_MUTANTS_FULL=1` runs the whole tree, which is what
# establishes the first baseline.
#
# The survivor baseline in scripts/mutant-baseline.txt ratchets downward only. A
# run with more survivors than the baseline fails; a run with fewer rewrites it.
# Accepting a rise has to be a deliberate edit of that file with a reason, not a
# number that drifts up unnoticed one commit at a time.

. "$(dirname "$0")/common.sh"

here="$(dirname "$0")"
RECEIPT="${INSPECT_MUTATION_RECEIPT:-$here/mutation-receipt.txt}"
BASELINE="${INSPECT_MUTANT_BASELINE:-$here/mutant-baseline.txt}"
# cargo-mutants' `--output` names the *parent* of the results directory, not
# the results directory itself: `--output mutants.out` writes
# `mutants.out/mutants.out`. So the parent is passed and the results are read
# from the `mutants.out` it creates inside it.
OUT_PARENT="${INSPECT_MUTANTS_OUT_PARENT:-.}"
OUT="$OUT_PARENT/mutants.out"

command -v cargo-mutants >/dev/null 2>&1 ||
    fail "cargo-mutants not installed — 'cargo install cargo-mutants'"

args=(--output "$OUT_PARENT")
if [ -n "${INSPECT_MUTANTS_FULL:-}" ]; then
    echo "Running a FULL mutation pass over the workspace."
else
    base="${INSPECT_MUTANTS_BASE:-}"
    if [ -z "$base" ] && [ -f "$RECEIPT" ]; then
        base="$(receipt_line "$RECEIPT" | awk '{print $1}')"
    fi
    if [ -n "$base" ]; then
        require_sha "$base" "mutation base commit"
        echo "Running a diff-scoped mutation pass against ${base:0:12}."
        # A temp file, not one beside the results: the results directory is
        # gitignored and a sibling would not be, so the scope of a past run
        # would show up as an untracked file in every later `git status`.
        diff_file="$(mktemp -t allium-inspect-mutants)"
        trap 'rm -f "$diff_file"' EXIT
        git diff "$base"..HEAD -- '*.rs' > "$diff_file" ||
            fail "could not compute the diff against $base"
        if [ ! -s "$diff_file" ]; then
            echo "No Rust changes since ${base:0:12} — nothing to mutate."
            exit 0
        fi
        args+=(--in-diff "$diff_file")
    else
        echo "No receipt and no base given — running a FULL pass to establish the baseline."
    fi
fi

set +e
cargo mutants "${args[@]}"
mutants_status=$?
set -e

# cargo-mutants exits 2 when mutants survived and 0 when none did. Anything
# else — a build failure, a bad argument — is a broken run, and reading a
# survivor count out of it would report a number nothing measured.
case "$mutants_status" in
    0|2) ;;
    *) fail "cargo mutants exited $mutants_status — the run did not complete" ;;
esac

exec "$here/score-mutants.sh" "$OUT" "$BASELINE" "$RECEIPT"
