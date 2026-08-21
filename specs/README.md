# allium-inspect, in Allium

The tool, specified in the language the tool reads.

```sh
just run specs/                              # look at it
allium-journey walk specs/ specs/journeys/   # walk the journeys
```

Four modules, and the split is the crate split — so the Modules view of this set
is a picture of the architecture rather than a second thing to keep in step:

| module | crate | what it owns |
|---|---|---|
| `reading` | `inspect-model` | files → one linked graph, and what would not resolve |
| `simulating` | `inspect-sim` | a world, a step, and three answers |
| `journeys` | `inspect-journey` + `apps/journey` | written demands, walked |
| `browsing` | `inspect-server` + `ui` | canvas, panel, source strip |

The dependency shape falls out of that, and it is the one the code has:

```
browsing   → reading     5        reading is the foundation
simulating → reading     4        nothing depends on browsing
journeys   → reading     2
journeys   → simulating  1
```

## Why `allium-journey` is not its own spec set

It is a **surface** in `journeys.allium`, beside the browser's.

The two commands are different boundaries onto one domain, not two domains. A
separate set would mean two declarations of `Journey`, two of `Walk`, and a
standing job keeping them in step — the coupling a module boundary exists to
prevent rather than create. What actually differs between them is who is on the
other side and what they can do, which is what a surface is for.

## What the invariants are

Mostly the stipulations from `CLAUDE.md`, restated where a machine can check
them — and several written directly from bugs found in this repository:

- `EveryUndecidedRuleSaysWhy` — an undecided answer with no reason is
  indistinguishable from a bug.
- `UndecidedIsNeitherHeldNorRefused` — the two failure modes, named together
  because they are one mistake pointing opposite ways.
- `AJourneyNamingWhatIsMissingIsNeverSatisfied` — from the defect where a cast
  naming a type the spec never had reported "1 of 1 steps hold", exit 0.
- `OnlyAStipulationThatLandedIsListed` — from the defect where the ledger
  printed a change to the world that never happened.
- `ClausesAreQuotedNotRebuilt`, `TheStripCountsBytes` — stipulations 3 and 4.

## What it reports about itself

One finding, kept on purpose. `allium analyse` calls `FinishReading` and
`ReadingFailed` a conflict: both leave `reading` and write `status` to different
values. They cannot both fire — one requires `parsed = true`, the other
`parsed = false` — but the check compares the shared from-state and the written
value without reading the rest of the preconditions, so **any** two-outcome
branch trips it.

It is left as it is. The model is true, and a spec bent into shape to keep an
analyser quiet is a worse description of the thing it describes. Being able to
say why a finding is not a bug is the ordinary use of one.

One warning, also kept: `Person` is an external entity with no governing spec,
because there is no specification of people.

## What the journeys report

`specs/journeys/` holds three, and they do not all hold end to end. That is the
normal state of a journey and the reason to write them — but here the gaps are
worth reading, because most of them are the tool describing its own limits:

- **A positive `sees` is always undecided.** Whether a surface's filter admits
  *this* actor needs the `exposes` clause as an expression, and it is stored as
  text. `cannot see` against a surface that does not carry the field is decided.
- **Derived values are not computed.** `module_count` is `modules.count`, so a
  rule waiting on it stays undecided until a journey says otherwise —
  deliberately, and on the record, which is what `stipulate` is for.
- **A field typed by a named enum** comes back unknown when a rule creates it
  with a bare value; the simulator resolves bare names against a field's own
  declared states.

None of those are wrong answers. They are the tool declining to guess, which is
the whole design — and seeing them from this side is the most direct argument
for it that this repository contains.

## Written after the fact

Worth admitting. A journey is the demand written first; this is a description
written from a tool that already runs. Its value is different: it is the shape
the code turned out to have, in a form somebody can argue with — and writing it
found three modelling errors in an afternoon, each caught by the tool it
describes.
