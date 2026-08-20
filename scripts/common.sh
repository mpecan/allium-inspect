#!/usr/bin/env bash

# Shared helpers for the quality-gate scripts in this repo.
#
# Every gate reads its definitions from here rather than holding a copy. Two
# copies of a rule drift, and a baseline written by one then fails the other on
# the next run.

# Print an error and exit 1; the gate scripts' shared failure path.
fail() {
    echo "ERROR: $1"
    exit 1
}

# Fail unless the argument is a non-negative integer.
#
# `[ x -gt y ]` exits 2 — not 1 — on a non-integer operand, and `set -e` does
# not apply inside an `if` condition, so an unguarded non-numeric limit skips
# the failure branch and the gate exits 0. A gate that cannot fail is worse
# than no gate.
require_int() {
    case "$1" in
        ''|*[!0-9]*) fail "${2:-value} must be a non-negative integer, got '$1'" ;;
    esac
}

# Fail unless the argument is a full 40-character hex object id.
#
# A full object id, not any revision this repo happens to resolve: `git cat-file
# -e` accepts expressions, so a receipt reading `HEAD` would verify fine and pin
# its own drift at zero forever.
require_sha() {
    case "$1" in
        *[!0-9a-f]*) fail "${2:-commit} is not hexadecimal: '$1'" ;;
    esac
    [ "${#1}" -eq 40 ] || fail "${2:-commit} is not a full object id: '$1'"
}

# Lines before the first `#[cfg(test)] mod`; the whole file when there is none.
#
# Test modules are exempt from the size budget, so every script that measures
# "source" uses this one definition of the boundary. The boundary is the test
# *module*, not any `#[cfg(test)]` item — keying on the bare attribute lets a
# single test-only helper mid-file exempt everything after it.
count_source_lines() {
    awk '
        /^[[:space:]]*#\[cfg\(test\)\]/ { cand = NR; pending = 1; next }
        pending {
            if ($0 ~ /^[[:space:]]*(pub[[:space:]]+)?mod[[:space:]]/) {
                print cand - 1; found = 1; exit
            }
            pending = 0
        }
        END { if (!found) print NR }
    ' "$1"
}

# Rust sources under crates/ and apps/. Build artifacts and generated bindings
# are excluded: neither is written by hand, so neither is subject to a budget.
rust_files() {
    find crates apps -name '*.rs' -not -path '*/target/*' -not -path '*/gen/*' "$@"
}

# The first non-comment, non-blank line of a receipt file.
receipt_line() {
    awk '!/^[[:space:]]*(#|$)/ { print; exit }' "$1"
}

# The current commit, or empty on an unborn branch.
#
# `git rev-parse HEAD` echoes the literal string "HEAD" back on stdout when it
# cannot resolve it, and only signals the failure on stderr and in its exit
# code. A gate that captures that output lands the word "HEAD" in a receipt
# where a commit belongs — which is exactly the symbolic-revision case the
# receipt validators exist to reject. `--verify --quiet` prints nothing instead.
head_commit() {
    git rev-parse --verify --quiet HEAD 2>/dev/null || true
}

# Rust-touching commits between a receipt's commit and HEAD.
#
# Counts commits that touch *.rs, not all commits: a run of docs-only commits
# cannot invalidate a measurement of Rust.
rust_commits_since() {
    git rev-list --count "$1"..HEAD -- '*.rs'
}

# Added + modified Rust lines between a receipt's commit and HEAD.
#
# This is the metric the mutation cadence is driven by. `--numstat` gives
# added and deleted per file; deletions are counted too, because removing a
# tested branch changes what the suite covers just as surely as adding one.
rust_lines_changed_since() {
    git diff --numstat "$1"..HEAD -- '*.rs' |
        awk '$1 != "-" { added += $1; deleted += $2 } END { print added + deleted + 0 }'
}
