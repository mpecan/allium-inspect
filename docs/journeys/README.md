# User journeys over an Allium spec

> **Writing one?** [`reference.md`](reference.md) is the grammar,
> [`adopting.md`](adopting.md) is the path through a repository that has none,
> and [`evidence.md`](evidence.md) is how a journey gets pictures of itself.
> This file is the design and the reasoning.

> **Looking for the syntax?** [`reference.md`](reference.md) has every form a
> journey can contain, with a runnable example of each. This document is the
> reasoning: what a journey is for, and what it deliberately is not.


**All of it is built**: the grammar, the static check, the walk, a browser view
and a command of its own. Run one:

```sh
allium-journey walk specs/ journeys/                # JSON, one document per file
allium-journey walk specs/ journeys/ --text         # for a person
allium-journey check specs/ journeys/               # the static half, no world
allium-inspect --journeys journeys/ specs/          # in the browser
```

The design below is the second draft, after answers on the open questions. What
changed is the direction of the whole thing: a journey is not a test written
after a spec, it is the **demand written first**, and the spec is filled in to
satisfy it.

> Think of it the other way around: I write the journeys and then fill the spec
> to fulfil them.

That inverts everything below. A step naming a surface the spec does not have is
not an error — it is a requirement nobody has met yet, and saying so is the most
useful thing the tool can do.

## The gap

Allium says what each rule does when its trigger happens. It says nothing about
what a person does next, and has no construct for saying it.

This tool draws a **Chain** view, and the rail is careful about what it is:

> A chain is derived from which triggers a surface offers and which each rule
> emits — so it is what *follows*, not what anyone set out to do. For that,
> somebody has to write it down: see Journeys.

A derived chain follows causation: this trigger fires that rule, which emits
that trigger. A journey follows a *person*: they act, they wait a day, somebody
else acts, they come back and look at a screen, and what they see decides what
they do next. Between two triggers on a causal chain there can be a decision, a
different actor, or a night's sleep, and the chain cannot tell those apart.

`friend-mesh`'s own `docs/journeys.md` says why this matters, about a codebase
rather than a spec, and the sentence transfers exactly:

> the design has been recorded *feature-first* — a roster, an invite ticket, a
> self-group — and features do not tell you what is missing. A journey does: it
> walks from an intent to an outcome and **stops dead at the first step nobody
> built**.

A specification is recorded construct-first for the same reason and has the same
blind spot. Journeys are how you find out what it does not let anyone do.

## Two harnesses, one shape

`just journeys` in `friend-mesh` walks a real app on two real devices: boot the
simulators, launch, attach to the webview, walk thirteen numbered steps, assert
the accessibility node that proves the app got there, photograph it.

What is designed here is the same walk one level up — through the
**specification**, in the simulator this tool already has, at design time,
before anything is built.

| | `just journeys` | this |
|---|---|---|
| walks | a built app | a specification |
| runs on | two simulators and a hub | `step(spec, world, event)` |
| a step fails when | the app does not do it | the spec does not permit it |
| costs | minutes, and five external binaries | milliseconds |
| answers | did we build it | is it even specified |

They are not the same tool and this does not propose merging them. But a step is
a sentence in a person's terms in both — *"Ada's phone goes off, and Bruno writes
to her anyway"* — and the sentences should be able to be the same ones, so that a
journey rehearsed against the spec becomes the walk that proves it later.

## The insight the design rests on

The engine already exists. `step(spec, world, event)` applies the rules that wait
for a trigger, checks every invariant, and reports what changed and what it could
not decide. The clock is a field you advance. A run is deterministic.

That is a journey execution engine with no journeys in it. What is missing is a
script.

## What a journey is

One or more **actors'** paths, through **acts the spec permits**, over a **world
that has been said to exist**, with **time passing where it passes**, asserting
both what becomes true and **what each person can see**, ending in an outcome
stated in words.

Linear. A branch is a second journey, which keeps a journey something you can
read aloud — and `friend-mesh` has seven separate walks rather than one with
conditionals for exactly that reason.

## Where it lives

Beside the specs, and read with them:

```text
specs/     messaging.allium  delivery.allium  …
journeys/  deletion-taken-back.journey  deletion-becomes-final.journey  …
```

`allium-inspect --journeys journeys/ specs/`. The spec set does not know they
exist: `allium check` says the same thing either way. The syntax is deliberately
Allium-shaped so graduating it into the language later is a rename.

## A worked example

Real, against `friend-mesh`. Two people, because one-person journeys are the easy
half and not what any of this is for.

```
-- What "everywhere" costs, and what taking it back does not undo. The messages
-- are gone from the screen the moment the intent is raised: the window buys the
-- author a change of mind, not a delay before anything happens.
journey DeletionCanBeTakenBack {
    goal: Ada deletes a message everywhere, changes her mind two hours later,
          and takes it back before the day is out — and Bruno, who was reading
          it, is told once rather than twice.

    cast:
        ada:        identity/Identity   -- deletes, then thinks better of it
        her_phone:  identity/Device
        bruno:      identity/Identity   -- was reading, and is only a bystander
        his_phone:  identity/Device

    given:
        ada.status = active
        her_phone.identity = ada
        her_phone.status = active

        bruno.status = active
        his_phone.identity = bruno
        his_phone.status = active

        chat: membership/Group {}
        note: messaging/Message { author: ada, group: chat, body: "forget I said that" }

    1. ada deletes it everywhere
        ada does PersonDeletesEverywhere(ada, her_phone, {note}) on Conversation
        then note.status = tombstoned
        then DeleteIntent#1.status = applied
        then DeleteAcrossMyDevices fires
        ada sees DeleteIntent#1.targets.count on OpenDeletions

    2. bruno's copy goes too, and his screen says so
        bruno cannot see note.body on Conversation

    3. two hours later she takes it back
        after 2.hours
        ada does PersonCancelsDeletion(DeleteIntent#1, her_phone) on OpenDeletions
        then DeleteIntent#1.status = cancelled
        then VetoAnnouncement#1 exists

    ends: The intent is cancelled inside the window, and Bruno was told once.
}
```

Step 2 is where a journey earns its keep, and it is the shape `just journeys`
already taught: *"a step asking whether that word was anywhere on screen would be
answered by the wrong message and pass."* `bruno cannot see note.body` names one
message. Not "nothing on his screen says that" — *this* message, on *his*
surface, given *his* membership.

## Two people, two states, one world

> We need to be able to model more than one instance of an actor, they can have
> different preconditions.

So `cast` is instances, not roles, and each gets its own lines in `given`. Ada
and Bruno are both `identity/Identity`; her phone is active and his may not be.
That is the whole of the `just journeys` step 10 — *"Ada's phone goes off, and
Bruno writes to her anyway"* — and it is a precondition on one instance, not a
second world.

One world, many instances, is also what the simulator already does, so nothing
new is needed to run it. Where two devices genuinely diverge, the spec already
models it: `OutboxEntry.awaiting` is the set of devices that do not have it yet.
A journey asserting `his_phone in OutboxEntry#1.awaiting` is asking a question
the spec can already answer.

## `given` is precise, and that costs lines

> Precision is important and it needs to be checkable.

So no `given: a group with two members`. Every instance is named, every field it
needs is assigned, and each line is checked against the entity's declared fields.
The setup will sometimes be longer than the journey, and the alternative —
shorthand that invents a shape the spec never stated — puts facts in the spec's
mouth, which is the one thing this tool does not do anywhere else.

Two things take the sting out without inventing anything. `given` may leave a
field unset, and an unset field the journey never reads costs nothing; if a rule
reads it, the step comes back **undecided** naming that field, which is the same
answer the simulator gives today. And a `given` block may be shared by the
journeys in one file, since the cost is per file rather than per journey.

## Seeing is behaviour

> If it is a behaviour, we need to be able to model it. Seeing a message or
> content is important.

So `sees` is a question about a boundary, and it has two halves. **Half of it is
built.**

The half that works is whether the surface carries the field at all. That is a
fact about the `exposes` block, and it settles a `cannot see` outright — not "no
instance matched" but "this boundary does not carry it", which is the strongest
form the claim has.

The half that is not built is the filter. `OpenDeletions` exposes `for intent in
DeleteIntents where owner = owner and status = applied: intent.targets.count`.
Asking whether Ada sees `DeleteIntent#1.targets.count` means evaluating that
filter with `owner` bound to Ada, which needs the `exposes` clause as an
expression rather than as text. It is stored as text today. So once the surface
*does* carry the field, neither direction can be settled, and both come back
**undecided** with a reason saying which half is unread.

That means a positive `sees` never holds yet. It is the honest answer rather than
a satisfying one, and it is the right way round:

**A `cannot see` that could not be evaluated is undecided, never safe.** A
privacy claim that passes because the tool could not check it is the worst output
available here, and refusing it is the single most important rule in this design.
Reading the *value* instead of the surface was how that rule got broken once: a
field nothing had set made the claim come back satisfied, so `ada cannot see
ada.open_loan_count on MemberShelf` held against a surface that exposes it on the
line above.

## Status, not pass/fail

The inversion changes what a report is. `friend-mesh`'s own journeys file has the
right vocabulary already, one level down; these are its spec-level counterparts:

| Mark | Means |
|---|---|
| **holds** | the spec permits this step and the assertions are true |
| **undecided** | something could not be evaluated, and here is the sub-expression |
| **refused** | the spec forbids it — a precondition is definitely false |
| **unspecified** | the step names a surface, operation or exposure the spec does not have |
| **unexposed** | the act exists but nothing lets this actor see the result |
| **remark** | worth a person's attention, and not a reason to stop |

The last two are the point of writing journeys first. **unspecified** is a
requirement nobody has met. **unexposed** is a system that does the right thing
and tells nobody — and `friend-mesh` has one: `OpenDeletions` exposes deletions
`where status = applied`, so once an intent settles it vanishes from the only
screen that ever mentioned it. A second journey states it in one line:

```no-check
journey DeletionBecomesFinal {
    …
    2. the day passes and it settles
        after 24.hours
        then DeletionSettles fires
        then DeleteIntent#1.status = settled
        ada sees DeleteIntent#1.status on OpenDeletions     -- unexposed
}
```

A journey's own status is the worst of its steps, and a run reports the ledger:
how many steps hold, and which spec constructs the rest are waiting on.

## Strictness is the caller's choice

> Report, do not reject is one option, I would allow a switch that allows
> either, default to report.

```text
allium-journey walk  --report   every step gets a status; exit 0
allium-journey walk             unspecified or refused is a failure; exit non-zero

allium-inspect --journeys PATH --check            the same, without a browser
allium-inspect --journeys PATH --check --strict   ... and --json for a pipe
```

Report is the default because that is the mode you write a journey in: you write
the walk, the tool tells you the spec has four of its seven steps, and the four
you are missing are the next thing to specify. Strict is the mode you defend a
finished journey in, and it is the one a build gate would use — on the journeys
somebody has decided are done, not on all of them.

That also settles the ownership question:

> The journey is an extension of the spec, being tested through behaviour.

A journey is not a document beside the spec that drifts from it. It is part of
the specification, and what keeps it honest is that it is executed.

## Stipulations, and why they matter for agents

The simulator cannot compute derived values, `JoinLookup`s, or calls into an
implementation. A journey over a real spec will hit those constantly.

```
stipulate ada.active_devices.count = 2
```

*Take this as true; I am not asking you to work it out.* The rule is that **every
stipulation appears at the top of the report**, always:

```text
DeletionCanBeTakenBack — holds, given 2 things it was told rather than shown
    stipulated  ada.active_devices.count = 2
    stipulated  note.has_attachment = false
```

A journey with no stipulations is a claim the spec supports on its own. One with
nine is a claim about something else, and the reader can tell at a glance.

This is the guardrail for agent-written journeys. An agent can make any journey
pass. It cannot make one pass *invisibly*.

## How an agent writes one

1. The agent is given the spec set and an intent in a person's terms — from a
   product doc, a bug report, or a human sentence.
2. It writes the walk: cast, `given`, numbered steps, what each person sees. It
   may name constructs the spec does not have; in report mode that is the output,
   not an error.
3. `allium-journey check --json` returns a status per step.
4. **unspecified** steps are the agent's brief: these are the surfaces,
   operations and exposures the spec still owes. It writes them — or hands the
   list to whoever owns the spec.
5. **refused** means the spec actively forbids the journey. That is a
   disagreement to raise, never an assertion to weaken.
6. **undecided** means the simulator could not tell. A `stipulate` moves past it
   and stays visible to every human who reads the report.

Which makes the pair with `allium propagate` symmetrical: propagate turns a spec
into the obligations an implementation owes; this turns a journey into the
obligations a *spec* owes.

## What the tool does with them

1. **`allium-journey check` and `walk`** — the run above, `--text` for a
   person or JSON for a pipe. `allium-inspect --journeys PATH --check` is the
   same check from the other binary.
2. **A Journeys view** — the written journey drawn against the derived chain,
   with the divergences marked. Steps that are `unspecified` draw as gaps rather
   than as constructs, which makes a half-specified journey legible at a glance.
   The journey's own source sits along the bottom, the way a spec's does:
   selecting a step moves the strip to the line that step is written on, so the
   verdict and the sentence that earned it are never in two different places.
3. **Evidence** — a picture of the software doing the step, under the step. A
   test run photographs the product, a marker in the code says which step a test
   demonstrates, and the panel tells *shown* from *claimed and never
   photographed* from *photographed before the step was reworded*. The whole
   chain is in [`evidence.md`](evidence.md).

### Not built

Both of these are still ideas, listed here because the shape of the tool makes
them cheap and because a reader should be told which parts they cannot use yet.

4. **Coverage, both ways** — which surface operations no journey exercises, and
   which journey steps no spec construct answers. The second is the backlog.
5. **Proposed journeys** — skeletons from surfaces and traces, for filling in.
   Useful for covering a spec that already exists; it is *not* the main path, and
   a tool that only proposed journeys derived from the spec could never find a
   step nobody had specified.

## What this deliberately is not

- **Not branching.** No `if`, no loops.
- **Not a wireframe.** `sees` says a person can observe a value, never how it
  looks or where.
- **Not new semantics.** A journey may not assert anything the spec cannot check.
  A step that needs a new fact is a step asking for that fact to be *specified* —
  which is the whole point — rather than a place to write it down instead.
- **Not a replacement for `just journeys`.** That walks a built app on real
  devices and proves things no simulator can.
- **Not performance, availability, or error copy.**

## The thing the example gets wrong, on purpose

`Conversation` faces `membership/Member`. `PersonDeletesEverywhere(owner, device,
targets)` takes an `identity/Identity`. The journey casts Ada as an Identity and
has her act on a surface facing a Member.

That is either a small inconsistency in the spec or entirely correct — the
surface faces the member, the act names the identity behind them, a person is
both. In report mode the tool says so and moves on. In strict mode it is a
failure, and if it turns out to be legitimate, the fix is a line in the spec that
says so rather than a rule in the checker that guesses.

## Still open

- **How much of `sees` can actually be evaluated?** Measured, on `friend-mesh`:
  **none of it yet**, and the reason is not the one this entry expected.

  Every one of the seven undecided `sees` lines in that set is exposed through
  a `for … in <collection>:` block, and every one of those collections is a
  *derived* value — `identity.listed_devices`, `group.active_memberships`,
  `identity.recovery_claims where status = pending`. Answering whether a
  surface shows *this* device to *this* person means computing the collection
  first. Not one of the seven is the bare `entity.field` case that could be
  decided today by reading the clause as an expression.

  So the work `sees` is waiting on is **derived values**, not exposure parsing,
  and that is a general capability rather than a `sees` feature: the same nine
  or so undecided lines in that walk include `claim.attestations.count is
  unknown`, which is the same gap seen from the other side.

  What this does *not* change is the verdict when a surface carries nothing
  like the field — that is read from the clause text and settled, including
  the negative case. A privacy claim that passed because nothing checked it
  would still be the worst answer this tool could give.
- ~~**What does a step number mean when a journey changes?**~~ Settled, and by
  building on it rather than by deciding: a step number is now a citation that
  code carries (`// journey: Name.3`), so renumbering on an insert breaks real
  references and not only prose ones. Never renumber. Rewording a step is
  allowed and detected — the pictures of it go `stale` and say what it used to
  say — which is the safety the numbers do not need.
- **Does `given` need a spec-level home?** If journeys are part of the
  specification, a world every journey in a module shares starts to look like
  something the spec should declare rather than something each file repeats.
