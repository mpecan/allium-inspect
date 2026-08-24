# allium-inspect

A local tool that reads a spec set — two commands from the `allium` CLI, two
calls into its parser library — and serves an explorable graph, a rule simulator
and a journey runner in a browser. See `README.md` for what it does and
why; this file is about working on it.

## Layout

```
crates/inspect-model   allium output → one linked SpecGraph, plus view projections
crates/inspect-sim     three-valued evaluator, world, step engine
crates/inspect-journey journeys: grammar, static check, and the walk
crates/inspect-server  axum routes; the built UI embedded via rust-embed
apps/inspect           args, a free port, a browser, a file watcher
apps/journey           `allium-journey`: the same walk without a browser
ui                     Svelte 5; wire types generated from the Rust by ts-rs
```

## Stipulations

These are the decisions everything else rests on. Changing one is a decision to
make deliberately, not a refactor to make in passing.

### 1. The simulator never guesses

An Allium expression can be true, false, or **undecidable by this simulator**.
The third case is not a defect to be papered over; it is the honest answer, and
`Truth::Unknown` carries the sub-expression and span that could not be settled.

Two failure modes to watch for in any change:

- treating unknown as false — rules get reported blocked by a precondition
  nothing checked;
- treating unknown as true — rules fire that should not have.

Both make the simulator state a conclusion it did not reach, which is worse than
useless in a tool people use to trust a specification.

**An unknown with no reason is indistinguishable from a bug.** Every path that
returns `Value::Unknown` must attach an `Unresolved` saying which sub-expression
and why. `crates/inspect-sim/tests/eval.rs` asserts this as a property.

### 2. Determinism is not negotiable

Ordered maps throughout, monotonic entity ids, and `now` is a field the user
advances — never a reading of the system clock. The same world and event produce
a byte-identical outcome.

Everything downstream assumes it: snapshot tests, a shared link, a diff between
two versions of a spec, and mutation testing having any signal at all. `HashMap`
iteration order or `Instant::now()` anywhere in the two pure crates breaks all
four at once.

### 3. Show what the author wrote

Clause and expression text is sliced from the spec source by byte span, never
reconstructed from the AST. The person reading the panel wrote the file; showing
them a normalised spelling of their own rule is a small lie that costs trust.

The one processing applied is dropping `--` comments from the collapsed one-line
form, because a surface's `exposes` block is mostly prose in a real spec and
collapsing it verbatim buries the field list in an essay. The untouched text is
one click away in the source strip.

### 4. Byte offsets are not string indices

The parser counts **bytes**. JavaScript indexes strings in **UTF-16 units**. A
person reading a column expects **characters**. All three agree for ASCII, which
is exactly why conflating them survives every test written against an ASCII
fixture and then puts the highlight a line off on the first spec with an em-dash
in a comment — which is every real spec.

Any test touching spans must build them from byte offsets
(`TextEncoder().encode(text.slice(0, at)).length`), not from `indexOf`.

### 5. Allium is a library where it can be, and a process where it must be

`parse` and `analyse` are `allium_parser` calls — allium's own crate, MIT,
pinned in `Cargo.toml` to the tag the recordings were made from. `model` and
`plan` are still process launches, and not by choice: allium builds those in
`crates/allium`, which declares only a `[[bin]]` target, so they cannot be
imported at any price.

That split is the whole of `Command`. Prefer the library end of it: against a
library, an upstream shape change is a **compile error**, which is what the rest
of this stipulation exists to compensate for at the other end.

`AlliumRunner` is still the impure seam for the two commands that need one, and
ingestion and simulation still run with no `allium` installed. Recordings live
in `crates/inspect-model/tests/fixtures/cli/` and are stamped with the CLI
version that produced them; a test fails loudly when the installed version
differs. Refresh with `just refresh-fixtures` and **read the diff**.

All four documents are still recorded, for two different reasons. `model` and
`plan` are *replayed* by the tests. `parse` and `analyse` are *compared against*
by `tests/agreement.rs`, which asserts the library we call still says what the
binary a reader runs says — the two must not drift, or `allium check` and this
tool would describe the same file differently.

Hand-built fixtures prove the code does what you believed the CLI emits. Only
recorded output proves the belief was right. Four wrong beliefs have been caught
this way, including `external entity` being its own AST block kind and a
when-guard naming its target `action`. A fifth was caught by the library swap
itself: a diagnostic whose location carried no `file` is discarded whole, and
silently stops badging the construct it is about.

### 6. Ingest all four documents

`model` describes entities and carries no spans. `parse` is the only source of
rules, surfaces, actors, invariants and positions. `plan` supplies the trigger →
rule → entity chain already computed. `analyse` contributes findings. The passes
run in that order per file, and linking runs once over the whole set.

The two halves are typed differently, on purpose. The **evaluator** walks
`allium_parser::ast::Expr` directly — `inspect-sim` has `serde_json` under
dev-dependencies only, and a shape it does not handle is a compile error rather
than a run-time `unknown`. The **ingestion passes** still read the AST as JSON,
because what they want from it is spans and names rather than structure, and a
walker over ~35 typed variants to reach a field name would be more code saying
less. `ingest/clauses.rs` and `ingest/writes.rs` are the two that went typed,
because both actually care about the shape.

Converting the rest is a separate decision and not an obviously good one.

The expression trees go in `Program`, not in `SpecGraph`. The graph is what the
browser draws and is under half a megabyte for a five-module spec set; the ASTs
are an order of magnitude larger and only the simulator reads them.

### 7. Gates must be able to fail

Every gate is driven in both directions by `scripts/gates-selftest.sh`, which
runs inside `just check`. A gate that cannot fail is worse than no gate: it
reports success and suppresses scrutiny.

Shapes that have shipped green-forever in sister repos:

- `find … | while read; do status=1; done` — the body is a subshell, so the
  assignment is discarded and the gate always exits 0. Use `done < <(find …)`.
- `covered=$(grep -c '^DA:' f | grep -v ',0$')` — a *count* piped into a filter,
  so the value is always 1.
- `grep -rql` — `-q` suppresses the list `-l` asks for.

**When you add or change a gate, prove both directions**: construct an input that
must fail, see the non-zero exit, then confirm the clean tree still passes.

### 8. Never edit a shell script while it is running

Bash reads a script incrementally by byte offset. Editing one mid-execution makes
it resume mid-token. This happened here: a 16-minute mutation run completed and
then died in its own scoring step on `cope: command not found`.

### 9. Evidence says which step, or it says nothing

A journey verdict answers *does the specification support this step*. It does
not answer *does the software do it*, and the second is what a reader assumes
they are being told unless the two are kept apart. So they are: separate
counts, separate marks, and separate words in `journeys/evidence.ts`.

The chain is a marker in code, a log a harness appends to, and a seal:

```
// journey: SomebodyMeetsASpecTheyDidNotWrite.3
```

Three rules hold it together, and each exists because the alternative is a
quiet lie rather than an untidiness.

- **`seal` refuses.** A frame naming a step no journey has is a rename half
  done. Dropping it would leave that step reading as never covered.
- **A sealed frame stores the step's words, not a hash of them.** When they
  stop matching, a reader can be shown what the step said then beside what it
  says now. "Digest mismatch" is a fact about the tool, not about the step.
- **The code is scanned as well as the manifest.** Without markers, a harness
  that quietly stopped photographing and a step nobody ever covered leave the
  same trace — none — and the first would be reported as the second. That is
  stipulation 7 one layer out.

A step may be photographed more than once, and a frame's `tags` say what each
picture is *of* beyond which step: `theme: dark`, `platform: ios`. **Named, not
bare**, and that is the whole reason the panel can offer a dropdown: the name
says which pictures are alternatives to each other, so picking `dark` declines
`light` and says nothing about `platform`. A flat `["dark", "ios"]` would leave
a reader to work out which of a pile of words were answers to the same question.

The tool never learns what a key means. Anything a harness names becomes an
axis, two harnesses with different vocabularies produce two axes rather than an
argument, and a picture that says nothing on an axis is shown whatever is picked
on it — silence is not disagreement.

`just walk` is the producer and is **not** in `check`: it needs a browser
downloaded and costs minutes, so it is a decision like `mutants`. The walk
asserts almost nothing on purpose — a journey's verdicts are the assertion, and
a browser test re-litigating them would be a second opinion with no standing.

## Quality gates

| Gate | Recipe | In `check`? |
|---|---|---|
| Format | `just fmt-check` | yes |
| Clippy (denies unwrap/expect/panic outside tests) | `just lint` | yes |
| Doc, warnings denied | `just doc` | yes |
| Tests | `just test` | yes |
| File size — 500 warn, 700 fail; tests exempt | `just file-size` | yes |
| Gate self-test | `just gates-selftest` | yes |
| Generated types in sync | `just types-check` | yes |
| Third-party notice matches the bundle | `just third-party-check` | yes |
| Frontend typecheck and tests | `just ui-check`, `just ui-test` | yes |
| Coverage freshness | `just coverage-fresh` | yes |
| **Mutation debt** | `just mutation-debt` | yes, sub-second |
| Coverage measurement | `just coverage-check` | `check-all` |
| **Mutation run** | `just mutants` | `check-all` |
| Dependency audit | `just deny` | `check-all` |
| **Browser walk** | `just walk` | no |

Every row was checked against `justfile:16` rather than against memory. The
`types-check` row said *yes* for a while and was in neither `check` nor
`check-all` — a gate documented as running and not running, which is the exact
failure stipulation 7 is about, sitting in the table that describes the gates.

**Coverage** has a floor in `scripts/coverage-floor.txt` that `just
coverage-ratchet` raises toward 95% and never lowers. It is enforced by proxy in
the hot path: `coverage-check` writes a receipt, and the sub-second
`coverage-fresh` blocks once HEAD is more than ten Rust-touching commits past it.
So `check` says *coverage was measured recently and cleared the floor*, not
*coverage passes now*; the drift bound is what keeps that gap small.

**Mutation is a decision, not a step.** It is the strongest signal here —
coverage says a line ran, mutation asks whether any assertion would notice it
changing — and it costs about twenty minutes for a full pass. Run it when the
assurance is worth buying. What runs automatically is the debt gate, which
measures Rust lines changed since the last recorded run (working tree included,
untracked files included) and escalates: silent under 250, warns to 500, blocks
past that or past 20 Rust-touching commits.

A green `just check` therefore says: it compiles, it is linted, its tests pass,
coverage was recently measured, and mutation is not overdue. It does **not** say
the tests would notice the code changing. Only `just mutants` says that.

## Definition of done

1. `just check` exits 0 — the whole recipe, not the part you ran.
2. Tests you wrote ran and passed, and you saw them fail first.
3. No new code is unreachable from a test or a running path.
4. Every new dependency has a one-line rent comment in `Cargo.toml` saying who
   uses it and for what.
5. Anything you could not finish is stated plainly in your summary, not left
   implied by silence.
6. **Anything touching the graph or the simulator has been run in the browser.**
   Not reviewed — run. Three defects shipped past a green suite and were found
   in the first minute of using it: a duplicate list key that unmounted the
   whole canvas, byte-versus-UTF-16 drift in the source panel, and every
   invariant reporting "could not be checked" while the panel claimed to check
   them. `just run crates/inspect-model/tests/fixtures/specs/` is two seconds.

## Commit conventions

- Conventional Commits: `type(scope): description`
- Types: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`, `ci`, `perf`
- Scopes: `model`, `sim`, `journey`, `server`, `app`, `ui`
- Stage explicit paths. Never `git add -A` — it sweeps in whatever else is in
  the tree.
