# Evidence: pictures of a journey actually happening

A journey verdict answers *does the specification support this step*. That is a
different question from *does the software do it*, and the second is the one a
reader assumes they are being told unless the two are kept apart.

Evidence is the second answer. A test run photographs the product, each picture
says which step it shows, and the Journeys view puts them under the steps they
are of.

```sh
allium-journey evidence seal  target/evidence/ specs/journeys/
allium-journey evidence check target/evidence/ --journeys specs/journeys/ --code .
allium-inspect --journeys specs/journeys/ --evidence target/evidence/ --code . specs/
```

Everything below is the contract. A harness in any language can produce it: the
whole obligation is *append a line of JSON per picture*.

## The five standings

Three inputs — the journeys, the manifest a run wrote, the markers scanned out
of source — resolve to one standing per step.

| standing | means |
|---|---|
| **shown** | a picture, from a run that was still passing |
| **stopped here** | a picture from a run that broke at this step, kept and marked |
| **stale** | a picture taken before the step was reworded |
| **claimed** | a test says it demonstrates this and produced nothing |
| *(nothing)* | nobody says they demonstrate this |

**claimed** is the one that pays for scanning the code at all. Without markers,
a harness that quietly stopped photographing and a step nobody ever covered
leave exactly the same trace — none — and the first would be reported as the
second.

## 1. A marker in the code

```text
// journey: ACopyGoesOutAndComesBack.3
```

A comment, anywhere in a file, naming `<Journey>.<step number>`. Shaped after
the `/// allium: rule-success.MintIdentity` tags an implementation already
carries, so the two read as siblings.

Every comment syntax works — `//`, `///`, `#`, `--`, `/* */`, `*`, `;`, `%` —
and a note may follow the id:

```text
# journey: ACopyGoesOutAndComesBack.3 — her empty shelf
```

The run-up must be comment punctuation, so a string literal or a stretch of
prose containing the word files no claim nobody made.

**Put it on the test that genuinely covers the step**, including tests no
browser can photograph. A command-line act cannot be a screenshot; marking the
test that does cover it makes the step read as `claimed` rather than as never
covered, which is a true statement and a much more useful one.

## 2. A line per picture

The harness appends to `frames.jsonl` in the evidence directory — one JSON
object a line, written as it goes:

```json
{"step":"ACopyGoesOutAndComesBack.3","image":"03-shelf.png","caption":"the copy in her hands","passed":true,"taken_at":"2026-08-24T09:00:00Z","source":"e2e/borrow.ts:42","tags":{"theme":"dark"}}
```

| field | |
|---|---|
| `step` | `<Journey>.<number>` — required |
| `image` | the file, relative to the directory the log is in — required |
| `caption` | what the picture is of, in the harness's words, or `null` |
| `passed` | whether the run was still passing when this was taken |
| `taken_at` | when, as the harness spells it — never parsed, only shown |
| `source` | where in the harness it was taken, for going there |
| `tags` | what this picture is *of*, beyond which step — may be omitted |

**Append, do not write once at the end.** A walk killed half way through is the
interesting case: what it managed to photograph before it stopped is exactly the
evidence somebody wants.

**Empty the directory before the run, not from inside the harness.** A directory
holding half of one walk and half of the last reads as a walkthrough of a flow
that no longer works. Doing it from the harness breaks the moment the harness
runs twice — two browsers, two platforms — because the second wipe takes the
first run's pictures with it.

## 3. Seal it

```sh
allium-journey evidence seal target/evidence/ specs/journeys/ --walk borrowing
```

This turns the log into `manifest.json`: it resolves every step id against the
journeys and stamps each frame with **the step as it currently reads**.

Sealing happens here, once, rather than in each harness. Working out what a step
says is a question with one right answer, and a harness computing it in its own
language would be a second implementation free to disagree with the first.

It **refuses**, rather than dropping what does not fit:

- a frame naming a step no journey has — a rename half done, and dropping it
  would leave that step reading as never covered;
- a picture the manifest would promise and nobody can open;
- two journeys of one name, whose steps would silently merge.

`--at <ISO8601>` stamps a given time instead of now, for a reproducible build.

## 4. Look at it

```sh
allium-journey evidence check target/evidence/ --journeys specs/journeys/ --code .
```

```text
3 of 13 steps have been shown
  claimed    ACopyGoesOutAndComesBack.1
               claimed by crates/lending/tests/borrow.rs:88
  shown      ACopyGoesOutAndComesBack.3
               03-shelf.png — 2026-08-24T09:00:00Z  [theme=dark]
  stale      ACopyGoesOutAndComesBack.4
               04-returned.png — 2026-08-19T11:02:00Z
               it now says: 4. she brings it back the next morning
```

Exit 1 when something is wrong, 0 with `--report`. `--evidence` and `--code` are
separate flags on purpose: the case worth reporting is a test that claims a step
and produced nothing, and in exactly that case the run wrote no manifest line —
so the claim cannot be derived from the pictures.

In the browser it is the same three inputs, under the steps:

```sh
allium-inspect --journeys specs/journeys/ --evidence target/evidence/ --code . specs/
```

## Staleness

A sealed frame carries the step's text, not a hash of it. When they stop
matching, the panel shows *what the step said then* beside *what it says now* —
so a reader can see that one line was reworded and the other four were not.
"Digest mismatch" is a fact about the tool rather than about the step.

Comments and layout are normalised away before comparing: reindenting a journey
file is not a rewording, and neither is fixing a typo in a `--` note.

## Tags: more than one picture of a step

A step may be photographed several ways. `tags` say what each picture is of
beyond which step, and the panel grows **one dropdown per key**:

```json
{"step":"ACopyGoesOutAndComesBack.3","image":"03-dark.png","passed":true,
 "taken_at":"…","caption":null,"source":null,"tags":{"theme":"dark","platform":"ios"}}
```

**Named, not bare.** `{"theme": "dark"}` rather than `["dark"]`, and that is
what makes a dropdown possible: the name says which pictures are *alternatives*
to each other, so picking `dark` declines `light` and says nothing about
`platform`. A flat list of words would leave a reader to work out which of them
were answers to the same question.

Two rules follow:

- a picture that says nothing on an axis is shown whatever is picked on it —
  silence is not disagreement;
- put the tag values in the file name too, or two runs photographing one step
  write the same name twice and the second overwrites the first.

## Declaring the ways a journey should be shown

A journey can say which axes it expects, beside `cast:` and `given:`:

```
shows:
    theme: dark, light
```

That inverts where the control comes from. Without it the panel reads axes off
whatever a harness happened to emit, so the dropdown exists only once somebody
has produced a picture — and a typo (`them: dark`) quietly becomes a second axis
nobody meant.

With it:

- the control appears **before a single picture exists**, with the values
  nothing has answered marked *none yet*. A declaration is a demand written
  before the thing it asks for, the same as a step, so this is reported and is
  **not** a failure;
- a tag outside the declaration is reported, and *is*. Key and value are told
  apart, because an unknown key is usually a typo and an unknown value is
  usually somebody adding a way of showing the journey without saying so.

An axis needs at least two values — a control offering one option does nothing —
and may not list the same value twice.

**Declaring nothing constrains nothing.** A journey with no `shows:` block reads
its axes off the pictures exactly as before and reports no tag. Declaring one
axis is opting in to being told about the rest.

## For an agent adding this to a repository

1. **Write the journeys first**, against the spec, without thinking about
   pictures. `allium-journey check specs/ journeys/ --text` until the
   `unspecified` list is the backlog you meant rather than a list of typos.
   Nothing below is worth doing over journeys nobody has read.

2. **Mark the tests that already cover a step.** This costs one comment and
   immediately turns "nobody has shown this" into "this is covered, here". Start
   with the steps no browser could ever photograph: it is the fastest honest
   coverage in the whole chain.

3. **Find where the harness already takes pictures.** Most repositories with a
   device or browser tier already screenshot; the work is adding a step id and
   an append, not building a harness.

4. **One journey end to end before a second.** The value is in a reader opening
   a journey and seeing it happen, and one complete journey demonstrates that
   where five half-covered ones do not.

5. **Declare `shows:` only once there is a second way of looking**, and let the
   first walk be untagged. A dropdown with one option is worse than none.

### What not to do

- **Do not photograph a flaky moment.** A picture of one gets sealed, shown, and
  believed. If a step will not settle, leave it `claimed`.
- **Do not assert in the walk what the journey already asserts.** The verdicts
  are the assertion; a browser test re-litigating them is a second opinion with
  no standing. The walk's job is to establish the thing a walk cannot: that this
  happened, and here is what it looked like.
- **Do not let the pictures decide the journey.** A step exists because somebody
  set out to do something, not because a screenshot was easy to take there.
- **Do not tag what nobody will switch between.** A tag is for a reader choosing
  between two ways of seeing one step. `run_id`, `hostname` and `git_sha` are
  not that; they belong in `caption` or `source`.
