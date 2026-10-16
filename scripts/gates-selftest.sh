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

# Assert a command exits 0 *and* says something: a pass for the right reason.
expect_pass_saying() {
    local what="$1" want="$2"; shift 2
    checks=$((checks + 1))
    local out status
    out="$("$@" 2>&1)" && status=0 || status=$?
    if [ "$status" -ne 0 ]; then
        echo "FAIL: $what — expected exit 0, got $status"
        echo "$out" | sed 's/^/      /'
        failures=$((failures + 1))
    elif ! printf '%s' "$out" | grep -q "$want"; then
        echo "FAIL: $what — passed but did not say '$want'"
        echo "$out" | sed 's/^/      /'
        failures=$((failures + 1))
    else
        echo "ok:   $what passes, saying why"
    fi
}

# Assert the Nth field of a receipt's data line is the Rust fingerprint of HEAD.
expect_fingerprint_field() {
    local what="$1" file="$2" field="$3" got want
    checks=$((checks + 1))
    got="$(receipt_line "$file" | awk -v f="$field" '{ print $f }')"
    want="$(rust_fingerprint HEAD)"
    if [ "$got" = "$want" ]; then
        echo "ok:   $what records the Rust fingerprint"
    else
        echo "FAIL: $what recorded fingerprint '$got', expected '$want'"
        failures=$((failures + 1))
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
    # And the receipt it wrote carries what a squash-merge needs to find its
    # sources again, and is one the freshness gate accepts.
    expect_fingerprint_field "a passing coverage run's receipt" "$INSPECT_COVERAGE_RECEIPT" 4
    expect_pass "freshness reading the receipt check-coverage wrote" \
        bash "$here/check-coverage-freshness.sh"
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
if [ "$status" -eq 0 ] && printf '%s' "$warned" | grep -q "WARN:  mutation debt"; then
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

# --- receipts after a squash-merge ----------------------------------------
#
# The owner squash-merges, so the commit a receipt names lives on a branch that
# never reaches main. What does reach main is the sources it measured. Built as
# a scratch history:
#
#   base (main.rs v1) -> squash (lib.rs added) -> docs -> docs
#
# and receipts naming a commit this repository has never had, carrying the Rust
# fingerprint of `squash` — exactly the shape a fresh clone of main sees.

sq="$scratch/squash"
mkdir -p "$sq/src"
git -C "$sq" init -q
git -C "$sq" config user.email selftest@example.com
git -C "$sq" config user.name "Gate Self-Test"
sq_commit() {
    git -C "$sq" add -A
    git -C "$sq" -c commit.gpgsign=false commit -qm "$1"
}
printf 'fn main() {}\n' > "$sq/src/main.rs"
sq_commit "base"
printf 'pub fn measured() {}\n' > "$sq/src/lib.rs"
sq_commit "squash"
sq_squash="$(git -C "$sq" rev-parse HEAD)"
sq_fp="$(cd "$sq" && rust_fingerprint HEAD)"
printf '# one\n' > "$sq/README.md"
sq_commit "docs one"
printf '# two\n' >> "$sq/README.md"
sq_commit "docs two"

gone="0123456789abcdef0123456789abcdef01234567"
other_fp="89abcdef0123456789abcdef0123456789abcdef"
sq_cov="$scratch/squash-coverage.txt"
sq_mut="$scratch/squash-mutation.txt"

in_sq() {
    ( cd "$sq" && INSPECT_COVERAGE_FLOOR_FILE="$scratch/floor.txt" \
        INSPECT_COVERAGE_RECEIPT="$sq_cov" INSPECT_MUTATION_RECEIPT="$sq_mut" "$@" )
}

# (a) The commit is gone, its sources are here: pass, and say which commit
# stood in. Three commits carry those sources; the oldest is the one named,
# because it is the one with the most drift after it.
printf '# scratch\n%s 99 %s %s\n' "$gone" "$stamp" "$sq_fp" > "$sq_cov"
printf '# scratch\n%s 40/40 0 %s %s\n' "$gone" "$stamp" "$sq_fp" > "$sq_mut"
expect_pass_saying "freshness after a squash-merge" \
    "not in this history; same Rust sources as ${sq_squash:0:12}" \
    in_sq bash "$here/check-coverage-freshness.sh"
expect_pass_saying "mutation debt after a squash-merge" \
    "not in this history; same Rust sources as ${sq_squash:0:12}" \
    in_sq bash "$here/check-mutation-debt.sh"

# A receipt at a commit whose Rust is HEAD's survives a squash-merge, and the
# gate does not warn that it would not. The warning's other direction is below,
# once HEAD has Rust the receipt did not measure.
printf '# scratch\n%s 99 %s %s\n' "$sq_squash" "$stamp" "$sq_fp" > "$scratch/sq-here.txt"
checks=$((checks + 1))
noted="$(in_sq env INSPECT_COVERAGE_RECEIPT="$scratch/sq-here.txt" \
    bash "$here/check-coverage-freshness.sh" 2>&1)" && status=0 || status=$?
if [ "$status" -eq 0 ] && ! printf '%s' "$noted" | grep -q "will not survive"; then
    echo "ok:   no squash-merge warning when HEAD's Rust is the receipt's"
else
    echo "FAIL: squash-merge warning (or failure, exit $status) with HEAD's Rust unchanged"
    echo "$noted" | sed 's/^/      /'
    failures=$((failures + 1))
fi

# The substitute is a real base, not a pass: drift past it still blocks.
printf 'fn added() {\n%s}\n' "$(for _ in $(seq 1 18); do echo '    let _ = 1;'; done)" \
    > "$sq/src/added.rs"
expect_fail "mutation debt after a squash-merge, over the line budget" "Rust lines have changed" \
    in_sq env INSPECT_MUTATION_WARN_LINES=5 INSPECT_MUTATION_FAIL_LINES=10 \
    bash "$here/check-mutation-debt.sh"
sq_commit "more rust"
expect_fail "freshness after a squash-merge, over the commit bound" "Rust-touching commits ago" \
    in_sq env INSPECT_COVERAGE_MAX_COMMITS=0 bash "$here/check-coverage-freshness.sh"

# The search is bounded, and the bound is a real one, counted in Rust-touching
# commits like the drift bounds. HEAD now changes Rust, so one deep finds
# nothing; two reaches `squash` itself, past the two docs commits that a bound
# over all commits would have spent its depth on.
expect_fail "freshness with the match past the search depth" "raise INSPECT_RECEIPT_SEARCH_DEPTH" \
    in_sq env INSPECT_RECEIPT_SEARCH_DEPTH=1 bash "$here/check-coverage-freshness.sh"
expect_pass_saying "freshness with the match just inside the search depth" \
    "same Rust sources as ${sq_squash:0:12}" \
    in_sq env INSPECT_RECEIPT_SEARCH_DEPTH=2 bash "$here/check-coverage-freshness.sh"

# The branch-side warning. A receipt whose commit is here but whose Rust is not
# HEAD's will not survive a squash-merge, and the gate says so while it can
# still be fixed — and says nothing when HEAD's Rust is the receipt's.
printf '# scratch\n%s 99 %s %s\n' "$sq_squash" "$stamp" "$sq_fp" > "$sq_cov"
expect_pass_saying "freshness with Rust changed after the receipt" "will not survive a squash-merge" \
    in_sq bash "$here/check-coverage-freshness.sh"

expect_fail "freshness with a non-numeric search depth" "non-negative integer" \
    in_sq env INSPECT_RECEIPT_SEARCH_DEPTH=banana bash "$here/check-coverage-freshness.sh"

# When the measured sources recur — the Rust is reverted to them — the oldest
# occurrence is the one drift counts from: two Rust commits after `squash`, not
# zero after the revert.
git -C "$sq" rm -q src/added.rs
sq_commit "revert more rust"
printf '# scratch\n%s 99 %s %s\n' "$gone" "$stamp" "$sq_fp" > "$sq_cov"
expect_pass_saying "freshness with the measured sources recurring" \
    "same Rust sources as ${sq_squash:0:12}), 2/10" \
    in_sq bash "$here/check-coverage-freshness.sh"
git -C "$sq" reset -q --hard HEAD~1
git -C "$sq" reset -q --hard HEAD~1

# A commit that is here but not in HEAD's history — a squash-merged branch
# whose ref was never deleted — is treated as gone, so main's answer does not
# depend on that housekeeping. `side` carries the measured Rust off to one side.
sq_side="$(git -C "$sq" commit-tree -p "$(git -C "$sq" rev-parse HEAD~3)" -m side "$sq_squash^{tree}")"
printf '# scratch\n%s 99 %s %s\n' "$sq_side" "$stamp" "$sq_fp" > "$sq_cov"
printf '# scratch\n%s 40/40 0 %s %s\n' "$sq_side" "$stamp" "$sq_fp" > "$sq_mut"
expect_pass_saying "freshness naming a commit outside HEAD's history" \
    "not in this history; same Rust sources as ${sq_squash:0:12}" \
    in_sq bash "$here/check-coverage-freshness.sh"
expect_pass_saying "mutation debt naming a commit outside HEAD's history" \
    "not in this history; same Rust sources as ${sq_squash:0:12}" \
    in_sq bash "$here/check-mutation-debt.sh"
# And one carrying Rust main never had: no match, so it fails.
sq_side_other="$(cd "$sq" && GIT_INDEX_FILE="$scratch/side.idx" sh -c "git read-tree HEAD~3 &&
    git update-index --add --cacheinfo 100644,\$(printf 'pub fn elsewhere() {}\n' | git hash-object -w --stdin),src/elsewhere.rs &&
    git commit-tree -p HEAD~3 -m side-other \$(git write-tree)")"
side_fp="$(cd "$sq" && rust_fingerprint "$sq_side_other")"
printf '# scratch\n%s 99 %s %s\n' "$sq_side_other" "$stamp" "$side_fp" > "$sq_cov"
printf '# scratch\n%s 40/40 0 %s %s\n' "$sq_side_other" "$stamp" "$side_fp" > "$sq_mut"
expect_fail "freshness naming a commit outside HEAD's history, sources not here" "not in HEAD's history" \
    in_sq bash "$here/check-coverage-freshness.sh"
expect_fail "mutation debt naming a commit outside HEAD's history, sources not here" "not in HEAD's history" \
    in_sq bash "$here/check-mutation-debt.sh"

# A receipt whose fingerprint is well-formed but is not the Rust at the commit
# it names is inconsistent, and fails on the branch — not later on main, when
# the search would find nothing. Its own fingerprint is the passing direction.
printf '# scratch\n%s 99 %s %s\n' "$sq_squash" "$stamp" "$other_fp" > "$sq_cov"
printf '# scratch\n%s 40/40 0 %s %s\n' "$sq_squash" "$stamp" "$other_fp" > "$sq_mut"
expect_fail "freshness with another commit's fingerprint" "internally inconsistent" \
    in_sq bash "$here/check-coverage-freshness.sh"
expect_fail "mutation debt with another commit's fingerprint" "internally inconsistent" \
    in_sq bash "$here/check-mutation-debt.sh"
printf '# scratch\n%s 99 %s %s\n' "$sq_squash" "$stamp" "$sq_fp" > "$sq_cov"
printf '# scratch\n%s 40/40 0 %s %s\n' "$sq_squash" "$stamp" "$sq_fp" > "$sq_mut"
expect_pass "freshness with the commit's own fingerprint" \
    in_sq bash "$here/check-coverage-freshness.sh"
expect_pass "mutation debt with the commit's own fingerprint" \
    in_sq bash "$here/check-mutation-debt.sh"

# A shallow clone is not searched: the commit may be just past its boundary,
# and the fix is to fetch history. The same receipt passes in the full history
# — case (a) above.
git clone -q --depth 1 "file://$sq" "$scratch/shallow"
printf '# scratch\n%s 99 %s %s\n' "$gone" "$stamp" "$sq_fp" > "$sq_cov"
expect_fail "freshness in a shallow clone with the commit gone" "this clone is shallow" \
    bash -c "cd '$scratch/shallow' && INSPECT_COVERAGE_FLOOR_FILE='$scratch/floor.txt' \
        INSPECT_COVERAGE_RECEIPT='$sq_cov' bash '$here/check-coverage-freshness.sh'"

# A commit the fingerprint cannot read must not hash to a fingerprint: a failed
# diff-tree piped into hash-object would otherwise produce one. Without
# pipefail, so it is the function refusing and not the caller's shell options.
checks=$((checks + 1))
if (set +o pipefail; cd "$sq" && rust_fingerprint "$gone" >/dev/null 2>&1); then
    echo "FAIL: rust_fingerprint of a missing commit succeeded"
    failures=$((failures + 1))
else
    echo "ok:   rust_fingerprint refuses a commit it cannot read"
fi

# (b) The commit is gone and so are its sources.
printf '# scratch\n%s 99 %s %s\n' "$gone" "$stamp" "$other_fp" > "$sq_cov"
printf '# scratch\n%s 40/40 0 %s %s\n' "$gone" "$stamp" "$other_fp" > "$sq_mut"
expect_fail "freshness with the commit and its sources both gone" "has the Rust sources it measured" \
    in_sq bash "$here/check-coverage-freshness.sh"
expect_fail "mutation debt with the commit and its sources both gone" "has the Rust sources it measured" \
    in_sq bash "$here/check-mutation-debt.sh"

# (c) An old-format receipt has nothing to search by: a missing commit fails as
# it always did, never passes quietly.
printf '# scratch\n%s 99 %s\n' "$gone" "$stamp" > "$sq_cov"
printf '# scratch\n%s 40/40 0 %s\n' "$gone" "$stamp" > "$sq_mut"
expect_fail "freshness, old-format receipt, commit gone" "records no Rust fingerprint" \
    in_sq bash "$here/check-coverage-freshness.sh"
expect_fail "mutation debt, old-format receipt, commit gone" "records no Rust fingerprint" \
    in_sq bash "$here/check-mutation-debt.sh"

# (d) A fingerprint of the wrong shape is a receipt nothing wrote. Checked
# against the commit that *is* here, so the shape check cannot hide behind the
# search.
for bad in "not-hex-at-all" "abc123"; do
    printf '# scratch\n%s 99 %s %s\n' "$sq_squash" "$stamp" "$bad" > "$sq_cov"
    printf '# scratch\n%s 40/40 0 %s %s\n' "$sq_squash" "$stamp" "$bad" > "$sq_mut"
    expect_fail "freshness with fingerprint '$bad'" "Rust fingerprint" \
        in_sq bash "$here/check-coverage-freshness.sh"
    expect_fail "mutation debt with fingerprint '$bad'" "Rust fingerprint" \
        in_sq bash "$here/check-mutation-debt.sh"
done
# The fingerprint of no Rust at all would match any commit without Rust; it is
# refused by name, not found inconsistent by luck.
printf '# scratch\n%s 99 %s %s\n' "$gone" "$stamp" "$EMPTY_FINGERPRINT" > "$sq_cov"
printf '# scratch\n%s 40/40 0 %s %s\n' "$gone" "$stamp" "$EMPTY_FINGERPRINT" > "$sq_mut"
expect_fail "freshness with the fingerprint of no Rust" "no Rust at all" \
    in_sq bash "$here/check-coverage-freshness.sh"
expect_fail "mutation debt with the fingerprint of no Rust" "no Rust at all" \
    in_sq bash "$here/check-mutation-debt.sh"
printf '# scratch\n%s 99 %s %s extra\n' "$sq_squash" "$stamp" "$sq_fp" > "$sq_cov"
expect_fail "freshness with a trailing field" "malformed" \
    in_sq bash "$here/check-coverage-freshness.sh"

# --- mutation re-stamp ----------------------------------------------------
#
# `just mutants` with no Rust changed since the receipt has nothing to mutate.
# It moves a 0-survivor receipt onto HEAD — the same measurement of the same
# sources — and nothing else. Driven in the squash history: HEAD is two docs
# commits past `squash`.
restamp() { in_sq bash "$here/run-mutants.sh"; }

# Refused: survivors were left. The receipt must come out byte-identical.
printf '# scratch\n%s 38/40 2 %s %s\n' "$sq_squash" "$stamp" "$sq_fp" > "$sq_mut"
cp "$sq_mut" "$scratch/before.txt"
expect_pass_saying "re-stamp of a receipt with survivors" "would carry them forward" restamp
checks=$((checks + 1))
if cmp -s "$sq_mut" "$scratch/before.txt"; then
    echo "ok:   a receipt with survivors is not re-stamped"
else
    echo "FAIL: a receipt with survivors was re-stamped"
    failures=$((failures + 1))
fi

# Refused: the receipt's fingerprint is not the Rust at its commit.
printf '# scratch\n%s 40/40 0 %s %s\n' "$sq_squash" "$stamp" "$other_fp" > "$sq_mut"
cp "$sq_mut" "$scratch/before.txt"
expect_fail "re-stamp of a receipt whose fingerprint disagrees" "internally inconsistent" restamp
checks=$((checks + 1))
if cmp -s "$sq_mut" "$scratch/before.txt"; then
    echo "ok:   a receipt whose fingerprint disagrees is not re-stamped"
else
    echo "FAIL: a receipt whose fingerprint disagrees was re-stamped"
    failures=$((failures + 1))
fi

# Refused: a receipt the debt gate would reject is not rewritten into one it
# accepts.
for bad_line in "$sq_squash 40/40 0 $stamp $sq_fp extra" "$sq_squash 40/40 none $stamp $sq_fp"; do
    printf '# scratch\n%s\n' "$bad_line" > "$sq_mut"
    cp "$sq_mut" "$scratch/before.txt"
    expect_fail "re-stamp of the receipt '${bad_line#* }'" "mutation receipt" restamp
    checks=$((checks + 1))
    if cmp -s "$sq_mut" "$scratch/before.txt"; then
        echo "ok:   a malformed receipt is not re-stamped"
    else
        echo "FAIL: a malformed receipt was re-stamped"
        failures=$((failures + 1))
    fi
done

# Refused: the Rust is the same but a build input is not. An allium_parser bump
# lands in Cargo.lock alone, and changes what the tests see.
printf 'bumped\n' > "$sq/Cargo.lock"
sq_commit "bump dep"
printf '# scratch\n%s 40/40 0 %s %s\n' "$sq_squash" "$stamp" "$sq_fp" > "$sq_mut"
cp "$sq_mut" "$scratch/before.txt"
expect_pass_saying "re-stamp across a Cargo.lock change" "inputs the run depended on changed" restamp
checks=$((checks + 1))
if cmp -s "$sq_mut" "$scratch/before.txt"; then
    echo "ok:   a receipt is not re-stamped across a build-input change"
else
    echo "FAIL: a receipt was re-stamped across a build-input change"
    failures=$((failures + 1))
fi
git -C "$sq" reset -q --hard HEAD~1

# Accepted: 0 survivors, same sources, commit gone after a squash-merge. The
# new receipt names HEAD, keeps the score and the time the mutants actually
# ran, and carries HEAD's fingerprint.
printf '# scratch\n%s 40/40 0 %s %s\n' "$gone" "$stamp" "$sq_fp" > "$sq_mut"
expect_pass_saying "re-stamp of a 0-survivor receipt over the same sources" "re-stamped" restamp
checks=$((checks + 1))
sq_head="$(git -C "$sq" rev-parse HEAD)"
if [ "$(receipt_line "$sq_mut")" = "$sq_head 40/40 0 $stamp $sq_fp" ]; then
    echo "ok:   the re-stamped receipt names HEAD and keeps the measurement"
else
    echo "FAIL: the re-stamped receipt reads '$(receipt_line "$sq_mut")'"
    failures=$((failures + 1))
fi
checks=$((checks + 1))
if grep -q "^# Re-stamped from $gone" "$sq_mut"; then
    echo "ok:   the re-stamped receipt says where the mutants actually ran"
else
    echo "FAIL: the re-stamped receipt does not say where it came from"
    failures=$((failures + 1))
fi
expect_pass_saying "mutation debt reading a re-stamped receipt" "since ${sq_head:0:12}" \
    in_sq bash "$here/check-mutation-debt.sh"

# Running it again changes nothing: the receipt already names HEAD.
cp "$sq_mut" "$scratch/before.txt"
expect_pass "re-stamp of a receipt already at HEAD" restamp
checks=$((checks + 1))
if cmp -s "$sq_mut" "$scratch/before.txt"; then
    echo "ok:   a receipt already at HEAD is left alone"
else
    echo "FAIL: a receipt already at HEAD was rewritten"
    failures=$((failures + 1))
fi

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
    expect_fingerprint_field "a mutation scoring receipt" "$score_receipt" 5
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
