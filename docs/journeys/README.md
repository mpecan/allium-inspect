# User journeys over an Allium spec

*A design. Nothing here is built yet.*

## The gap

Allium says what each rule does when its trigger happens. It says nothing
anywhere about what a person does next, and it has no construct for saying it.

This tool already draws a **Journey** view, and the rail is careful about what
that view actually is:

> A trace is derived from which triggers a surface offers and which each rule
> emits. Allium has no journey construct, so this is what follows — not what a
> person does.

The difference is not a detail. A derived trace follows causation through the
system: this trigger fires that rule, which emits that trigger, which fires the
next. A journey follows a *person*: they act, they wait a day, somebody else
acts, they come back and look at a screen, and what they see decides what they
do next. Between two triggers on a causal chain there can be a decision, a
different actor, a night's sleep, or nothing at all — and the causal chain has no
way to tell those apart.

So the tool can show what the system does and cannot show what anyone gets.

## The insight this design rests on

The engine already exists.

`step(spec, world, event)` takes a world and a trigger, applies the rules that
wait for it, checks every invariant, and reports what changed, what it could not
decide, and which state-condition rules the change just made possible. The clock
is a field you advance. A run is deterministic and replayable.

That is a journey execution engine with no journeys in it. What is missing is
not machinery — it is a *script*.

Which means a journey should not be prose about the spec. It should be an
**executable claim about what an actor can do, checked against the spec**, and
the tool should be able to answer: does this specification actually support this?

## What a journey is

A journey is:

- one **actor's** path, sometimes crossing another's,
- through **acts the spec says they may perform**,
- over a **world** that must be said to exist,
- with **time** passing where it passes,
- asserting both what becomes **true** and what the person can **see**,
- ending in an outcome stated in words.

It is linear. There are no branches: a branch is a second journey, and saying so
keeps a journey something a person can read aloud.

## Where it lives

Outside Allium, for now. A sidecar directory beside the specs:

```
specs/
    messaging.allium
    delivery.allium
journeys/
    deletion-can-be-taken-back.journey
    deletion-becomes-final.journey
```

`allium-inspect --journeys journeys/ specs/` reads both. The spec set does not
know the journeys exist, which is the point: nothing about this changes what
`allium check` says, and a spec set with no journeys is exactly as valid as one
with fifty.

The syntax is deliberately shaped like Allium's own — blocks, keyword clauses,
`--` comments — so that a reader who has one in their head has both, and so that
graduating it into the language later is a rename rather than a rewrite.

## A worked example

Real, against `friend-mesh`. Someone deletes a conversation everywhere, thinks
better of it inside the day, and takes it back.

```
-- What "everywhere" costs, and what taking it back does not undo. The messages
-- are gone from the screen the moment the intent is raised: the window buys the
-- author a change of mind, not a delay before anything happens.
journey DeletionCanBeTakenBack {
    goal: Ada deletes a message everywhere, changes her mind two hours later,
          and takes it back before the day is out.

    cast:
        ada:   identity/Identity
        phone: identity/Device

    given:
        ada.status = active
        phone.identity = ada
        phone.status = active
        note: messaging/Message { author: ada, body: "forget I said that" }

    ada does PersonDeletesEverywhere(ada, phone, {note}) on Conversation
        then note.status = tombstoned
        then DeleteIntent#1.status = applied
        then DeleteAcrossMyDevices fires
        ada sees DeleteIntent#1.targets.count on OpenDeletions

    after 2.hours
        ada does PersonCancelsDeletion(DeleteIntent#1, phone) on OpenDeletions
        then DeleteIntent#1.status = cancelled
        then VetoAnnouncement#1 exists

    ends: The intent is cancelled inside the window, and the people she was
          talking to are told once rather than twice.
}
```

And its sibling, which is the same story with nobody changing their mind:

```
journey DeletionBecomesFinal {
    goal: Ada deletes a message everywhere and lets the day pass, and it settles
          without anybody doing anything else.

    cast:
        ada:   identity/Identity
        phone: identity/Device

    given:
        ada.status = active
        phone.identity = ada
        phone.status = active
        note: messaging/Message { author: ada, body: "forget I said that" }

    ada does PersonDeletesEverywhere(ada, phone, {note}) on Conversation
        then DeleteIntent#1.status = applied

    after 24.hours
        then DeletionSettles fires
        then DeleteIntent#1.status = settled
        ada cannot see DeleteIntent#1.created_at on OpenDeletions

    ends: The deletion is final, and the screen that showed it pending shows
          nothing at all.
}
```

**That last assertion is the point of the whole exercise.** `OpenDeletions`
exposes deletions `where owner = owner and status = applied`. A settled intent
does not match, so it vanishes from the only screen that ever mentioned it. The
system did the right thing and told nobody it had finished. No view in this tool
can show that today; a journey states it in one line, and the checker can prove
it from the surface's own `exposes` clause.

Whether the vanishing is a defect or intended is a question for whoever owns the
spec. Making it a *question* is the contribution.

### The thing that example gets wrong, on purpose

`Conversation` faces `membership/Member`. `PersonDeletesEverywhere(owner, device,
targets)` takes an `identity/Identity`. The journey above casts Ada as an
Identity and has her act on a surface that faces a Member.

That is either a small inconsistency in the spec, or it is entirely correct — the
surface faces the member; the act names the identity behind them; a person is
both. I do not know which, and neither would a checker.

So it is the first real design constraint: **the static pass must not demand that
a cast type match the surface's actor.** A checker strict enough to reject that
journey would reject a legitimate one, and the way to be useful here is to
*report the mismatch* — "Ada acts on a surface facing `membership/Member` and is
cast as `identity/Identity`" — and let a person decide. A validator that guesses
which of two readings the author meant is the same failure as a simulator that
guesses a truth value.

## The clauses

| | |
|---|---|
| `journey <Name> { … }` | one journey, named as a claim rather than a scenario number |
| `goal:` | one or two sentences, in the actor's terms. Never load-bearing |
| `cast:` | `name: Type` per party. Types are constructs the spec declares |
| `given:` | the world before anything happens — assignments and instances |
| `<actor> does <Trigger>(args) on <Surface>` | an act |
| `after <duration>` | the clock advances; temporal rules that come true fire |
| `then <assertion>` | what must hold after the step above it |
| `then <Rule> fires` / `does not fire` | which rules ran |
| `<actor> sees <path> on <Surface>` | what the person can observe |
| `<actor> cannot see <path> on <Surface>` | what they must not |
| `stipulate <path> = <value>` | a fact the simulator cannot compute |
| `ends:` | the outcome, in words |

Instances are named the way the simulator already names them — `DeleteIntent#1`
— so what a journey says and what a step outcome says are the same vocabulary.

## What the checker does

Two passes, and the first is worth more than it sounds.

**Statically, against the spec alone.** Every name resolves. Every act is a
trigger some surface `provides`. Every `does … on S` names a surface that
actually offers that trigger. Every `sees … on S` names a path inside that
surface's `exposes` clause. Every `after` is a duration. Every type in `cast`
exists.

This pass needs no simulator and no world, and it catches the things worth
catching: an act nobody offers, an observation nothing exposes, a journey that
still names a construct somebody renamed last week. It is also what makes the
format safe for an agent to write, because there is no way to be vaguely right.

**Dynamically, through the step engine.** Seed a world, apply `given`, then walk
the steps: fire each act, advance the clock, evaluate each assertion. Every
verdict is the same three the simulator already gives — and the third one is why
this is worth building rather than a test framework.

| | |
|---|---|
| **holds** | every step fired and every assertion was true |
| **broken** | a step was refused, or an assertion was definitely false. The spec forbids this journey |
| **undecided** | something could not be evaluated. The spec may or may not support this, and here is the sub-expression that stopped it |

A journey never comes back green by accident. In particular a `cannot see`
assertion whose filter could not be evaluated is **undecided, not safe** — a
privacy claim that passes because the tool could not check it is the worst
possible output, and the one this design most needs to refuse.

## Stipulations, and why they are the interesting part

The simulator cannot compute derived values, `JoinLookup`s, or calls into the
implementation. On a real spec, a journey will hit those constantly.

`stipulate` is how a journey gets past one: *take this as true, I am not asking
you to work it out.*

```
stipulate ada.active_devices.count = 2
```

The rule is that **every stipulation appears in the report**, always, at the top:

```
DeletionCanBeTakenBack — holds, given 2 things it was told rather than shown
    stipulated  ada.active_devices.count = 2
    stipulated  note.has_attachment = false
```

A journey with no stipulations is a claim the spec supports on its own. A journey
with nine is a claim about something else. The reader can tell at a glance, which
is the only thing that keeps "holds" meaning anything.

## What the tool does with them

1. **`--check-journeys`** — the two passes above, human-readable or `--json`.
2. **`--propose-journeys`** — skeletons. Every operation a surface offers is the
   opening act of some journey, and the derived trace already knows what can
   follow it. The tool emits the shape with the prose left blank; a person or an
   agent supplies the intent. It proposes; it never claims the result is a
   journey anyone wants.
3. **A Journeys view** — the specified journey drawn against the derived trace,
   with the places they disagree marked. "Here is what a person does. Here is
   what the system does. Here is where the second does not carry the first."
4. **Coverage** — which surface operations no journey opens with; which rules no
   journey reaches. The reciprocal of the obligations `allium propagate`
   generates: obligations are what the spec demands of an implementation,
   coverage is what anyone has actually claimed you can do with it.

## How an agent writes one

The loop this format is shaped for:

1. The agent reads the spec set and `--propose-journeys`.
2. It writes intent — the goal, the cast, what the person should see. It cannot
   invent a trigger, a surface or a field, because the static pass rejects any
   name the spec does not declare.
3. It runs `--check-journeys --json` and gets a verdict per step.
4. **broken** means the spec does not support the journey. The agent's move is
   to fix the journey or raise the disagreement — never to weaken the assertion
   until it passes.
5. **undecided** means the simulator could not tell. The agent adds a
   `stipulate`, and that stipulation is now visible to every human who reads the
   report.

That last property is the guardrail, and it is why the format has `stipulate`
rather than letting an agent quietly assert whatever makes the run green. An
agent can make any journey pass; it cannot make it pass *invisibly*.

## What this deliberately is not

- **Not branching.** No `if`, no loops. A second path is a second journey, and
  the pair reads better than the flowchart would.
- **Not a wireframe.** No layout, no copy, no ordering of a screen. `sees` says
  a person can observe a thing, never how it looks.
- **Not new semantics.** A journey may not assert anything the spec cannot
  check. If a journey needs a fact the spec does not carry, the fact belongs in
  the spec — that is the whole discipline, and the moment it slips the journeys
  become a second, unverified specification.
- **Not a replacement for obligations.** `allium propagate` says what an
  implementation owes. Journeys say what a person is owed. Both are needed and
  neither derives the other.
- **Not performance, not availability, not error copy.**

## Graduating into Allium

If this earns its place, it becomes `journey` in the language proper, and the
work is a parser change rather than a redesign — which is the reason for shaping
the syntax like Allium's now, while it costs nothing.

Two things would have to be true first. The format has to survive a spec set
nobody wrote it for. And journeys have to catch something no other view does,
often enough to be worth a construct — the vanishing deletion above is one, and
one is not a pattern.

## Open questions

Honest ones, which I cannot settle alone.

- **Who owns a journey?** If they live beside the spec, they drift like anything
  else. Coverage tells you a journey stopped touching a rule; nothing tells you a
  journey stopped being what anyone wanted.
- **Do the two actors in a journey need separate worlds?** `friend-mesh` is a
  mesh: two devices genuinely hold different states, and "Bob has not received it
  yet" is a first-class situation. One world with an `awaiting` set models that,
  and I am not certain it always will.
- **How much world does `given` have to build?** Most rules act on entities that
  are already there, and the setup can end up longer than the journey. A
  `given: a group with two members` shorthand would help and would be inventing
  facts the spec did not state.
- **Does `sees` need to follow the projection?** `OpenDeletions` exposes
  `for intent in DeleteIntents where …: intent.targets.count`. Checking the path
  is inside the clause is easy; checking the *filter admits this actor* is the
  part that makes a privacy assertion mean something, and it is exactly the part
  the simulator finds hardest.
- **How far should the static pass go?** The mismatch above says: report, do not
  reject. That line is easy to state and hard to hold — every rule that would
  catch a real mistake also catches a legitimate spelling of something else.
- **Should a broken journey fail a build?** A spec under development breaks
  journeys constantly, and a gate that cries wolf is a gate people turn off.
