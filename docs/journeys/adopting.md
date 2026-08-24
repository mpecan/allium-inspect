# Adding journeys to a repository that has none

Written for an agent, in the order the work actually goes. The reasoning is in
[`README.md`](README.md), the grammar is in [`reference.md`](reference.md), and
the pictures are in [`evidence.md`](evidence.md). This is the path through them.

The repository you are starting in probably has: an Allium spec set, tests, and
somewhere a document describing what people do with the product in prose. It
does not have `.journey` files. That is the ordinary starting point and none of
what follows assumes otherwise.

```sh
allium-journey --version        # if this is missing, stop and say so
allium check specs/             # the spec set should already be in good order
```

## 1. Read the spec before writing anything

A journey may only name constructs the spec declares — actors, triggers,
surfaces, entities. Writing one from memory produces a file where every step is
`unspecified` for the boring reason that you guessed the names wrong, and that
buries the `unspecified` steps that mean something.

```sh
allium-inspect --print-graph specs/ | jq -r '
  .nodes[] | select(.kind == "surface" or .kind == "actor" or .kind == "trigger")
  | "\(.kind)\t\(.module)/\(.name)"' | sort
```

What you are collecting: which **surfaces** exist, which **actor** each faces,
which **triggers** each surface `provides`, and what those triggers take. A step
is `<actor> does <Trigger>(args) on <Surface>` and all four have to be real.

## 2. Find the intents, do not invent them

Journeys are what people set out to do. That is somewhere in the repository
already — a product document, an onboarding flow, a support runbook, a list of
what the device tests walk. Take them from there.

Two tests for whether something is a journey:

- **It has an actor and an outcome.** "Ada borrows a copy and brings it back" is
  a journey. "Borrowing" is a feature.
- **It is worth reading aloud.** A journey is linear on purpose — no branches,
  no loops — so a branch is a second journey. If you cannot say it as a sentence
  ending in something that became true, it is not one yet.

Write the `goal:` line first, in the actor's terms, before any steps. If that
sentence is hard to write, the journey is not clear enough to be worth walking.

## 3. Write one, and let it be wrong

```sh
allium-journey check specs/ journeys/ --text
```

The first run will be mostly `unspecified`. Read every line and sort it into two
piles:

- **you guessed a name wrong** — fix it against step 1;
- **the spec does not have this yet** — leave it. That is the whole point. A
  step naming a surface the spec does not have is a requirement nobody has met,
  reported and not an error.

Do not weaken a step to make it hold. A journey bent into the shape the spec
already supports has stopped being a demand and become a description, and a
description of what already works is worth nothing.

**`refused` is different from `unspecified`.** Refused means the spec actively
forbids this, which is a disagreement to raise with whoever owns the spec, never
an assertion to delete.

## 4. Then walk it

```sh
allium-journey walk specs/ journeys/ --text
```

`undecided` means the simulator could not tell — usually a derived value, or a
`sees` whose surface filter is stored as text. Two honest ways forward:

- `stipulate <path> = <value>` says it outright. Every stipulation is printed
  above the walk, always, so a journey cannot pass invisibly on things it was
  told rather than shown;
- leave it undecided. It is not a failure and it does not need to be resolved.

## 5. Where to put them

```text
specs/     the spec set
journeys/  one file per area, several journeys in a file
```

Journey names are global across the set — the tool refuses two of one name — so
name them for what happens rather than for where they live:
`ACopyGoesOutAndComesBack`, not `Borrowing2`.

**Never renumber a step.** Numbers are citations: prose refers to them, and once
you reach step 6 below, code does too. Inserting a step means appending one, or
splitting the journey.

## 6. Only then, evidence

Everything above is worth doing on its own. Evidence is the second half, and
[`evidence.md`](evidence.md) is the contract; the order that works:

1. **Mark the tests that already cover a step** — one comment,
   `// journey: Name.3`. Start with the steps no browser could photograph,
   because that is the fastest honest coverage there is and it turns "nobody has
   shown this" into "this is covered, here".
2. **Find where the harness already takes pictures.** Most repositories with a
   device or browser tier already screenshot. The work is adding a step id and
   an append, not building a harness.
3. **One journey end to end before a second.**

```sh
allium-journey evidence check target/evidence/ --journeys journeys/ --code .
```

## What good looks like

A journey that a person who has never seen the spec can read, that names only
things the spec declares, whose remaining gaps are the backlog somebody meant to
write down, and whose steps are numbered the same today as when they were
written.

Not: every step holding. A journey where every step holds on the day it is
written was probably written from the spec rather than from an intent, and it
will never tell anybody anything they did not already know.
