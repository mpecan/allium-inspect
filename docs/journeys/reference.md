# Writing a journey

The design and the reasoning behind it are in [`README.md`](README.md). This is
the reference: everything a `.journey` file can contain, with an example of each
that runs.

Every example here is checked against the fixture spec set in
`crates/inspect-model/tests/fixtures/specs/`, so you can copy any of them and
run it:

```sh
allium-journey walk crates/inspect-model/tests/fixtures/specs/ your.journey --text
```

## The shape of a file

A file holds one or more journeys. Nothing outside a `journey` block is read.

```
-- Comments start with two dashes and run to the end of the line.

journey ACopyGoesOut {
    goal: What this is for, in the actor's terms. Prose. Never load-bearing —
          nothing checks it, and it can run across as many lines as you like.

    cast:
        ada:  Member
        copy: catalogue/Copy

    given:
        ada.is_at_limit = false
        copy.status = available

    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf
        then copy.status = on_loan

    ends: What is true afterwards, in words. Prose, like `goal`.
}
```

Blocks may be left out. A journey with no `cast` and no `given` starts from a
world seeded with the spec's own configuration defaults and nothing else.

**Indentation is meaningful in exactly one way**: a line indented *deeper* than
the clause above it continues that clause. That is how a long act wraps.

```
    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf
            creating loan: Loan
```

## `cast` — who is in it

```
cast:
    ada:  Member
    bob:  Member
    copy: catalogue/Copy
```

`name: Type`. The type is written as the spec writes it, so a cross-module
reference keeps its module: `catalogue/Copy`.

Each line makes **an instance**, not a role. `ada` and `bob` above are two
different members — which is the point, because "two people of the same kind
with different preconditions" is the ordinary case rather than the interesting
one.

The names are what every clause below refers to.

## `shows` — how it should be demonstrated

Optional, and only worth writing when a journey is photographed more than one
way. Each line is a question a picture can answer and the answers worth having.

```
shows:
    theme: dark, light
```

The Journeys view grows one dropdown per key, offering it **before any picture
exists** and marking the values nothing has answered *none yet*: a declaration
is a demand written before the thing it asks for, the same as a step. A tag
outside the declaration is reported — which is the typo it exists to catch.

An axis needs at least two values (a control offering one does nothing) and may
not list the same value twice. Declaring nothing constrains nothing.

See [`evidence.md`](evidence.md) for the whole chain.

## `given` — what is already true

Two forms.

**An assignment** sets a field on something already cast:

```
given:
    ada.name = "Ada"
    ada.is_at_limit = false
    copy.status = available
```

**An instance** creates something and describes it in one line:

```
given:
    res: Reservation { member: ada, status: waiting }
```

Both are *told*, not shown — nothing ran to make them true. That is the point of
`given`: it is the starting position, and the journey is what happens next.

Values may be a number (`5`, `2_000_000`), a duration (`21.days`), a string
(`"Ada"`), `true`, `false`, `null`, a set (`{a, b}`), a name the journey cast, or
a bare word, which is read as a state the spec declares.

## Steps

A step is a line beginning `<number>.` followed by a sentence:

```
1. she borrows it
2. a fortnight passes
3. she brings it back
```

The number is yours — it is reported back, not checked. The sentence is what the
report shows beside the verdict, so write it for whoever reads the failure.

Under each step come its clauses, in the order they happen.

## Clauses

### `does` — somebody acts

```
ada does MemberBorrows(ada, copy) on MemberShelf
```

`<actor> does <Trigger>(<arguments>) on <Surface>`. Arguments are positional and
matched against the trigger's declared parameters, which is how the spec writes
the act and how you read it back.

To keep what the act created:

```
ada does MemberBorrows(ada, copy) on MemberShelf
    creating loan: Loan
```

`loan` is then a name like any cast member, usable by every clause after it —
including as the actor of a later step.

### `after` — time passes

```
after 14.days
after 1.hour
```

The clock is a field you advance, never a reading of the system clock, so a run
reproduces exactly. Advancing it also runs whatever the new time made true: a
state-condition rule is not fired by anybody, and a journey that says a fortnight
passed has already said that whatever became true in it happened.

### `then` — assert something

```
then loan.status = open
then copy.status != available
then ada.open_loan_count <= 5
then BorrowCopy fires
then ReturnCopy does not fire
then loan exists
then reservation does not exist
then loan in Loans
```

| Form | Means |
|---|---|
| `<path> = <value>` | and `!=`, `<`, `<=`, `>`, `>=` |
| `<Rule> fires` | the rule ran during the step above |
| `<Rule> does not fire` | it did not |
| `<name> exists` | the name resolves to something in the world |
| `<name> does not exist` | it does not |
| `<term> in <collection>` | `Loans` is every loan; `Copies` every copy |

`==` is not this grammar and is refused rather than read as `=`.

Two things to expect. A **derived** field — `open_loan_count` is
`open_loans.count` — comes back `undecided` rather than computed, because the
simulator cannot settle; `stipulate` is how a journey gets
past that deliberately. And an assertion about a path nothing has set is
`undecided` **both ways round**: `exists` does not refuse it and `does not
exist` does not hold, because neither is something the world said.

### `sees` — what a boundary shows

```
ada sees loan.status on MemberShelf
ada cannot see copy.shelfmark on MemberShelf
```

Being able to *do* something and being able to *see* the result are different
claims, and a system that does the right thing and tells nobody is a real
failure. `cannot see` is how a privacy claim gets written down.

**Half of this is built, and the half that is missing is why a positive `sees`
never holds yet.** Whether the surface carries the field at all is read, and it
settles a `cannot see` outright — the second line above holds because
`MemberShelf` exposes nothing like `copy.shelfmark`. Whether the surface's
filter admits *this particular actor* needs the `exposes` clause as an
expression, and it is stored as text today. So once the field *is* exposed, both
directions come back `undecided` with a reason saying so. That is the right way
round: a privacy claim that passed because nothing checked it would be the worst
answer this tool could give.

### `stipulate` — say it rather than show it

```
stipulate ada.is_at_limit = false
```

Sets a value directly, mid-journey, the way `given` does at the start. Use it
when reaching a state honestly would take ten steps that are not what this
journey is about.

**Every stipulation is reported, first and always.** An agent can make any
journey pass; it cannot make one pass invisibly.

## What comes back

Six verdicts, and telling them apart is the whole point.

| Verdict | Means | In the report |
|---|---|---|
| **holds** | the spec does this | ✓ |
| **refused** | the spec does something else | ✗ |
| **unspecified** | the spec has not got this yet | + |
| **unexposed** | it happens, and nobody is shown it | ⊘ |
| **undecided** | this tool could not tell | ◈ |
| **remark** | worth a look | · |

The three that are not "holds" are three different pieces of work. "The spec
forbids this", "the spec has never heard of this" and "this tool could not tell"
send you to three different places, and a report that called all of them
*failed* would send you to change a specification that is not wrong.

**undecided is not a polite no.** An expression this simulator cannot evaluate —
a field nobody stated, a name nobody bound — comes back undecided
with the sub-expression that could not be settled, never as false.

## Running them

```sh
allium-journey walk specs/ journeys/          # JSON, one document per file
allium-journey walk specs/ journeys/ --text   # for a person
allium-journey check specs/ journeys/         # the static half, no world
allium-inspect --journeys journeys/ specs/    # in the browser
```

Exit `0` nothing to say, `1` something reported, `2` nothing to read. `--report`
exits 0 anyway, which is the mode a journey is *written* in: the steps the spec
cannot support are the backlog, not an error.
