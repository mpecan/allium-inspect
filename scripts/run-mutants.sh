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
# When the base is the receipt's and no Rust has changed since it, there is
# nothing to mutate, and a 0-survivor receipt is re-stamped onto HEAD if nothing
# else the run depended on changed either — see restamp_receipt below for why
# that is the same measurement and not a new one.
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

# What a mutation run's outcome depends on besides the Rust: the build inputs
# (an allium_parser bump changes behaviour — stipulation 5) and the files the
# tests read (fixtures under crates/, the journeys under specs/, the fenced
# blocks in docs/journeys checked by tests/docs.rs). The drift counters measure
# Rust alone; a re-stamp claims more — that the run would come out the same — so
# it checks more. When a test starts reading something new, add it here.
MUTATION_INPUTS=(crates apps Cargo.toml Cargo.lock rust-toolchain.toml .cargo specs docs/journeys)

# Move a 0-survivor receipt onto HEAD when nothing the run depended on has
# changed. Called only when the base came from the receipt and the Rust diff
# from it is empty, so there is nothing to mutate — and without this the receipt
# would keep naming a commit that a squash-merge is about to drop.
#
# Honest only because it is the same measurement of the same inputs, checked
# rather than assumed. The Rust is the receipt's: resolve_measured_commit has
# already tied the receipt's fingerprint to the base, and the Rust diff from the
# base is empty. Everything else in MUTATION_INPUTS must be unchanged too. The
# survivor count must be 0 — a run that left survivors left work open, and
# carrying it forward would advance the debt clock without that work being done.
# The measured-at time is kept, and the receipt says where it was re-stamped
# from: the mutants ran there, not at the commit it now names.
restamp_receipt() {
    local head head_fp
    head="$(head_commit)"
    [ -n "$head" ] || return 0
    if [ "$receipt_survivors" -ne 0 ]; then
        echo "Receipt left at ${receipt_sha:0:12}: it recorded $receipt_survivors survivors, and a re-stamp"
        echo "would carry them forward unexamined. Run a pass that clears them."
        return 0
    fi
    if ! git diff --quiet "$base" HEAD -- "${MUTATION_INPUTS[@]}"; then
        echo "Receipt left at ${receipt_sha:0:12}: inputs the run depended on changed since ${base:0:12}:"
        git diff --name-only "$base" HEAD -- "${MUTATION_INPUTS[@]}" | sed 's/^/       /'
        echo "       Run a pass that covers them, e.g. INSPECT_MUTANTS_FULL=1 just mutants."
        return 0
    fi
    head_fp="$(rust_fingerprint "$head")" || fail "could not read the Rust sources at $head"
    if [ "$head" = "$receipt_sha" ] && [ "$receipt_fp" = "$head_fp" ]; then
        return 0
    fi
    write_mutation_receipt "$RECEIPT" "$head" "$receipt_score" 0 "$receipt_measured_at" "$head_fp" \
        "$receipt_sha"
    echo "Receipt re-stamped at ${head:0:12}: same inputs as the 0-survivor run at ${receipt_sha:0:12}."
}

args=(--output "$OUT_PARENT")
if [ -n "${INSPECT_MUTANTS_FULL:-}" ]; then
    echo "Running a FULL mutation pass over the workspace."
else
    base="${INSPECT_MUTANTS_BASE:-}"
    from_receipt=""
    if [ -z "$base" ] && [ -f "$RECEIPT" ]; then
        read_mutation_receipt "$RECEIPT"
        # The same resolution the debt gate makes, so after a squash-merge the
        # pass is scoped against the commit the gate is counting from.
        resolve_measured_commit "$receipt_sha" "$receipt_fp" "mutation receipt" \
            "INSPECT_MUTANTS_FULL=1 just mutants (or INSPECT_MUTANTS_BASE=<the commit this work started from> just mutants)"
        base="$measured_from"
        from_receipt=1
    fi
    if [ -n "$base" ]; then
        require_sha "$base" "mutation base commit"
        echo "Running a diff-scoped mutation pass against ${base:0:12}."
        # A temp file, not one beside the results: the results directory is
        # gitignored and a sibling would not be, so the scope of a past run
        # would show up as an untracked file in every later `git status`.
        diff_file="$(mktemp "${TMPDIR:-/tmp}/allium-inspect-mutants.XXXXXX")"
        trap 'rm -f "$diff_file"' EXIT
        git diff "$base"..HEAD -- "$RUST_PATHSPEC" > "$diff_file" ||
            fail "could not compute the diff against $base"
        if [ ! -s "$diff_file" ]; then
            echo "No Rust changes since ${base:0:12} — nothing to mutate."
            [ -z "$from_receipt" ] || restamp_receipt
            exit 0
        fi
        args+=(--in-diff "$diff_file")
    else
        echo "No receipt and no base given — running a FULL pass to establish the baseline."
    fi
fi

# Checked here rather than first: a run with nothing to mutate needs no
# cargo-mutants, and that is what lets the self-test drive the re-stamp.
command -v cargo-mutants >/dev/null 2>&1 ||
    fail "cargo-mutants not installed — 'cargo install cargo-mutants'"

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
