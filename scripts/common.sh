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

# Rust sources under crates/ and apps/.
#
# Build artifacts and generated bindings are excluded: neither is written by
# hand, so neither is subject to a budget.
#
# `tests/` is excluded for the same reason `count_source_lines` stops at the
# first test module — a thorough test file is the goal, not a cost. The
# distinction is invisible to `count_source_lines` there, because an integration
# test has no `#[cfg(test)] mod` to stop at: the whole file is the test module.
rust_files() {
    find crates apps -name '*.rs' \
        -not -path '*/target/*' \
        -not -path '*/gen/*' \
        -not -path '*/tests/*' \
        "$@"
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

# What "Rust" means to every receipt gate: the commit counter, the line
# counter, the mutation diff and the fingerprint all take this one pathspec, so
# a receipt cannot be fresh by one definition and stale by another.
RUST_PATHSPEC='*.rs'

# Rust-touching commits between a receipt's commit and HEAD.
#
# Counts commits that touch *.rs, not all commits: a run of docs-only commits
# cannot invalidate a measurement of Rust.
rust_commits_since() {
    git rev-list --count "$1"..HEAD -- "$RUST_PATHSPEC"
}

# The fingerprint of no Rust at all: `git hash-object` of empty input. A
# receipt carrying it would match every Rust-free commit, so it is refused.
EMPTY_FINGERPRINT=e69de29bb2d1d6434b8b29ae775ad8c2e48c5391

# The empty tree, which every git knows without storing it. SHA-1 only, like
# require_sha: this repository is SHA-1, and both validators say so.
EMPTY_TREE=4b825dc642cb6eb9a060e54bf8d69288fbee4904

# Fail unless the argument is a 40-character hex Rust fingerprint.
require_fingerprint() {
    case "$1" in
        *[!0-9a-f]*) fail "${2:-fingerprint} is not hexadecimal: '$1'" ;;
    esac
    [ "${#1}" -eq 40 ] || fail "${2:-fingerprint} is not a 40-character fingerprint: '$1'"
    [ "$1" != "$EMPTY_FINGERPRINT" ] ||
        fail "${2:-fingerprint} is the fingerprint of no Rust at all: '$1'"
}

# A content fingerprint of the Rust sources at a commit.
#
# Every path matching RUST_PATHSPEC with its mode and blob id, listed as a diff
# from the empty tree (`ls-tree` takes no glob pathspec; `diff-tree` takes the
# same one the drift counters do) and hashed. `-z` so no path is ever quoted —
# quoting follows `core.quotePath`, which would make the fingerprint depend on
# whose machine computed it. No checkout, no working tree: two commits with the
# same Rust bytes at the same paths have the same fingerprint, whatever else
# differs between them.
#
# Returns non-zero for a commit it cannot read — checked first, because a
# failed diff-tree piped into hash-object would otherwise hash empty input and
# produce a fingerprint like any other. Callers capture it with `|| fail`.
#
# This is what lets a receipt survive a squash-merge. The commit it names lives
# on a branch that never reaches main; the sources it measured do.
rust_fingerprint() {
    git cat-file -e "$1^{commit}" 2>/dev/null || return 1
    git diff-tree -r -z --no-renames "$EMPTY_TREE" "$1" -- "$RUST_PATHSPEC" | git hash-object --stdin
}

# How many Rust-touching first-parent commits back from HEAD are searched for a
# missing receipt commit's Rust sources.
#
# Rust-touching, the same unit both drift bounds count in. A fingerprint can
# only change at a commit that touches Rust, so the oldest match among those is
# the oldest match in the whole first-parent window — docs-only history costs
# nothing. And a match k of them back has at least k-1 commits of drift after
# it, so anything past either gate's commit limit would fail there anyway; 100
# is far beyond both. The bound is what keeps the gate sub-second; past it the
# search fails closed and the message names this knob.
RECEIPT_SEARCH_DEPTH="${INSPECT_RECEIPT_SEARCH_DEPTH:-100}"

# Decide which commit a receipt's drift is measured from. Sets two globals,
# because `fail` inside a command substitution would print its reason into a
# variable and exit only the subshell:
#
#   measured_from   the commit to count drift from
#   measured_note   empty, or the substitution to say aloud
#
#   resolve_measured_commit <sha> <fingerprint-or-empty> <what> <rerun-recipe>
#
# What a receipt certifies is the Rust sources its fingerprint names. The
# commit is where to find them, and is used when it is in HEAD's history — after
# checking that the fingerprint really is the Rust there, so an inconsistent
# receipt fails on the branch rather than after the merge. When the commit is
# missing, or present but not an ancestor of HEAD (a squash-merged branch whose
# ref has not been deleted yet — the answer must not depend on that), the
# fingerprint is looked for in HEAD's first-parent history, RECEIPT_SEARCH_DEPTH
# Rust-touching commits deep. Of several matches the *oldest* is used: each is a
# first-parent ancestor of the next, so the oldest has the most commits of drift
# after it, and every match has the same Rust and therefore the same line delta.
# The stricter reading, and the same answer every time.
#
# A shallow clone is not searched: a missing commit there may be just past the
# boundary, and the fix is to fetch history, not to measure again.
#
# A receipt with no fingerprint (the old format) gets no search and no
# ancestry check: present, it is used exactly as before; missing, it fails as it
# always did. No match fails too. Neither ever passes silently.
#
# On the in-history path a NOTE says when HEAD's Rust differs from the
# receipt's. That is the branch-side warning for a squash-merge: on main only an
# exact fingerprint match survives, so the drift budget a branch is spending
# does not carry across the merge.
resolve_measured_commit() {
    local sha="$1" fp="$2" what="$3" rerun="$4" c f actual head_fp why match=""
    measured_from=""
    measured_note=""
    # Validated whether or not it is needed, like every other limit: a bad
    # setting should fail the run that set it, not the one that first searches.
    require_int "$RECEIPT_SEARCH_DEPTH" "INSPECT_RECEIPT_SEARCH_DEPTH"
    if git cat-file -e "$sha^{commit}" 2>/dev/null; then
        if [ -z "$fp" ]; then
            measured_from="$sha"
            echo "NOTE:  $what records no Rust fingerprint, so it will not survive a squash-merge."
            echo "       Run '$rerun' before merging."
            return 0
        fi
        actual="$(rust_fingerprint "$sha")" || fail "could not read the Rust sources at $sha"
        [ "$actual" = "$fp" ] ||
            fail "$what records Rust fingerprint $fp, but the Rust at ${sha:0:12} is $actual.
       The receipt is internally inconsistent — '$rerun' did not write it as it stands. Run it again."
        if git merge-base --is-ancestor "$sha" HEAD 2>/dev/null; then
            measured_from="$sha"
            head_fp="$(rust_fingerprint HEAD)" || fail "could not read the Rust sources at HEAD"
            if [ "$head_fp" != "$fp" ]; then
                echo "NOTE:  HEAD's Rust is not the Rust $what measured, so it will not survive a squash-merge."
                echo "       Run '$rerun' after the last Rust change, before merging."
            fi
            return 0
        fi
        why="is not in HEAD's history"
    else
        why="is not in this repository"
    fi
    [ -n "$fp" ] ||
        fail "$what names commit $sha, which $why
       It records no Rust fingerprint to find the same sources by. Run '$rerun'."
    [ "$(git rev-parse --is-shallow-repository 2>/dev/null)" != "true" ] ||
        fail "$what names commit $sha, which $why — and this clone is shallow.
       Fetch full history ('git fetch --unshallow'; actions/checkout 'fetch-depth: 0') and run again."
    # Oldest first within the window, so the first match is the one wanted.
    while read -r c; do
        f="$(rust_fingerprint "$c")" || fail "could not read the Rust sources at $c"
        if [ "$f" = "$fp" ]; then
            match="$c"
            break
        fi
    done < <(git rev-list --reverse --first-parent --max-count="$RECEIPT_SEARCH_DEPTH" HEAD -- "$RUST_PATHSPEC")
    [ -n "$match" ] ||
        fail "$what names commit $sha, which $why, so the measured commit is gone,
       and none of the last $RECEIPT_SEARCH_DEPTH Rust-touching first-parent commits has the Rust sources it measured.
       Run '$rerun' to measure these sources, or raise INSPECT_RECEIPT_SEARCH_DEPTH if they are further back."
    measured_from="$match"
    measured_note=" (not in this history; same Rust sources as ${match:0:12})"
}

# Added + modified Rust lines between a receipt's commit and the working tree.
#
# This is the metric the mutation cadence is driven by. `--numstat` gives added
# and deleted per file; deletions are counted too, because removing a tested
# branch changes what the suite covers just as surely as adding one.
#
# Against the working tree, not `..HEAD`. The gate runs inside `just check`,
# which is what you run *before* committing — so at that moment the change being
# judged is not in HEAD yet. Comparing to HEAD would report zero debt for a
# thousand uncommitted lines and then start counting them only once they were
# already in.
# Untracked files are counted separately, because `git diff` does not see them
# at all. A brand new module — the single most likely thing to need mutation
# testing — would otherwise contribute nothing to the metric until the moment it
# was staged, which is precisely when the gate has already been asked.
rust_lines_changed_since() {
    local tracked untracked
    tracked="$(
        git diff --numstat "$1" -- "$RUST_PATHSPEC" |
            awk '$1 != "-" { added += $1; deleted += $2 } END { print added + deleted + 0 }'
    )"
    untracked="$(
        git ls-files --others --exclude-standard -z -- "$RUST_PATHSPEC" |
            xargs -0 -r cat 2>/dev/null |
            wc -l | tr -d ' '
    )"
    echo $(( tracked + untracked ))
}

# Write the mutation receipt. One writer for the scorer and the re-stamp, so the
# header describing the fields cannot drift from the line beneath it.
#
#   write_mutation_receipt <file> <commit> <caught>/<total> <survivors> <measured-at> <fingerprint> [<restamped-from>]
write_mutation_receipt() {
    {
        echo "# Mutation receipt — written by 'just mutants'. Do not hand-edit."
        echo "# Read by scripts/check-mutation-debt.sh, which runs inside 'just check'."
        echo "# Fields: <commit> <caught>/<total> <survivors> <measured-at> <rust-fingerprint>"
        echo "# What is certified is the Rust sources <rust-fingerprint> names (see"
        echo "# rust_fingerprint in scripts/common.sh); <commit> is where to find them, and"
        echo "# <measured-at> is when the mutants ran. A 'just mutants' with nothing to mutate"
        echo "# may move <commit> forward onto a later commit with the same inputs, keeping"
        echo "# the time. After a squash-merge drops <commit>, the gate finds the commit on"
        echo "# main with the same sources. Receipts without a fingerprint are still read."
        [ -z "${7:-}" ] || echo "# Re-stamped from $7, over the same inputs; the mutants did not run at <commit>."
        echo "$2 $3 $4 $5 $6"
    } > "$1"
}

# Read and validate the mutation receipt's data line. Sets receipt_sha,
# receipt_score, receipt_survivors, receipt_measured_at and receipt_fp (empty for
# an old-format receipt). One reader for the debt gate and for `just mutants`,
# so neither accepts a receipt the other would refuse.
read_mutation_receipt() {
    local extra
    read -r receipt_sha receipt_score receipt_survivors receipt_measured_at receipt_fp extra \
        <<<"$(receipt_line "$1")"
    [ -n "${receipt_sha:-}" ] && [ -n "${receipt_score:-}" ] && [ -n "${receipt_survivors:-}" ] &&
        [ -n "${receipt_measured_at:-}" ] && [ -z "${extra:-}" ] ||
        fail "mutation receipt $1 is malformed — run 'just mutants'"
    require_sha "$receipt_sha" "mutation receipt commit"
    require_int "$receipt_survivors" "mutation receipt survivor count"
    receipt_fp="${receipt_fp:-}"
    [ -z "$receipt_fp" ] || require_fingerprint "$receipt_fp" "mutation receipt Rust fingerprint"
}
