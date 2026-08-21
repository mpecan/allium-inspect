#!/usr/bin/env bash
set -euo pipefail

# Re-record the `allium` CLI outputs the model tests read.
#
# Four documents, recorded for two different reasons now.
#
# `model` and `plan` are *replayed*: allium builds them in a crate with only a
# `[[bin]]` target, so nothing can import them, and the ingestion tests must not
# shell out — they would need the CLI installed to run at all, they would take a
# process launch per case, and a CLI upgrade would change their input underneath
# them without anything saying so.
#
# `parse` and `analyse` are *compared against*. Those are `allium_parser`
# function calls, pinned in Cargo.toml to the tag recorded here, so no test
# replays them. They are kept because a reader who runs `allium check` and then
# opens this tool has to be shown the same spec: tests/agreement.rs asserts the
# library we call still says what the binary they run says.
#
# The pinning is the point either way, so the CLI version is stamped alongside
# the recordings and a test asserts the installed CLI still matches. An upgrade
# then surfaces as one loud, specific failure telling you to re-run this script
# — and to move the tag in Cargo.toml with it — rather than as a misparse three
# layers down.
#
# The fixture specs are ours, not a sibling repo's: between catalogue.allium and
# lending.allium every construct allium-inspect draws has an instance, they
# cross-reference each other so the linker has something real to resolve, and
# they are small enough that the recordings can live in git.

. "$(dirname "$0")/common.sh"

here="$(cd "$(dirname "$0")/.." && pwd)"
specs="$here/crates/inspect-model/tests/fixtures/specs"
out="$here/crates/inspect-model/tests/fixtures/cli"

command -v allium >/dev/null 2>&1 ||
    fail "the allium CLI is not on PATH — this script records its real output, so it needs one"
command -v jq >/dev/null 2>&1 ||
    fail "jq is not on PATH — it is used to minify the recordings"

[ -d "$specs" ] || fail "fixture specs not found: $specs"

version="$(allium --version)"
[ -n "$version" ] || fail "allium --version printed nothing"

mkdir -p "$out"
recorded=0

for spec in "$specs"/*.allium; do
    name="$(basename "$spec" .allium)"
    for command in model parse plan analyse; do
        # `set +e` around the call: these commands exit non-zero when they have
        # something to report — `analyse` whenever it finds anything at all —
        # and the document describing it is still on stdout. Treating that as a
        # failure would record nothing for exactly the specs worth recording.
        set +e
        json="$(cd "$specs" && allium "$command" "$name.allium" 2>/dev/null)"
        set -e
        [ -n "$json" ] ||
            fail "allium $command produced no output for $name.allium"
        printf '%s' "$json" | jq -c . > "$out/$name.$command.json" ||
            fail "allium $command produced output that is not JSON for $name.allium"
        recorded=$((recorded + 1))
    done
done

[ "$recorded" -gt 0 ] || fail "no fixture specs matched — nothing was recorded"

printf '%s\n' "$version" > "$out/VERSION"

echo "Recorded $recorded documents from $version into ${out#"$here/"}"
