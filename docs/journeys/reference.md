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
world seeded with the spec's own configuration defaults and nothing else. A
journey can change a default — see [configuration](#configuration--running-with-a-switch-turned-on).

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

**A type may be an actor.** `ines: Reader` makes somebody who *is* a `Reader`,
by the journey's say-so, and every surface facing a `Reader` faces her — the
actor's `identified_by` condition is not asked of her, because the cast line is
where the claim is made and where a reader looks for it. Cast her as the entity
instead — `ines: Person` with `ines.role = reader` in `given` — and the
condition is asked, which is what to do when whether she is one is the point.
See [the surface has to face whoever acts](#does--somebody-acts).

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

Three forms.

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

**An answer** says what a call the specification never defines comes back as,
from the start — the same answer a [`stipulate`](#stipulate--say-it-rather-than-show-it)
gives mid-journey, matched the same way:

```
given:
    may_reserve(ada, book) = true
```

All three are *told*, not shown — nothing ran to make them true. That is the point of
`given`: it is the starting position, and the journey is what happens next.

An answer is listed with the journey's stipulations, because a field set in
`given` is a position the spec describes and a black box answered is a policy
it does not: `stipulated  may_reserve(ada, book) = true`. `stipulate` may
lead a `given` line, as it leads one in a step, and changes nothing about what
the line says. Answers are put in force after every field in the block is
written, so `allowed(note.author)` reads the author whichever line set it. An
argument that still has no value — `allowed(ada.team)` with no `team` set — or
an answer that has none — `allowed(ada) = ada.team` — is **undecided**, says
which term, and is not listed, in `given` or in a file's `world`: no call could
ever be answered by it, and a ledger line for it would be a receipt for nothing.
A later answer about the same call with the same arguments replaces an earlier
one, so a `stipulate` in a step changes what `given` said rather than standing
beside it, and of two such lines in one `given` only the later is listed.

The line is listed as written — `may_reserve(ada, book) = true`, not the ids
those names resolved to — the way a `world` lists its lines. A `stipulate` in a
step lists the value it resolved to, because there the clock and the world have
moved since the journey began.

Values may be a number (`5`, `2_000_000`), a duration (`21.days`), a string
(`"Ada"`), `true`, `false`, `null`, a set (`{a, b}`), a name the journey cast, or
a bare word, which is read as a state the spec declares — and is checked
against the spec before anything runs, see [a word the spec does not
declare](#a-word-the-spec-does-not-declare).

A set is written on one line, and may be empty:

```
given:
    repo.tags = {"a", "b"}
    repo.owners = {ada, bob}
    repo.labels = {}
```

What tells an assignment from an instance is an `=` before the first brace, not the brace:
`note: Message { body: "a = b" }` is still an instance.

### A word the spec does not declare

Nothing in the grammar tells a state apart from somebody the journey cast:
`copy.status = available` and `loan.member = ada` are the same shape. The walk
decides — a name the journey bound is what it is bound to, and any other word
is the state it spells. So before anything runs, every such word is checked
against the states the spec declares, the way the simulator checks a word one
of the spec's own rules hands on:

- **Written into a field the checker can name** — in `given`, in an inline
  instance, in `stipulate <path> = …`, or on the right of a `then` comparison —
  the word must be one of *that field's* states: its inline `open | returned`,
  or the values of the enumeration it is typed by. The field is followed hop
  by hop from the type the journey cast the name as, as long as each hop is an
  entity or value type the spec declares. The enumeration's name is read in
  the module that declares the field: two modules may each have an `enum
  Status`, and an issue's `status` takes its own module's values, not the first
  `Status` in the set. Where a name could mean either and nothing says which,
  the field's states count as unknown, and the next rule applies.
- **Anywhere else** — a field typed by an entity, a `String`, a collection, a
  path the checker cannot follow — any state the spec declares anywhere will
  do, because nothing says which one is meant.

A word that fails is **unspecified**, names the field and the states it does
declare, and suggests the nearest one when it is a slip away:

```text
`availble` is no state of `Copy.status`, which declares available, on_loan, lost — did you mean `available`?
`bob` is nobody in this journey and no state the spec declares
```

On a `then` or `stipulate` line nothing is compared or written, as with any
unspecified line. In `given`, the field is **left unset** rather than holding
the word, so a rule that reads it is undecided and names the field, instead of
refused on the strength of a state the spec has never heard of. A set with one
such word in it, `hold.reviewers = {ada, bbo}`, is left unset whole: each bad
member is reported, and laying out the rest would let `then ada in
hold.reviewers` hold beside a line saying the set is wrong. Wherever the walk
leaves a `given` unwritten the line carries a note saying why, so nothing is
dropped without a word. The names a
`given` block makes are known on every line of it, in any order: `copy: spare`
may come before `spare: catalogue/Copy { … }`. A word that is
a state — of another field — is still reported where the field is known:
`then loan.status = available` can never hold when `available` is a copy's.
A path through another entity is followed the same way, each hop in the module
of the entity declaring the field: `bo.item.stage` on a `queue/Issue` with
`item: Item` is the queue's `Item`, even when another module declares one too.

The file's `world` is laid out before the journey's own cast, so a world line
can name only the world's own cast and instances, and the journeys this one
continues from. A world line naming somebody only the journey casts is
**unspecified** — `` `ada` is cast by this journey, and the file's world is
laid out before the journey's cast `` — and left unset.

**An act's arguments** have no field to check against: a trigger's parameters
carry no types. A word nobody cast and nothing declares there is reported as
**unspecified** when a rule on that trigger compares the parameter against
states (it can never match), and as a **remark** otherwise — worth a look, as
it is usually somebody missing from the cast, and walked, since no rule has to
match it. A remark is an *aside*: shown under the step by `check`, by the
walk and in the browser alike, and no part of the step's verdict, which reads
`holds` when everything the step asserts held.

Left alone: a stipulated *call*'s arguments and answer — `stipulate can(r,
code) = true` matches the words the spec writes in that call, which no
enumeration need declare — anything under `config`, which the walk checks
against the parameter's declared type, and a capitalised word, which is a
collection (`Loans` is every loan — see [`then`](#then--assert-something)).

### Configuration — running with a switch turned on

A walk starts from the spec's `config` defaults. Much of a real system is off
until somebody turns it on, so a journey can set a parameter wherever it can set
a field — in `given`, in a file's `world`, and in a `stipulate`:

```
given:
    config.loan_limit = 1
    lending/config.loan_period = 7.days
```

`config.<name>` is the parameter whichever module declares it. When two modules
each declare one by that name, the bare form is refused rather than guessed
between, and the module goes in front the way it does on a type:
`lending/config.loan_limit`. A parameter with no default can be set the same
way, and until it is, a rule that reads it is undecided and names it.

**It is reported as a stipulation**, unlike a field set in `given`. A field is
the starting position; a parameter is a world the spec's own defaults say is not
the one it describes, and a step holding on the strength of that is a step
holding invisibly unless the report says so:

```text
Config In Given  —  1 of 1 steps hold
    stipulated  config.lfs_enabled = true
   1. use LFS on an instance that enabled it           holds
```

A line in the file's `world` is listed once, as `from the file` — under a
journey that continues from another as well, since the line is still the
file's. One a journey this one continues from set in its own `given` is listed
`through` it. The line is listed as the author wrote it — `config.lfs_enabled =
r.lfs_used`, not the `unknown` that evaluated to. A world line and a journey
line about the same parameter are an override however each spells it:
`switches/config.lfs_enabled` overrides `config.lfs_enabled`.

What is written is checked against what the spec declares, and nothing is
written when it does not fit:

- a name no `config` block declares is **unspecified** — the spec has not got
  that switch — and the default stays in force;
- a value that is not of the parameter's declared type (`config.loan_limit =
  true`) is **undecided**, naming the type. `Boolean`, `Integer`, `Decimal`,
  `String`, `Duration` and `Timestamp` are checked; any other type is taken as
  written.

`then config.loan_limit = 1` reads a parameter back, resolved the way a write
is: a name no block declares reads **unspecified**, and a bare name two modules
declare reads **undecided**, each with the reason a write of it would give.

Parameter names are checked by `walk`, which has the spec's `config` blocks to
check them against, and not by `check`: a `given` naming a switch the spec does
not have passes the static half and is reported when the journey is walked.

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

Every journey below it starts there — answers to black boxes included, which is
usually the reason to write one: a permission predicate asked by every rule it
guards is said once, here, rather than stipulated in the first step of every
journey that reaches one. `cast:` and `given:` and nothing else — a
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
stipulations, because the world was there first. An answer the world gives is
listed here, `from the file  can(r, code) = true`, and a journey writing an
answer about the same call with the same arguments overrides it. *The same
arguments* means the same values once the world is laid out, not the same
words: a world answering `can(r, r.level)` with `level` at 1 and a journey that
sets `level` to 2 and answers `can(r, r.level)` again have answered two
different calls, and the world's line is not marked:

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
own `cast` and `given` go on top. Its file's `world` is **not** laid out again
when a journey earlier in the chain already started from it: a second over the
top would make a second `ada`, and put back every field and switch the earlier
journey changed. Its lines are still listed `from the file`.

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

`<actor> does <Trigger>(<arguments>) on <Surface>`. Arguments are positional,
which is how the spec writes the act and how you read it back. Each rule the act
wakes takes them under the names its own `when` clause gives them, in order —
see [how a trigger's arguments reach a rule](#how-a-triggers-arguments-reach-a-rule).

To keep what the act created:

```
ada does MemberBorrows(ada, copy) on MemberShelf
    creating loan: Loan
```

`loan` is then a name like any cast member, usable by every clause after it —
including as the actor of a later step.

What a call the spec never defines answers for the instance being made is said
under the same act, with [`where`](#stipulate--say-it-rather-than-show-it):
`creating issue: Issue` then `where audience(issue) = {}`.

**An act runs everything it sets off.** A rule that emits a trigger wakes the
rules waiting on that trigger, and those wake the next — so `creating` can name
what a rule two hops away made, and `then <Rule> fires` can ask about it. The
Simulate view in the browser stops at the first rule and offers you the emission
to follow, because there a person is picking. A journey has already picked.

That includes a trigger handed to another module. `ensures:
admin/FailureReported(kind: "migration", description: "boom")` is the trigger
of the module this one imported `as admin` — here `failures`, from `use
"./failures.allium" as admin` — and wakes the rules that would hear that module
emit it: its own `when: FailureReported(kind, description)`, and any module
that `use`s it. A journey names what that rule made by the module, not by the
alias: `creating n: failures/Notice`. An alias leading to no module in the set
wakes nobody, and the step is **undecided** saying which mistake it is —
`` `migrations` imports `admin` from "./failures.allium", which is not in this
spec set `` when that file was not walked, or `` `migrations` imports nothing as
`ghost` `` when the alias was never imported — rather than waking every rule of
that name on a guess.

**The surface has to face whoever acts.** A surface's `facing` names who is on
the other side of it, and an actor's `identified_by` says who that is:

```text
actor SiteAdmin {
    identified_by: User where is_admin = true
}

surface AdminPanel {
    facing admin: SiteAdmin
    provides:
        Flip(admin)
}
```

`bob does Flip(bob) on AdminPanel` asks the actor's condition of Bob before
anything fires, the same three ways a `requires` is asked:

- **It holds**, and the act goes ahead to the rules behind it.
- **It is definitely false**, and the act is refused without firing anything,
  so a rule that restates nothing of the surface's actor is still closed to
  somebody the surface does not face. The world is the one he walked up to, and
  a `then Flip fires` after it is told the surface is why.
- **Nothing settles it** — `is_admin` never set — and the rules behind the act
  are still asked, because an act goes through only if the surface faces him
  *and* its rule lets him. A rule that refuses him anyway decides the line:
  `cannot do` holds and `does` is refused, each saying both halves. Otherwise
  the act is undecided, with the condition as written and what was missing from
  it; the world is put back, and every rule it would have woken is undecided
  too, so a `then Flip does not fire` after it is not answered by a rule nobody
  ran. It is never let through and never blocked on a condition nothing
  checked.

```text
`AdminPanel` faces `SiteAdmin`, and `bob` is not one: not `is_admin = true`
whether `bob` is the `SiteAdmin` `AdminPanel` faces could not be settled — `is_admin = true`: `bob` has no `is_admin` set
```

An actor declared in another module (`facing admin: admins/SiteAdmin`) is read
the same way.

Four cases ask nothing:

- An actor that is only a type — `identified_by: User` — demands nothing
  beyond the type.
- Nor does a surface facing an entity directly (`facing c: Clock`).
- A type that differs from whoever acts is not adjudicated when the actor has
  no condition: a surface facing a `Member` and an act by an `Identity` is
  either a small inconsistency or one person being both, and a walk that
  picked would be guessing. So it goes ahead, as it always has.
- Somebody [cast as the actor itself](#cast--who-is-in-it) — `boss: SiteAdmin`
  — is one, because the journey said so. Not when an entity has the actor's
  name: an instance of that entity is somebody else, and is asked the
  condition like anybody.

An actor identified `within` a context, at a surface that is at two contexts of
that type, is undecided: which one the actor is identified within is not said.

An actor *with* a condition asked of somebody of another type — `crew: Team`
acting on `AdminPanel` — is undecided, naming both types: the condition is a
demand on a `User`'s fields, and nothing says whether a `Team` meets it.

An actor identified **`within`** a context —
`identified_by: Seat where workspace = within` beside `within: Workspace` —
reads `within` as the instance of the surface's context it is at. An act says
which one through the surface's own `provides`: `provides: Archive(workspace)`
passes the context as the first argument, so `seat does Archive(desk) on
WorkspacePanel` is at `desk`. A surface at no context of that type has no way
to say, and an act or look on it is undecided, both ways, saying so — casting
the person as the actor is then the one thing that settles it. Two contexts of
the type on one surface, and which one is meant is not picked for you: it is
undecided, naming the ambiguity.

A specification that restates every permission as a rule's `requires` as well is
still a good one — a rule should not trust its caller — but it no longer has to,
for a journey to notice that a surface is closed to somebody.

**A fan-out runs once per member.** A rule that wraps some of its clauses in
`for w in Hooks where active = true:` applies them once for each member the
filter keeps, with `w` bound to that member, and the clauses after the loop
still run once. The filter reads the member's fields bare (`active`) or through
the loop's name (`w.active`). A collection with no members — no `Hook` in the
world — is iterated zero times: nothing is emitted, nothing chained from it
runs, and that is decided. It is the same whether the clock or an act set the
rule off. Members are the instances the journey cast or the walk created; a
journey that wants none simply casts none.

A member the filter cannot decide about — a hook whose `active` nobody stated
— is not applied to and not quietly left out either. The filter reads the
member's own `active` even when the rule's trigger has an argument of the same
name: a hook that has none is undecided, not answered by the argument. The rule
still fired, and `then Fan fires` holds; but the world may be missing what it
would have done for that member, so the act or the `after` that set it off is
undecided, saying what was not applied:

```text
c does Poke(c) on Console
  `Fan` fired, but part of what it ensures was not applied: `for w in Hooks` could not tell whether its filter keeps Hook#1, so the clauses inside it were not applied to it — in `active = true`
```

The same goes for a collection nobody settled, `for h in b.hooks` with `b.hooks`
never stated.

**A filter reads what each member computes, too.** A projection may filter on a
field the member derives rather than stores — `active_watches: watches where
is_watching = true` on a repository, with `is_watching: a or b` on the watch.
Each watch is asked for its own `is_watching`, computed in the watch's scope:
the watch is `this` there and only its fields are bare, so a field of the
repository spelled the same way, computed or stored, does not answer for it. A
watch with `a` true is kept — whether or not anyone stated `b`, because `true
or` anything is true — and a watch with both false is left out, and a `for w
in r.active_watches` over them is decided. A watch whose `a` is false and whose
`b` nobody stated cannot be decided, and the loop says so with that watch's
reason inside — by the name the journey cast it as:

```text
r does Poke(r) on S
  `Poke` fired, but part of what it ensures was not applied: `for w in r.active_watches` may be missing members it could not decide on (`w` has no `b` set), so the clauses inside it were not applied to them — in `r.active_watches`
```

Only the first reason is shown in the parenthesis, and it is always a member
that could not be decided — a watch decided by half of its `or` leaves nothing
to say. It is usually the one to fix, a field the journey did not set on a
member it cast.

**A `requires` inside the loop is the rule's, for every member the filter
keeps.** The reading this tool takes: a `requires` is a precondition of the
rule wherever it is written, and inside a `for` it is quantified over the loop.
One kept member failing it refuses the whole rule, and nothing it ensures is
applied — not to that member, not to the others, not the clauses after the
loop. It is not a guard that skips the member: the loop's `where` and an `if`
inside it are how a spec says that, and reading `requires` as a third way would
make it mean something different inside a loop from everywhere else. The
language does not settle this, so the refusal says which reading was taken.

```allium
rule Muster {
    when: Muster(c)
    for h in Hooks where h.armed = true:
        requires: h.active = true
        ensures: h.swept = true
    ensures: c.done = true
}
```

It is decided the way `all` is over the same members, which are the ones the
loop's `ensures` are applied to:

- no hook, or no armed one: it holds — there is nothing for it to be false
  about — and `Muster` fires;
- every armed hook active: it holds, and an idle hook the filter leaves out is
  not asked;
- one armed hook inactive: `Muster` refuses, quoting `h.active = true` and
  naming the loop and the hook — `` `for h in Hooks`: Hook#2 does not satisfy
  this precondition, and a `requires` inside a loop is read for every member
  the loop keeps, so the whole rule is refused `` — whatever else is
  undecided;
- an armed hook nobody said was active: undecided, naming the loop and the
  member — `` `for h in Hooks` could not decide this precondition for Hook#1 ``;
- a hook whose `armed` nobody stated counts only if it fails: active, it passes
  whether the filter keeps it or not;
- a collection that may be missing members — `for h in r.hooks` over
  `hooks: Hook with rack = this`, with a hook whose `rack` nobody set — is
  undecided when every hook it holds passes, because the one it may be missing
  was never asked, and it says so, with the first reason it could not decide:
  `` `for h in r.hooks` may be missing members it could not decide on
  (`h2` has no `rack` set) ``, naming the hook as the journey cast it. A hook it
  does hold failing still refuses.

A `let` written inside the loop above the clause is bound per member, so
`let live = h.active` then `requires: live = true` reads each hook's own.
The clause is shown as the spec wrote it, sliced from its own span.

A loop that takes its members apart, `for (h, ok) in Hooks`, is not read at
all: the simulator binds a member to one name, and leaving `ok` unbound would
let a trigger argument or a rule-level `let ok` answer for it. A `requires`
inside one is undecided and nothing it ensures is applied, each quoting the
binding as written — `` `for (h, ok) in Hooks` takes each member apart into
names, which this simulator does not do ``.

**A rule that fired and left part of what it ensures unapplied leaves the step
undecided**, `for` or not. Six things count, and only these: a member of a
`for` the filter could not decide about, a `for` over something that is not a
collection, a `for` over a collection that may be missing members it could not
decide on, an `if` whose condition could not be decided, an assignment to
something that is not an instance, and a postcondition of a form the simulator
does not model. A postcondition that was decided and applied is not one of
them, even when deciding it left notes about what it did not need — an `any`
settled by one member, an `or` settled by its left side. An assignment whose
value could not be decided *was* applied: the field now holds an undecided
value, and the `then` lines that read it say so.

While the step is unsettled this way, `then <Rule> does not fire` about a rule
that did not run is undecided too, because whatever was not applied is what
might have woken it — and so is one that was woken and refused, because the
firing that was not applied might have woken it again with something it takes.
A `then` about a field in the same step is undecided either way, with what it
read and why the world is not settled: `then b.fired = false` holding when the
member that would have set it was never visited would be the walk guessing.

**A creation is followed the same way.** A rule written `when: o: Order.created`
runs once for each order the act's rules create, about that order, and whatever
it changes is there for the `then` lines after it. It is not a condition over
the world: an order that was already in the cast, or made by an earlier step, was
not just created, so nothing reacts to it again — and a firing about an order for
some other reason does not wake it either. In the browser the reaction is listed
under *Now possible*, labelled *created*, as reacting to the instances that were.

Because it is not a condition, `then Welcome fires` about it is decided either
way. If the act made no order, it did not run. If its `requires` refused, the
line quotes the clause that did.

A reaction can itself create, including another of what it reacts to. Each new
instance is a new event, so no memory of what already ran will stop a rule that
makes a cell every time a cell is made. After 32 rounds the act is reported
*undecided*, naming what was still being set off, rather than being judged on a
world cut off partway through. A reaction that makes *two* of what it reacts to
doubles every round and would not reach that bound for hours, so one step also
stops after 512 firings and says so the same way.

Two modules may each declare an `Order`. A reaction written `o: Order.created`
reacts to its own module's orders, and `memo/Order.created` to those of the
module imported `as memo`. Another module's `Order` being made does not wake it.

A creation written through another module's name — `ensures: memo/Note.created(…)`
— is a creation like any other, placed in the module the `use … as memo` names.
An unqualified `Order.created(…)` goes in the creating module when that module
declares an `Order`. `creating` catches by the entity's name, so
`creating n: notes/Note` and `creating n: Note` name the same thing, and a
reaction written `when: n: memo/Note.created` reacts to it.

`creating` catches the first instance of that kind made since the act began,
in the order they were made. That is the act's own creation when it made one,
even if a reaction to it made another of the same kind afterwards.

### How a trigger's arguments reach a rule

A trigger is declared by nothing but the rules that listen for it: `when:
Published(doc)` *is* the declaration, for that rule. So each rule a firing wakes
binds what it carries on its own terms, whether the firing was an act or an
`ensures:` emission:

- **By position, under the rule's own names.** `ensures: Announced(doc)` reaches
  `when: Announced(thing)` as `thing`, `when: Announced(doc)` as `doc`, and
  `when: chains/Announced(story)` in another module as `story`. Two listeners
  need not agree on what to call an argument, and each is handed it.
- **By name, under that name only.** `ensures: Published(item: doc)` binds
  `item`. A rule waiting on `when: Published(doc)` is not handed it, even
  though it has only the one parameter: which of a rule's names an argument
  written for another name was *meant* for is a guess, and the walk does not
  make one.
- **By neither, when it is handed both.** `ensures: Moved(doc, to: doc.owner)`
  against `when: Moved(to, from)` says two things about `to` — `doc` by
  position, `doc.owner` by name — and picking one is the same guess, so `to`
  is left unbound and the reason says it was handed both.
- **`?` is `null` when nothing was passed.** `when: Reported(loan, note?)` fired
  with one argument binds `note` to `null`. Not when the firing named an
  argument the rule has no parameter for: `Noted(doc: d, reason: d)` against
  `when: Noted(doc, cause?)` may have meant `reason` for `cause`, so `cause`
  is left unbound and the reason names `reason`.
- **`_` takes its place and binds nothing.** A discard is never read, so it is
  never waited on. `when: Moved(_, to)` drops the first argument passed by
  position and binds the second as `to`. `ensures: Published(doc: doc, by:
  doc)` against `when: Published(doc, _)` binds `doc`, and `by` goes to no
  one, which is what the `_` is there to say. `_?` is a discard the same way;
  `_by` is an ordinary name. A `_` does not make an optional
  parameter beside it `null`, though: against `when: Noted(doc, _, cause?)`
  the unmatched `reason` may be meant for the `_` or for `cause`, misnamed. That
  is a guess, so `cause` stays unbound, as above.

A parameter nothing bound is the rule run about nothing, so the rule is
**undecided** — not run, and not refused — and says which name it waited for
and what the firing carried instead. The act that set it off is undecided with
it:

```text
d does Publish(d) on Desk
  what it set off: `MarkSeen` could not be decided: nothing this firing of `Published` carried binds `doc`: it was handed `item` by name, and a named argument binds only the parameter it names — in `Published(doc)`
```

The fix is in the spec: name the argument the way the listener does
(`Published(doc: doc)`), pass it by position (`Published(doc)`), or rename the
listener's parameter. A precondition that is definitely false still refuses
the rule, because that is decided whatever the missing name would have been.

#### Said before anything runs

Which parameters an emission binds depends only on its *shape* — the names it
passed and how many arguments it passed by position — so it does not need a
journey to find out. Two places say it up front, from the same rules the walk
binds by (one function answers all three, so they cannot disagree):

- **The spec, in the browser and at the terminal.** Every `ensures:` emission
  that leaves some listener's parameter unbound is a warning at the emission,
  on the emitting rule, coded `inspect.trigger.unbound` — not `allium.*`,
  because allium did not say it. Every pair is reported, whether or not a
  journey reaches it:

  ```text
  `Publish` emits `Published(item: doc)` without binding `doc` for `MarkSeen`, which listens for `Published(doc)`: it was handed `item` by name, and a named argument binds only the parameter it names
  ```

- **`check`, on the act that sets it off.** An act whose trigger wakes a rule
  that emits — or wakes a rule woken by one that emits, however far down — is
  given a **remark** for each such listener parameter, with the same words and
  the file and line of the emission — once a journey, on the first act that
  reaches it. A remark and not a verdict: whether the
  emission is reached in a world is the walk's question (it may sit in a branch
  no world takes), so it is an aside beside the step and the step still
  counts toward "N of M steps hold". The walk does not stop at a remark; it
  shows it beside the step, runs the act and, when the emission is reached,
  reports the listener **undecided** as above. An act the journey says is
  refused (`cannot do`) sets nothing off and gets no remark.

The reading this takes of the language, where the reference is silent:

- **What an emission is.** A call to a name, bare or qualified, in an
  `ensures:` — at the top
  of the clause, in a block, in either branch of an `if`, in the body of a
  `for` that names its member once, or as the value of a `let` — a `for (a,
  b) in …` is never applied, so nothing inside one is reached.
  `Entity.created(…)` is a creation, and a
  call inside a value (`x.n = f(y)`) is evaluated rather than emitted.
- **Whose trigger it is.** A bare `Published(…)` is the emitting module's. A
  qualified one, `admin/FailureReported(…)`, hands work to another module: it
  is the trigger of the module this one imported `as admin`, and is heard
  exactly as though that module had emitted it. An alias this module does not
  import, or whose `use` leads to no module in the set, names nobody's
  trigger: nothing is emitted, the walk reports the emitting rule's step
  **undecided** naming the alias, and nothing is checked here.
- **Which rules it wakes.** A rule whose `when:` calls that trigger name and
  *hears* the module the emission is from: an unqualified `when:
  Published(doc)` hears its own module, every module it `use`s and every
  module that `use`s it — whose bare `Published(…)` may be this module's
  trigger — and a qualified `when: chains/Published(doc)` hears only the
  module it imported `as chains`. Two modules neither of which imports the
  other and each have a
  `Closed` have two triggers that share a word, and neither's emission wakes
  the other's listener — in the walk, in the browser's *follow*, and here
  alike. A trigger nobody emitted — an act, or one fired from the browser's
  form — still wakes every rule waiting on its name, because which module's
  trigger a person meant is not something to guess.
- **A listener's own `let`** of a parameter's name is bound before its `when`
  is read, so a parameter it gives itself is not reported.
- **Not followed:** what a rule *reacting to a creation* emits. `check` follows
  an act through emissions only; which module's `Order.created` a reaction
  answers is a question about the instance. The spec-level warning still
  reports such an emission, and the walk still reports the listener.

### Rules woken by one firing are not a sequence

Every rule one firing wakes reads its `let`s, its `when` and its `requires` in
the world **as the trigger found it** — not as another rule woken by the same
firing left it. Rules listening for one trigger are alternatives to each other,
each guarded by its own `requires`, or independent reactions to it; nothing in
the language says one sees what the other did.

```allium
rule CreateIfMissing {
    when: Put(name)
    requires: not exists Box{name: name}
    ensures: Box.created(name: name)
}
rule ReplaceIfPresent {
    when: Put(name)
    let old = Box{name: name}
    requires: exists old
    ensures: not exists old
    ensures: Box.created(name: name)
}
```

`Put("y")` on a world with no box called `y` runs `CreateIfMissing` and refuses
`ReplaceIfPresent`, whichever is declared or named first: the box the creation
makes is not there yet for the replacement to find. Likewise two reactions to
one act — `Stamp` setting `lamp.touched = true` and `Tally` requiring
`lamp.touched = false` — both run, the second not refused by what the first
wrote.

What they ensure is read in that same world. `lamp.shade = lamp.colour` in a
rule woken beside one that sets `lamp.colour = "red"` copies the colour the
lamp had when the trigger fired, whichever rule is declared or named first,
and a state change the lifecycle refuses is refused from the value the trigger
found — so it is that rule's own refusal, and the other's write stands.

What they ensure then lands in one world, in the walk's own order. The only
thing that order could still decide is which of two values written to one
field stands, and it is not allowed to. Two rules that set the same field to
different values leave it **undecided**, and the act says which two:

```text
b does Paint(l) on Shelf
  `PaintRed` fired, but part of what it ensures was not applied: `PaintBlue` and `PaintRed` were both woken by `Paint`: `PaintBlue` set `Lamp#1.colour` to "blue" and `PaintRed` set it to "red"; the specification does not say which of the two runs first, so which value stands is undecided
then l.colour = "red"
  l.colour is unknown
```

It stays undecided for the rest of the walk, and whatever a third rule woken
by the same firing writes to it: agreeing with one of the two does not settle
the question between them. A later step that reads it is undecided, and says
the field "was set to a value nothing settled". Writes that agree, writes to
different fields, and a removal beside an assignment to what it removed come
out the same in either order, and stand. Settle a clash in the spec — a
`requires` on one rule that rules the other out — and the walk can decide it.

A rule that **should** see another's work is a rule that listens for what the
other emits, not for the same trigger: `ensures: Boxed(box)` and `when:
Boxed(box)`.

### `cannot do` — the spec refuses, and that is the point

```
ada cannot do MemberBorrows(ada, copy) on MemberShelf
```

The mirror of `cannot see`. A step where a block *works* is a step where the act
is refused — and written as `does` it reports `refused`, which is the correct
verdict about the specification and the wrong one about the journey. Correct,
and shaped exactly like a step that is simply wrong.

Satisfied when the spec refuses, and it says which precondition held the line —
or which actor, when [the surface does not face whoever acts](#does--somebody-acts):
`bob cannot do Flip(bob) on AdminPanel` holds for a user who is not an admin even
when `Flip` has no `requires` at all.
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

**A `null` deadline is not due.** `when: task: Task.stopped_at + config.ttl <=
now` asks whether a deadline has passed, and a task whose `stopped_at` is `null`
has not stopped — there is no deadline yet, so the answer is no and the rule does
not run. No guard is needed in the trigger (the language has nowhere to write
one). This holds for a state rule's own `when`, in the shape that asks *has it
passed*:

- the deadline on the earlier side and the clock on the later — `d <= now`,
  `d < now`, `now >= d`, `now > d`, the clock moved or not (`now - 1.day`);
- the deadline `null`, or `null` moved by durations the world knows —
  `stopped_at`, `stopped_at + config.ttl`, `config.ttl + stopped_at`,
  `sent_at + a + b`.

The comparison is the whole `when`: allium rejects an `and` or an `or` in a
state trigger (`allium.rule.invalidTrigger`), and a conjunct belongs in
`requires`.

The deadline has to be written in the trigger. A derived field that computes
one — `expires_at: stopped_at + config.ttl`, then `when: t: Task.expires_at <=
now` — evaluates to an unknown rather than a `null`, and stays undecided.

`then <Rule> does not fire` about such a rule is answered — and about any
state rule: when the clock settled the world in that step and none of the
listings it read was unsure about the rule, every one of them said *no*, and
the line holds. If any was unsure, or the step did not move the clock (an act
settles nothing), it is still undecided.

Everywhere else a `null` is read as it always is. `null + 1.hour` in a
`requires`, an invariant or a `then` is not a timestamp, and is undecided with
that reason. So is the opposite question in a trigger — `when: offer:
Offer.opens_at > now`, *is it still ahead* — because a `null` answers it neither
way, and reading "no deadline" as "never ahead" or "always ahead" would be the
tool choosing a meaning the spec did not write. The walk says which:

```text
1. a day passes over an offer with no opening       undecided
     after 1.day
       while the clock moved, what it set off: `Withdraw` could not be decided: a null cannot be ordered against a timestamp — in `Offer.opens_at > now`
```

That line is about any temporal rule the clock leaves undecided over the world it
settled on, not only a `null`: a loan whose `window` nobody set is not known to
be overdue three weeks on, and `after 21.days` says so rather than holding.
Each such rule is woken on the instance it could not settle before it is
reported, so one whose `requires` already refuses — a loan already returned —
is not. A *state* rule that is not temporal is not reported on the clock's line:
time does not decide it, and a `then <Rule> fires` about it, when a listing
could not decide it, still says it could not be told.

**An instance that is gone has no state left to check.** A state rule's trigger
is its entity, so the one event that runs `Cleanup` — `when: d:
Delivery.is_expired`, `ensures: not exists d` — reaches every other rule on
`Delivery` too, `TimesOut` among them, after the delivery is gone. What that
rule comes to turns on whether it was due when the event woke them both.

If it was not — the delivery was `done`, so never overdue — it does not apply:
it is refused, decided, and the refusal says which rule removed the instance:

```text
`TimesOut` refused: d: Delivery.is_overdue — `Delivery#1` was removed earlier in this step, by `Cleanup`, so it has no state left for this condition to be about
```

That holds whatever the second rule calls the instance. `Lapse` reading `l:
Lease.is_lapsed` is woken by an event that carries the lease as `Expire`'s `e`,
and is still answered about that lease. No guard such as `requires: exists d`
is needed for this case.

If it was due — the delivery was still `sending` — the two rules raced over one
delivery, and the specification does not say which runs first. The walk runs
them in an order of its own — by module, then rule name — so a verdict either way would be
that order's and would flip if a rule were renamed. It is undecided instead,
and says which two rules raced:

```text
while the clock moved, what it set off: `TimesOut` could not be decided: `TimesOut` was due on `Delivery#1` when this step began, and `Cleanup` removed it earlier in the same step; the specification does not say which of the two runs first — in `d: Delivery.is_overdue`
```

The same holds a round wider. Everything due is listed before anything fires,
so an instance one firing removes can still be waiting for its own event. That
event is not fired about something that is gone, and the rules it would have
woken are not dropped either — they were due, so they are undecided the same
way:

```text
`GiveUp` could not be decided: `GiveUp` was due on `Parcel#1` in the same round, and `Close` removed it before it ran; the specification does not say which of the two runs first
```

A race leaves the step undecided, and every `then` about the world after it
with it: the world the walk reached is one order's. Settle it in the spec — a
`requires` on one rule that rules the other out — and the walk can decide it.

Only an instance the walk removed. A reference to something the world never
held is not answered this way — it is still undecided, as `` `Delivery#9` is
not in this world ``.

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
`open_loans.count` — is computed from its definition, and when that cannot be
settled it comes back `undecided` with the simulator's reason on the line:

```text
then r.ne = true
  r.ne is unknown — `r` has no `other` set
```

The reason names the cast member the way the journey did, not by the
simulator's id for it. `stipulate` is how a journey gets past that
deliberately. And an assertion about a path nothing has set is `undecided`
**both ways round**: `exists` does not refuse it and `does not
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

**A `when` guard is read against the world.** An item the language marks as
conditionally visible —

```text
context doc: Doc
exposes:
    doc.owner
    doc.secret when doc.is_private = false
```

— is shown while its guard holds and hidden while it does not, the same as a
`for … where` filter: `sees` holds and `cannot see` is refused for a public
document, and the other way round for a private one. Inside a `for` body the
guard is about the element being visited, so one shelf can show one document's
secret and hide the next. The guard may read whoever the surface faces, so
`doc.secret when viewer.owner = doc.owner` shows the same field to one reader
and not the next. A hidden field says which guard hid it — ``… shows
`d.secret` only `when doc.is_private = false`, which does not hold here`` —
because the answer is in the journey's `given:`, not in the surface.

A guard nothing could settle — `is_private` never set — leaves **both** `sees`
and `cannot see` undecided, and the reason quotes the guard as written, ``…
`when doc.is_private = false` could not be settled``, followed by what was
missing. So does a guard that came back something other than true or false,
and so does one computed over a filtered set that left a member out because
nothing could decide it: `(shelf.docs where is_private = true).count > 0` is
not settled while one document's privacy is unsaid, however definite the count
looks, and the same holds for a `for … where` filter. The guard reaches only
the item it is written on; `doc.owner` beside it is shown whatever the guard
says.

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

**A surface shows nothing to somebody it does not face.** The actor's
condition is asked of whoever is looking, the same as
[for an act](#does--somebody-acts): definitely not one, and `cannot see` holds
— with the actor and the condition — while `sees` reads *unexposed*. Not
settled, and a `sees` it would otherwise show is undecided, and so is a
`cannot see`; a field the surface does not carry at all is still settled from
the clause text, whoever is looking. An actor identified `within` a context
reads `within` as the instance `in` named (or the one bound from the actor), so
`mine sees desk.archived on WorkspacePanel in desk` asks whether *this* seat is
in *that* workspace. Nothing naming one leaves it undecided, with the `in` that
would settle it — or, on a surface at no context of that type, saying that no
`in` can.

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

Sets a value directly, mid-journey, the way `given` does at the start (where a
call [can be answered too](#given--what-is-already-true), and in a file's
`world`, once for every journey). Use it
when reaching a state honestly would take ten steps that are not what this
journey is about. A configuration parameter is set the same way —
`stipulate config.loan_limit = 1` — and checked the same way as
[in `given`](#configuration--running-with-a-switch-turned-on).

**A call, as well as a path.** A specification names functions it never
defines — a policy nobody has decided yet — and no simulator can work one out,
now or ever. Every rule guarded by one is permanently undecided until somebody
says, and this is how they say it. The answer is matched on the argument
*values*, so the rule's `may_invite(group, issuer)` and the journey's
`may_invite(chat, ada)` are the same call; and it is about the arguments it
names, so answering for Ada says nothing about Bob.

Both answers are sayable. `= false` refuses the rule, or this would be a way of
making journeys pass rather than a way of saying what nobody has decided.

**A word as an argument.** Permission checks naturally take a kind:
`requires: can(repo, code)`, where `code` is a word the rule binds to nothing.
Write the same word in the stipulation:

```
stipulate can(r, code) = true
```

The journey reads its own `code` as the state it spells, and the spec's `code`
is read the same way *only to match a stipulation that names that word in that
place* — the judgement `copy.status = available` makes, with the journey's
stipulation as the side that says it is a state. `can(r, issues)` does not
answer `can(repo, code)`, and with no stipulation naming `code` the rule stays
undecided and says what would answer it:

```text
`Grant` could not be decided: nothing defines `can`, and no stipulation answers this call — in `can(repo, code)`
```

A name that could mean something else is never read as its spelling, because
then the call is about a value nobody knows rather than about a word:

- a name the rule binds, even to a value nobody knows;
- a parameter of the rule that nobody passed — `Ask(repo)` emitted for
  `when: Ask(repo, kind)` leaves `kind` a question, not the state `kind`;
- any bare name inside an entity's own definition. `allowed: can(this, level)`
  reads `level` as the repo's field, and a field nobody set is not bound — so
  inside an entity no word is read, and the call stays undecided.

**About what the act makes: `where`.** A reaction to a creation often asks a
black box about the new instance — who hears about an issue, `audience(issue)`,
the moment it is opened. `stipulate` cannot answer that: before the act there is
no issue to name, and after it the reaction has already asked and been left
undecided. So the answer goes under the act, beside the `creating` that names
the instance:

```no-check
repo does Open(repo, "crash") on Tracker
    creating issue: Issue
    where audience(issue) = {}
```

It is in force from the engine turn after the one that made the instance
`creating` will catch: for everything that creation sets off, and not for the
rule that made it. It is listed in the ledger like any other stipulation:

```text
Nobody Is In The Audience  —  1 of 1 steps hold
    stipulated  audience(issue) = {}
   1. open an issue whose audience is empty            holds
```

As many `where` lines as there are calls to answer, each `where <call> =
<value>`, and each held to what it is for:

- **It needs a `creating`**, because without one there is nothing to wait for.
- **It is a call, never a field.** What the act writes into what it makes is
  what the specification says, and overruling it mid-act would be a `then` that
  cannot fail. `stipulate` it after the act if that is really what you mean.
- **It names the caught instance** — `audience(issue)`, `muted(issue, repo)`,
  `reach({issue, other})`. An answer about anything else can be said with
  `stipulate` before the act, and is refused here so there is one place to say
  it.
- **It answers for that instance and no other.** An act that makes two issues
  has its first caught and answered; the second's audience is still nobody's
  answer, and the step stays undecided, with a remark beside the act saying
  the call was asked of something the answer is not about.
- **The rule that makes the instance cannot read it.** A rule that creates the
  issue and asks `audience(issue)` in its own `ensures` asked before the answer
  existed; the step stays undecided, and the remark beside the act says so.
- **Its arguments are read when the instance is made.** `reach(issue.repo)`
  reads `issue.repo` as it stood then; a field the cascade sets later is not
  what the answer is about.
- **Its words are not checked.** A bare word in the value is a word, as it is
  in a `stipulate` about a call: `{rpo}` is a set holding the word `rpo`, not
  the repo.
- **Nothing made, nothing listed.** An act that makes nothing of the kind, or
  that its surface refuses, answered no call, and the ledger does not claim it
  did.

Written as a `stipulate` after the act instead, the answer still lands — a later
act may ask the same question — but the step stays undecided, and when what the
act set off asked that very call the walk says why beside the line rather than
leaving it to look like the next line answers it:

```text
        remark  stipulate audience(issue) = {}
          said after the act that made `issue`, so nothing the act set off could read it — to answer `audience(issue)` for what it set off, write `where` under `creating issue: Issue`
```

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

A step marked **remark** is an act no rule waits for: not a failure, and not
the spec supporting anything either, so it is not counted in "N of M steps
hold". A remark `check` makes about a line — a bare word no rule compares, an
emission some listener cannot bind — is not a step's verdict at all: it is an
aside, shown under the step, and a step whose every line holds still holds
beside one.

The three that are not "holds" are three different pieces of work. "The spec
forbids this", "the spec has never heard of this" and "this tool could not tell"
send you to three different places, and a report that called all of them
*failed* would send you to change a specification that is not wrong.

**undecided is not a polite no.** An expression this simulator cannot evaluate —
a field nobody stated, a name nobody bound — comes back undecided
with the sub-expression that could not be settled, never as false.

**A collection a journey filled in is asked about member by member.**
`box.items.all(i => i.kind = a)` and `box.items.any(i => i.kind = b)` are
computed over whatever the set holds — a `stipulate box.items = {x, y}`, a
relationship the spec computes, or every instance of a type — by the same
three-valued rules as `and` and `or`. So one member that settles the answer
settles it: a `b` among the items makes `all … = a` false whatever the others
are, and an empty set makes `all` true and `any` false. Only when nothing
settled it does a member nobody described leave the step undecided, and the
reason names that member's field: `` `Item#2` has no `kind` set — in `i.kind` ``.
A relationship is only as complete as what decides it: with
`items: Item with box = this`, an item whose `box` nobody stated may or may
not be in the box, so the items it does hold can refute `all` (or satisfy
`any`) but cannot settle it by agreeing — that comes back undecided, naming
the field nobody stated. `for i in box.items: …` reads the same way, and so do
`box.items.count`, which is then not known at all; `x in box.items`, which a
member it holds settles and nothing else does; and `exists`, which one member
settles and none does not. As a postcondition, `for i in box.items:` applies
its clauses to the items it holds and says the rest were not applied, so the
act is undecided.
Write the quantifier the spec means rather than a black-box predicate a
journey then has to stipulate — the predicate hides the logic the journey was
meant to check.

**A word one rule hands the next.** `ensures: Finish(repo, cancel)` passes a
word nothing binds. When the spec declares it as a state — `enum Ending {
cancel | fail }`, or a field's inline `cancel | fail` — the rule waiting on
`Finish` receives that state, so its `how = cancel` is decided and a
`may_close(repo, how)` it calls is answered by `stipulate may_close(r, cancel)`.
A word the spec does **not** declare stays undecided: a trigger's parameters
carry no types, so nothing else says the word is a state rather than something
this simulator does not model. Declare the enumeration and the chain decides.

**A word a definition returns.** `status_now: if status = waiting: waiting
else: running` returns a bare word, and inside an entity's definition a word
the spec declares as a state — in any enumeration, or any field's inline
`waiting | running` — is that state. So `status_now` is `waiting`, and
`stale: status_now != status` is decided. A word no enumeration declares stays
undecided and names itself (*nothing is bound to `paused`*). Outside a
definition a bare word is still read as a state only where a comparison's
other side is one, or a rule hands it on as above — with one exception that
follows from how a filter is read: inside `Runs where …` in a rule or an
invariant, each run is `this`, so the run's own unset fields and declared
words are read there exactly as in a definition.

**A comparison with an undecided side is undecided.** `copy.status =
available` reads `available` as a state because the other side is one. That
reading is never given to a name that already means something: one of the
entity's own fields nobody set, a computed field that came back undecided, a
rule parameter nobody passed, or a name bound to something undecided — nor,
inside a filter, one of the *element's* fields: on a shelf, `docs where status
= active` over a doc with no `status` is undecided, not a doc whose status is
the state `status`. Each of those is a value nobody knows, so `other != status`
with `other` unset is undecided and says *`` `r` has no `other` set ``* — not
`true` because the state `other` is not `waiting`. A field spelled like a state
is still the field.

**What an act set off counts too.** A rule one hop down a chain that could not
be decided is on no line of the journey, so the act that set it off is
undecided even when its own rule fired, and says which rule and why:

```text
r does Halt(r) on Settings
  what it set off: `Halted` could not be decided: nothing is bound to `abort` — in `abort`
```

Every refusal after it in the same step is undecided with it, and the step does
not hold, whichever way a `then` about what `Halted` would have written comes
out — a *yes* about a field included, because the world it was read from is
not one the act leads to. A later step is not softened: it reads the world as
the walk left it, and a refusal there stays a refusal. The step that acted is the one that says
something was undecided, so look there first.

`all` and `any` are the two methods that take a lambda here. Any other —
`items.filter(i => …)` — and a lambda anywhere else is still undecided, with
the reason *a lambda is not simulated*.

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
