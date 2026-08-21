# Contributing

Thanks for looking. This is a small project with an unusual amount of
scaffolding, and the point of this file is to explain the scaffolding so it
helps you instead of getting in your way.

## Getting set up

You need:

- **Rust 1.96** — `rustup toolchain install 1.96`, or just let `rust-toolchain.toml` do it
- **Node 20+** for the frontend
- **[`just`](https://github.com/casey/just)** — `brew install just`
- **the `allium` CLI** on `PATH` — `brew install juxt/allium/allium`

Then:

```sh
just ui-install     # once
just build
just run crates/inspect-model/tests/fixtures/specs/
```

That last one opens a browser on the fixture spec set in about two seconds. It
is the fastest way to see whether a change did what you meant.

Two more you will want when you touch the gates:

```sh
cargo install cargo-nextest cargo-llvm-cov cargo-mutants cargo-deny
```

## The loop

```sh
just check      # about a minute — run this before every commit
```

`just check` is format, clippy, docs, the whole test suite, file-size budgets,
the gate self-test, generated-type freshness, the frontend, and two receipt
checks. If it exits 0 you are in good shape.

```sh
just check-all  # adds measured coverage, a mutation pass and a licence audit
```

`just --list` for everything else. The ones you will reach for most:

| | |
|---|---|
| `just test` | the suite on its own |
| `just ui-dev` | Vite with hot reload, against a running backend |
| `just types` | regenerate the TypeScript from the Rust structs |
| `just graph specs/` | the whole SpecGraph as JSON, no browser |
| `just mutants` | a mutation pass — minutes, and worth it |

## What "done" means here

1. **`just check` exits 0** — the whole recipe, not the part you ran.
2. **The tests you wrote ran, passed, and you saw them fail first.** A test that
   has never failed has not been shown to test anything.
3. **No new code is unreachable** from a test or a running path.
4. **Every new dependency has a one-line comment** in `Cargo.toml` saying who
   uses it and for what.
5. **Anything touching the graph or the simulator has been run in the browser.**
   Not reviewed — run. Three defects have shipped past a green suite and been
   found in the first minute of using it.
6. **Anything you could not finish is said plainly** in the pull request, not
   left implied by silence.

## The gates, and why they are shaped like that

Most of this is ordinary. Three things are not, and they are the ones worth
understanding before you fight them.

### A gate that cannot fail is worse than no gate

It reports success and suppresses scrutiny. So every gate is driven **in both
directions** by `just gates-selftest`, which runs inside `just check`: each one
is given input that must break it, and the run fails if it does not.

If you add or change a gate, prove both directions — construct the failing
input, watch the non-zero exit, then confirm the clean tree still passes. Shell
being shell, the failure modes are real: `find … | while read; do status=1; done`
discards the assignment in a subshell and always exits 0.

### Mutation testing is a decision, not a step

Coverage says a line ran. Mutation asks whether any assertion would notice it
changing. It is the strongest signal here and it costs minutes, so it does not
run on every commit — a gate people skip is a gate that cannot fail.

What runs automatically is `just mutation-debt`, which measures how much Rust
has changed since the last recorded run and escalates: silent under 250 lines,
a warning to 500, and a block past that or past 20 Rust-touching commits.

When it blocks, run `just mutants`. The survivor baseline in
`scripts/mutant-baseline.txt` ratchets downward only. A survivor is a change to
the code that no assertion noticed — **write the assertion**, or raise the
baseline deliberately with a reason in the commit message. Raising it quietly is
the one thing that makes the whole gate pointless.

### Coverage is enforced by receipt

Measuring on every commit is too slow, so `just coverage-check` writes a receipt
and the sub-second `just coverage-fresh` blocks once HEAD is more than ten
Rust-touching commits past it. A green `just check` therefore says *coverage was
measured recently and cleared the floor*, not *coverage passes now*. The drift
bound is what keeps that gap small.

## Stipulations

`CLAUDE.md` holds eight decisions everything else rests on. Changing one is a
decision to make deliberately, not a refactor to make in passing. Read them
before a change that seems to conflict with one — they are short, and each says
why. The two that catch people:

- **The simulator never guesses.** An Allium expression can be true, false, or
  *undecidable by this simulator*. The third is the honest answer, not a defect
  to paper over. Every path returning `Unknown` must attach a reason saying which
  sub-expression and why; a test asserts this as a property.
- **Byte offsets are not string indices.** The parser counts bytes, JavaScript
  indexes UTF-16, a reader expects characters. All three agree for ASCII, which
  is exactly why conflating them survives every test written against an ASCII
  fixture and then puts the highlight a line off on the first spec with an
  em-dash in a comment.

## Fixtures

The tests replay recorded `allium` output so they need no binary installed, run
in milliseconds and are pinned. Recordings live in
`crates/inspect-model/tests/fixtures/cli/`, stamped with the CLI version that
made them.

`just refresh-fixtures` re-records them. **Read the diff** — a shape change
upstream is exactly what those recordings exist to surface. If you refresh them,
move the `allium-parser` tag in `Cargo.toml` to match, or the library and the
recordings drift apart and `tests/agreement.rs` will say so.

## Commits

Conventional Commits: `type(scope): description`.

- types: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`, `ci`, `perf`
- scopes: `model`, `sim`, `journey`, `server`, `app`, `ui`

Say *why* in the body, not what — the diff already says what. Stage explicit
paths; never `git add -A`, which sweeps in whatever else is in the tree.

## Filing something instead

A bug report is welcome and does not need any of the above. The most useful
thing you can include is the spec that provoked it, or the smallest part of it
that still does — this tool is entirely about reading specs, so a spec is a
reproduction.

If you are reporting something about the interface, say which of the five people
in [`docs/personas/`](docs/personas/) you were being. "The rail is cluttered" and
"the rail is thorough" are the same observation from two people with different
jobs, and only naming the job settles it.

## Licence

This project is MIT — see [`LICENSE`](LICENSE). Contributions are accepted under
the same terms: open a pull request and you are offering your work under MIT,
which is what lets any of it graduate upstream into allium without a second
conversation.

If you add a frontend dependency, `just third-party-check` will tell you whether
its licence is one the bundle can carry. Adding it to the allowed set in
`scripts/third-party.mjs` is a deliberate decision rather than a formality: the
binary embeds that code, and its notice has to travel with it.
