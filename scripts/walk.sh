#!/usr/bin/env bash
set -euo pipefail

# A browser walk over the tool's own UI, sealed into evidence.
#
# Build, serve the repository's own specs on a free port, drive the journey,
# seal what it photographed, stop the server. What comes out is
# `target/evidence/`, which `just run --evidence target/evidence/` then shows
# under the steps the pictures are of.
#
# Not in `just check`. It needs a browser downloaded and a binary built, and it
# costs about as long as the rest of the suite together — the same reasoning
# that keeps `just mutants` a decision rather than a step.
#
# The walk asserts almost nothing. A journey's verdicts are the assertion; this
# produces the half a walk cannot reach.

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
cd "$root"

evidence="${INSPECT_EVIDENCE:-$root/target/evidence}"
journeys="$root/specs/journeys"
specs="$root/specs"

# A free port from the kernel rather than a fixed one, so a walk does not fail
# because the tool is already open in a browser beside it.
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"

if [ ! -d ui/node_modules ]; then
    echo "walk: ui/node_modules missing — run 'just ui-install'" >&2
    exit 1
fi

if ! (cd ui && npx playwright --version >/dev/null 2>&1); then
    echo "walk: playwright is not installed — run 'just ui-install'" >&2
    exit 1
fi

echo "walk: building"
just build

server=""
cleanup() {
    if [ -n "$server" ] && kill -0 "$server" 2>/dev/null; then
        kill "$server" 2>/dev/null || true
        wait "$server" 2>/dev/null || true
    fi
}
trap cleanup EXIT

echo "walk: serving $specs on 127.0.0.1:$port"
./target/debug/allium-inspect --no-open --no-watch --port "$port" \
    --journeys "$journeys" "$specs" >/dev/null 2>&1 &
server=$!

# Wait for it to answer rather than sleeping a guess. A walk that starts against
# a socket nobody is listening on photographs a browser error page, and a
# picture of one of those is worse than none: it would be sealed and believed.
ready=""
for _ in $(seq 1 60); do
    if curl -fsS "http://127.0.0.1:$port/api/health" >/dev/null 2>&1; then
        ready=yes
        break
    fi
    if ! kill -0 "$server" 2>/dev/null; then
        echo "walk: the server exited before it answered" >&2
        exit 1
    fi
    sleep 0.5
done
if [ -z "$ready" ]; then
    echo "walk: the server never answered on 127.0.0.1:$port" >&2
    exit 1
fi

echo "walk: walking"
walked=0
(cd ui && INSPECT_URL="http://127.0.0.1:$port" INSPECT_EVIDENCE="$evidence" \
    npx playwright test) || walked=$?

# Sealed either way. A walk that stopped part way through is the interesting
# case — the frame at the point of failure is usually the most informative one
# there is — and refusing to seal it would throw exactly that away.
echo "walk: sealing"
./target/debug/allium-journey evidence seal "$evidence" "$journeys" --walk reading-a-spec

echo
./target/debug/allium-journey evidence check "$evidence" \
    --journeys "$journeys" --code "$root/ui/e2e" --code "$root/crates" \
    --code "$root/apps" --report

echo
echo "walk: see it with — just run --evidence $evidence --code ui/e2e --code crates --code apps $specs"

exit "$walked"
