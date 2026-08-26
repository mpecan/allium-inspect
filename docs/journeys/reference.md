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

## `world` — what a whole file starts from

Thirty-nine journeys in one set wrote 166 `given` lines between them, 58 of them
distinct — and 125 of the 166 were repeats of seventeen. A file can say those
once, above the journeys that take them:

```
world {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available
}
```

Every journey below it starts there. `cast:` and `given:` and nothing else — a
world says who is there and how things stand; a step is something somebody
*does*, and a world that could act would be a journey nobody named.

**A journey can decline it**, which the design needs: one about somebody who has
no identity yet cannot start from a world where she has one, and belongs in the
file with the others about her.

```
journey SheHasNoCopyYet {
    world: none

    cast:
        ada:  Member
        copy: catalogue/Copy

    1. she borrows a copy nobody has described
        ada does MemberBorrows(ada, copy) on MemberShelf
}
```

**A journey can override a line**, and is reported doing it. Eight journeys
wanting the same two-member membership and one wanting it `departed` is the
ordinary case; forbidding the override splits the file, and allowing it silently
is the thing that must not happen. The journey's own `cast` and `given` are laid
down after the world's, so a line about the same thing wins.

One world per file, declared before the journeys that take it, and no
inheritance across files. A world reaching in from somewhere else, or a world
built on another world, and *where did this come from* stops having a short
answer.

### What it costs, and why the report grows

This is the one feature that can break the rule everything else here rests on:

> Every stipulation is reported, first and always. An agent can make any journey
> pass; it cannot make one pass invisibly.

A step holding because of a line in another part of the file **is** passing
invisibly. So a journey that inherits a world reports every line it inherited,
where each was written, and which of them it went on to change — above its own
stipulations, because the world was there first:

```text
TheCopyIsAlreadyGone  —  1 of 1 steps hold
    from the file  ada: Member
    from the file  copy: catalogue/Copy
    from the file  overridden copy.status = available
   1. she cannot borrow it                             holds
```

A journey that inherits nothing reports nothing here — printing its own `given`
block back at it would say nothing at all.

## `after` — the journey this one continues from

Journeys do not happen in a vacuum. Somebody arrives, and *then* reads what was
sent to them, and *then* takes it back — three journeys about one afternoon, and
the second and third spend their `given` blocks rebuilding what the first
already established.

```
journey SheBorrowsACopy {
    cast:
        ada:  Member
        copy: catalogue/Copy
    given:
        copy.status = available

    1. she borrows it
        ada does MemberBorrows(ada, copy) on MemberShelf creating loan: Loan
}

journey AndBringsItBack {
    after: SheBorrowsACopy

    1. she brings it back, without saying again that she had it
        ada does MemberReturns(loan) on MemberShelf
        then loan.status = returned
}
```

The named journey is walked to its end and this one starts in the world it
left — its cast, its instances, everything a step of it caught. This journey's
own `cast` and `given` go on top. Its file's `world` is **not** laid out again:
the journey it follows already started from one, and a second over the top would
make a second `ada`.

Chains carry as far as they are written, and across files: a name is unique
across the whole set, and a life does not stop at a file boundary. Written down,
the chain reads as a life rather than as three unrelated mornings.

A journey that continues from one that does not exist is told so. So is a circle
of them — reported and not followed, because the journeys in it are real and
their steps are still worth walking; only the loop is refused.

`after:` is also a clause — `after 1.hour` — and the two are told apart the way
every block key is: a colon, and a place above the steps rather than under one.

### What makes it safe

Different in kind from what makes `world` safe. A world is a list of lines and
every one of them is reported; the end state of another journey **is a world**
and cannot be listed. What can be said, and is, is that the ground under this is
itself checked, and what its verdict was:

```text
And Brings It Back  —  1 of 1 steps hold
    after  SheBorrowsACopy  —  1 of 1 steps held
   1. she brings it back, without saying again that she had it   holds
```

And when it did not hold, the line says so and **the journey standing on it
cannot come out better than it**:

```text
What Stands On It  —  1 of 1 steps hold
    after  TheGroundGivesWay  —  0 of 1 steps held, so this begins somewhere the spec does not fully support
```

Every step above was answered in a world that journey built, so a green report
over a broken foundation is the same invisible pass everything else here
refuses — with more distance between the two halves of it than usual.

A chain also carries its **stipulations** forward, each marked with whose it
was. An agent that could make this journey pass by stipulating in the one before
it would have found the loophole that rule exists to close.

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

**An act runs everything it sets off.** A rule that emits a trigger wakes the
rules waiting on that trigger, and those wake the next — so `creating` can name
what a rule two hops away made, and `then <Rule> fires` can ask about it. The
Simulate view in the browser stops at the first rule and offers you the emission
to follow, because there a person is picking. A journey has already picked.

### `cannot do` — the spec refuses, and that is the point

```
ada cannot do MemberBorrows(ada, copy) on MemberShelf
```

The mirror of `cannot see`. A step where a block *works* is a step where the act
is refused — and written as `does` it reports `refused`, which is the correct
verdict about the specification and the wrong one about the journey. Correct,
and shaped exactly like a step that is simply wrong.

Satisfied when the spec refuses, and it says which precondition held the line.
**Refused when the act goes through**, which is the whole reason this is a
clause and not a comment. Undecided stays undecided: a refusal that came back
green because nothing could work out whether the act happens is the same failure
as a `cannot see` that passed unchecked.

It takes no `creating` — an act the spec refuses makes nothing to name.

`then <Rule> does not fire` is close and does not do this: it sits under a `does`
that has already been marked refused, so the step's own verdict still reads as a
failure.

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
ada sees loan.status on MyLoans in ada
```

A `sees` may name a **call** as well as a path, because a surface may expose
one — `exposes: announces_reads(owner)` shows a person whether they announce
reads, and there is no field to name for it. Write it with whoever is looking
in the argument: `ada sees announces_reads(ada) on PrivacyControls`.

Being able to *do* something and being able to *see* the result are different
claims, and a system that does the right thing and tells nobody is a real
failure. `cannot see` is how a privacy claim gets written down.

**The clause is walked, not merely matched.** The question is never "does this
surface expose labels" but "does it show me *this* label", so

```text
context borrower: Member
exposes:
    for loan in borrower.open_loans:
        loan.status
```

shows a reader the loans on their own shelf and answers `cannot see` for
everybody else's.

The walk is matched **whole**, not by its last field. A surface reaching
something through another noun —

```text
for m in group.messages:
    m.attachment.size_bytes
```

— shows an attachment's size, and a journey names the attachment it caught:
`ada sees shot.size_bytes on Conversation`. Those are the same walk written
from two places. `loan.shelfmark` is not, and does not match `loan.copy.
shelfmark` merely because both end in the same word.

**Say which one with `in`.** A surface scoped to a `Group` shows one group's
business, and a person is usually in several — so
`bruno sees proposal.decision on GroupMembers in room` says which room he has
open, and nothing else can.

Without it the context is bound from the actor, and only when the actor is an
instance of its type — which is right for `context identity: Identity` and
impossible for anything else. Walking from a person to a plausible group would
be this tool deciding which room somebody has open, which is a fact about their
afternoon and not about the specification. So the tool asks, and the reason on
an undecided `sees` names the surface and the type it wants.

`in` is for `sees` alone. An act names which one with its arguments.

Undecided is still the answer whenever anything under it is: a filtered
collection keeps its definite members and notes what it could not settle, and a
subject missing from a result like that might have belonged. Saying *this
surface does not show you that* on those grounds is the same failure as saying
yes, pointed the other way — and a privacy claim that passed because nothing
checked it would be the worst answer this tool could give.

### `stipulate` — say it rather than show it

```
stipulate ada.is_at_limit = false
stipulate may_invite(chat, ada) = true
```

Sets a value directly, mid-journey, the way `given` does at the start. Use it
when reaching a state honestly would take ten steps that are not what this
journey is about.

**A call, as well as a path.** A specification names functions it never
defines — a policy nobody has decided yet — and no simulator can work one out,
now or ever. Every rule guarded by one is permanently undecided until somebody
says, and this is how they say it. The answer is matched on the argument
*values*, so the rule's `may_invite(group, issuer)` and the journey's
`may_invite(chat, ada)` are the same call; and it is about the arguments it
names, so answering for Ada says nothing about Bob.

Both answers are sayable. `= false` refuses the rule, or this would be a way of
making journeys pass rather than a way of saying what nobody has decided.

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
