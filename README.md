# allium-inspect

Read an [Allium](https://github.com/juxt/allium) specification in a browser, and
run its rules.

```sh
allium-inspect specs/
```

Runs the `allium` CLI over a spec set, binds a free port, and opens a browser
onto four views of it plus a simulator. Nothing is uploaded and nothing is
persisted; it is one binary and the specs you point it at.

## Why

The `allium` CLI emits well-structured JSON, and every command answers about one
file. Real spec sets are modular and cross-referential — `friend-mesh` is five
files and 6,700 lines with references like `membership/Group` crossing between
them — and no existing tool draws that shape. The official VS Code extension has
a diagram preview and a rule simulator; both are thin, and both stop at one file.

Two things follow from reading the set as a set:

- **Cross-module references become edges.** Over friend-mesh: 354 constructs,
  526 edges, 55 of them crossing a module boundary. `allium model` reports the
  target of one of those relationships as the literal string `unknown`, because
  from inside one file it *is*.
- **The simulator can follow a chain out of the module it started in.** A rule
  emitting `MessageSent` in `messaging` is consumed by `QueueOnSend` in
  `delivery`, and walking that is the closest an Allium spec gets to describing a
  user journey — the language has no journey construct.

## The views

| View | Answers |
|---|---|
| **Domain** | what the spec holds — entities, fields, relationships, enums |
| **Flow** | what happens and in what order — trigger → rule → entity → trigger |
| **Lifecycle** | how each entity changes state — one state machine per entity |
| **Journey** | what follows from an action, traced forward or backward |
| **Simulate** | fire a trigger against a world and watch |

`/` opens search, which matches a construct's name, its kind or the module it
is in — and opening a result clears whatever was in the way, turning its module
back on and switching to a view that draws it.

Tracing dims what the chain did not reach, which answers "which of these three
hundred?" by leaving all three hundred on the canvas. **Double-clicking a
construct** opens it on its own instead: the construct, everything joined to it,
and three ways of asking — adjacent, what follows, what leads to it — each laid
out and framed on its own. Double-clicking inside walks to the next construct,
so a chain can be followed a step at a time. The canvas behind it never moves.

The pop-up asks about the construct rather than about the current view, so it
shows the rule that creates an entity and the invariants that constrain it even
from the domain view, which draws neither. A form with no answer is switched off
and says why rather than opening empty.

Edges follow the route the layout engine chose for them rather than a curve
between two handles: ELK reserves channels between the layers as it places the
nodes, and drawing along those is the difference between a diagram and a bowl of
spaghetti on any view with more than a dozen constructs.

Selecting anything shows its source, anchored to the byte span the parser
reported. The spec text gets permanent space along the bottom rather than a
tooltip: in a spec explorer the text is the artifact and the graph is one way of
looking at it.

## The simulator

Build a world, fire a trigger, and the simulator evaluates the rule's
preconditions against it, applies its postconditions, checks every invariant, and
reports which state-condition rules the change just made possible.

**It never guesses.** An Allium expression can be true, false, or something a
simulator has no way to decide — a derived value the spec computes and this does
not, a name nobody bound, a comparison between kinds that do not compare.
Choosing a default for that third case is the failure worth avoiding: treat it as
true and rules fire that should not have, treat it as false and rules are
reported blocked by a precondition nothing checked. Either way the simulator
states a conclusion it did not reach.

So `unknown` is a real verdict, it propagates by Kleene's rules, and it carries
the sub-expression and source span that could not be settled. It is also the only
verdict on the panel that gets a fill, because it is the only one that needs a
person.

A state change is checked against the entity's declared lifecycle. `transitions
status { visible -> tombstoned }` is a claim about what may happen, so an
assignment the graph does not permit is refused and reported rather than written.

The clock is a field you advance rather than a reading of the system clock, so a
rule waiting on a due date is something you step *to*, and a run reproduces
exactly.

## Requirements

The `allium` CLI on `PATH` — this drives the real parser rather than
reimplementing it. Get it from [juxt/allium-tools](https://github.com/juxt/allium-tools).

## Options

```
allium-inspect [PATHS]...

  --port <PORT>      bind this port instead of a free one
  --no-open          do not open a browser
  --no-watch         do not reload when a spec changes
  --print-graph      print the whole graph as JSON and exit
  --allium <PATH>    the allium binary to run
```

`--print-graph` makes the whole pipeline scriptable:

```sh
allium-inspect --print-graph specs/ | jq '[.edges[] | select((.from|split("::")[0]) != (.to|split("::")[0]))]'
```

## Building

```sh
just build     # the frontend, then the workspace
just run specs/
just check     # every fast gate
```

`just --list` for the rest.

## How it is put together

```
crates/inspect-model   ingest the CLI's JSON → one linked SpecGraph      [pure]
crates/inspect-sim     three-valued evaluator, world, step engine        [pure]
crates/inspect-server  axum routes; the built UI embedded in the binary
apps/inspect           argument parsing, a free port, a browser, a watcher
ui                     Svelte 5, with wire types generated from the Rust
```

The two pure crates hold all the logic and touch no clock, socket or random
number generator. The `allium` CLI is reached only through a trait, so ingestion
and simulation are tested against recorded real output with no binary installed.

Four commands are ingested per spec file, not one: `model` describes entities but
carries no spans, `parse` is the only source of rules, surfaces and positions,
`plan` supplies the trigger → rule → entity chain, and `analyse` the findings.
Linking then runs once over the whole set.

The expression trees never cross the wire. They are an order of magnitude larger
than the graph and only the simulator reads them, so what travels is a world in
and a step out — which is also what makes the server stateless and a whole run a
value the browser can step back through.

## Who this is for

Five people who would open it are written down in
[`docs/personas/`](docs/personas/) — a spec author, a domain lead who does not
write Allium, an implementer, a reviewer, and someone who joined on Monday. They
exist so that a review of the interface has something to be a review *of*:
"the rail is cluttered" and "the rail is thorough" are the same observation from
two people with different jobs, and only naming the job settles it.

Reviews run against them live in [`docs/reviews/`](docs/reviews/).

## Quality gates

`just check` runs the fast ones in about a minute. Every gate is driven in both
directions by `just gates-selftest`, which runs inside `check`: a gate that
cannot fail is worse than no gate.

Coverage has a floor that ratchets upward toward 95% and never down, enforced in
the hot path by a receipt-freshness check rather than by measuring on every
commit.

Mutation testing is a decision rather than a step — it costs minutes and a gate
people skip is a gate that cannot fail. What runs automatically is
`just mutation-debt`, which measures how much Rust has changed since the last
recorded run and escalates: silent under 250 lines, a warning to 500, and a block
past that or past 20 Rust-touching commits. So the cadence follows code volume,
and the deferral is bounded.

## Licence

MIT.
