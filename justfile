# === default ===
default:
    @just --list

# === quality gates ===
#
# The fast gates. Run this before every commit; it aims to stay under a minute.
#
# Mutation is deliberately NOT here — see `mutants` — but `mutation-debt` is:
# it is sub-second, and it is the ceiling on how long the real run can be put
# off. Coverage is here by proxy for the same reason: `coverage-fresh` reads the
# receipt that `coverage-check` writes, so the slow measurement stays out of the
# hot path while the drift stays bounded.

# The fast gates: format, lint, doc, tests, budgets, coverage and mutation debt.
check: fmt-check lint doc test file-size gates-selftest types-check coverage-fresh mutation-debt ui-guard third-party-check ui-check ui-test

# Everything, nothing skipped for speed: the fast gates plus mutation, the
# dependency audit and a real coverage measurement.

# Everything: the fast gates plus mutation, cargo-deny and measured coverage.
check-all: check mutants deny coverage-check

# `just ci` is an alias for check-all, not a second copy of it.
ci: check-all

# === formatting ===

# Format the workspace.
fmt:
    cargo fmt --all

# Fail if anything is unformatted.
fmt-check:
    cargo fmt --all -- --check

# === linting ===
# unwrap/expect/panic are denied outside tests: this is a tool people point at
# their own specs, and a panic on a spec shape we did not anticipate is a crash
# report instead of a diagnostic.

# Clippy across the workspace, denying unwrap/expect/panic outside tests.
lint:
    cargo clippy --workspace --all-targets --all-features -- \
        -D warnings \
        -D clippy::unwrap_used \
        -D clippy::expect_used \
        -D clippy::panic \
        -D clippy::todo \
        -D clippy::unimplemented \
        -D clippy::dbg_macro

# === testing ===

# Run the whole test suite under nextest.
test:
    cargo nextest run --workspace --profile ci

# Re-run tests on change (needs cargo-watch).
test-watch:
    cargo watch -x 'nextest run --workspace'

# === docs ===

# Build the docs, denying warnings.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --document-private-items

# === source budgets ===

# 500 source lines warns, 700 fails. Test modules exempt.
file-size:
    @bash scripts/check-file-sizes.sh

# The gates, driven in both directions. Sub-second, and in `check`: a gate that
# cannot fail is worse than no gate, so the proof that each one can runs as
# often as the gates themselves.

# Prove every gate can fail: each is driven against input that must break it.
gates-selftest:
    @bash scripts/gates-selftest.sh

# === coverage ===

# Measure coverage into lcov.info.
coverage:
    cargo llvm-cov nextest --workspace --profile ci --lcov --output-path lcov.info

# The measurement, plus the floor. Writes the receipt `coverage-fresh` reads.
coverage-check: coverage
    @bash scripts/check-coverage.sh lcov.info

# Sub-second: asserts coverage was *measured* recently and cleared the floor,
# not what it is now.

# Sub-second: assert coverage was measured recently and cleared the floor.
coverage-fresh:
    @bash scripts/check-coverage-freshness.sh

# Raise the floor toward 95% when the suite has earned it. Never lowers.
coverage-ratchet: coverage
    @bash scripts/coverage-ratchet.sh lcov.info

# Open an HTML coverage report.
coverage-report:
    cargo llvm-cov nextest --workspace --profile ci --html
    @echo "Report: target/llvm-cov/html/index.html"

# === mutation ===
#
# Out of `check` on purpose. Mutation is the strongest signal here — coverage
# says a line ran, mutation asks whether any assertion would notice it changing
# — and it costs minutes. Run it when the assurance is worth buying: after
# writing tests you want to trust, when hardening the evaluator, before a
# release. Nothing runs it for you.
#
# What *is* automatic is the debt gate below, which measures how much Rust has
# changed since the last run and blocks once that exceeds the budget. So the
# decision stays yours, and the deferral is bounded.

# Run a diff-scoped mutation pass and refresh the receipt. Minutes.
mutants:
    @bash scripts/run-mutants.sh

# === the walk ===
#
# A browser over the tool's own UI, photographing the journey it is walking.
# Not in `check`: it needs a browser downloaded and costs minutes, so it is a
# decision like `mutants` rather than a step. What it leaves behind is
# `target/evidence/`, which `just run --evidence target/evidence/` shows under
# the steps the pictures are of.

# Walk this repository's own journey and seal what it photographed. Minutes.
walk:
    @bash scripts/walk.sh

# Sub-second, in `check`: the code-volume metric that decides when `mutants`
# stops being optional.

# Sub-second: how much Rust has changed since the last mutation run.
mutation-debt:
    @bash scripts/check-mutation-debt.sh

# === dependencies ===

# Audit licences, advisories and sources.
deny:
    cargo deny check

# === generated wire types ===
#
# ui/src/lib/api/ is generated from the Rust structs by ts-rs, so the
# browser and the server cannot disagree about the shape of a SpecGraph. The
# check regenerates and fails on any diff — a hand-edit of the generated file,
# or a Rust change that was not propagated, both show up here rather than as a
# runtime undefined three views later.
#
# ts-rs writes one file per type. They are committed, not gitignored: the whole
# point of the check is `git diff`, and a generated file nothing tracks is a
# file nothing reviews.

# Regenerate the TypeScript bindings from the Rust structs.
types:
    cargo test --workspace export_bindings

# Fail if the generated types are stale.
types-check: types
    @git diff --exit-code --stat -- ui/src/lib/api/ > /dev/null || \
        (echo "ERROR: generated bindings in ui/src/lib/api/ are stale." && \
         echo "       'just types' has regenerated them — review and commit the result." && \
         git diff --stat -- ui/src/lib/api/ && exit 1)
    @test -z "$(git status --porcelain --untracked-files=all -- ui/src/lib/api/)" || \
        (echo "ERROR: 'just types' produced bindings that are not tracked by git:" && \
         git status --porcelain --untracked-files=all -- ui/src/lib/api/ && \
         echo "       An untracked binding is invisible to the diff above, so a new" && \
         echo "       type could go unreviewed forever. Commit them." && exit 1)

# === third-party notices ===
#
# The binary embeds the built bundle, which is npm code `cargo deny` never
# sees. ISC, BSD and MIT all require their notice to travel with a binary, and
# Vite strips every legal comment on the way out.

# Regenerate the notice the shipped bundle carries.
third-party:
    node scripts/third-party.mjs

# Fail if the notice does not match what the bundle actually ships.
third-party-check:
    @node scripts/third-party.mjs --check

# === frontend ===
#
# The UI is part of what ships, so its gates are in `check` rather than left to
# a separate command nobody runs. `ui-guard` turns a missing `npm install` into
# a loud failure instead of a silently skipped gate.

# Install the frontend's dependencies.
ui-install:
    cd ui && npm install

# Fail loudly when the frontend was never installed.
ui-guard:
    @test -d ui/node_modules || \
        (echo "ERROR: ui/node_modules missing — run 'just ui-install'" && exit 1)

# Typecheck the frontend (svelte-check + tsc).
ui-check:
    cd ui && npm run check

# Run the frontend test suite.
ui-test:
    cd ui && npm run test

# Build the frontend into the assets the binary embeds.
ui-build:
    cd ui && npm run build

# Vite dev server with hot reload, against a running backend.
ui-dev:
    cd ui && npm run dev

# === build and run ===

# Build the frontend, then the workspace.
build: ui-build
    cargo build --workspace

# Optimised build of both.
build-release: ui-build
    cargo build --workspace --release

# === install ===
#
# The one recipe that must build the frontend first, and the reason it exists
# rather than leaving people to `cargo install`.
#
# `allium-inspect` embeds the built interface with `rust-embed`, from
# `crates/inspect-server/assets/`. `cargo install` on its own bakes in whatever
# that directory happens to hold, which after a UI change is the *previous*
# bundle — so the binary comes out serving stale JavaScript, and nothing on
# screen says so. It looks exactly like the change not working. Twice now.
#
# Both binaries, because the pair is the product. `allium-inspect` and
# `allium-journey` share `inspect-journey` and `inspect-sim`, so installing one
# after a change to either leaves the other answering differently about the
# same specification — the drift `tests/agreement.rs` exists to prevent one
# layer further down.
#
# `--locked` on purpose: `allium-parser` is pinned to the tag the fixtures were
# recorded from, and an install that quietly resolved a different one would
# make the installed binary disagree with everything `just check` proved.
#
# rustup prints a note that the toolchain file overrode the default. That is
# the pin in `rust-toolchain.toml` doing its job; `+stable` would undo it.

# Build the frontend, then install both binaries onto PATH.
install: ui-guard ui-build
    cargo install --path apps/inspect --locked
    cargo install --path apps/journey --locked
    @echo
    @echo "installed:"
    @command -v allium-inspect && allium-inspect --version
    @command -v allium-journey && allium-journey --version

# === run ===

# Point it at a spec directory and it opens a browser.
run *ARGS:
    cargo run -p inspect -- {{ARGS}}

# The whole SpecGraph as JSON, no browser. The scriptable path, and what the
# end-to-end test asserts against.

# Print the whole SpecGraph as JSON and exit. No browser.
graph *ARGS:
    @cargo run -q -p inspect -- --print-graph {{ARGS}}

# === fixtures ===
#
# Re-records the four `allium` outputs for the fixture specs. Run it when the
# installed CLI version changes; the version is stamped into the fixtures so a
# mismatch surfaces as a failing test rather than a silent misparse.

# Re-record the allium CLI outputs the model tests run against.
refresh-fixtures:
    @bash scripts/refresh-fixtures.sh
