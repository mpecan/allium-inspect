# Surface review — 20 August 2026

Every surface of `allium-inspect` walked as each of the five
[personas](../personas/), against `../friend-mesh/specs/` — five modules, 6,700
lines, 353 constructs, 526 edges, 712 obligations, 5 analysis findings, 14
diagnostics.

Findings are evidence first. Where something is broken the evidence is a thing
you can check, not a thing I thought.

## Summary

Ten findings. Four are defects rather than design opinions: the tool reports
problems the spec does not have, hides every problem it does have, and tells the
browser nothing when the spec changes underneath it. **Those four are fixed**;
see [Fixed](#fixed) for what each one is now.

| | Finding | Who | Severity | State |
|---|---|---|---|---|
| F1 | Live reload never reaches the browser | Ines | high | fixed |
| F2 | Every diagnostic is unreachable, including the badged ones | Ines, Tomas | high | fixed |
| F3 | Nine of the fourteen diagnostics are the tool's own artefact | Tomas | high | fixed |
| F4 | Analysis findings are a count, not a place | Tomas | high | fixed |
| F5 | The module checkboxes do nothing in the simulator | Dan | medium | open |
| F6 | The trace controls explain themselves only on hover | Dan, Aoife | medium | open |
| F7 | No path from a field to the rules that write it | Priya | medium | open |
| F8 | The trigger picker has no heading | Dan | low | open |
| F9 | A cross-module type in the inspector is inert text | Priya | low | open |
| F10 | A missing space in an unresolved note | Ines | low | open |

Nine checks passed outright and are recorded below, because a review that lists
only faults gives no sense of where the bar already is.

---

## F1 — Live reload never reaches the browser

**Ines Okonjo.** Her first check, and the one she said would make her stop.

The server watches the spec set and re-ingests on change. The browser never
finds out. `App.svelte` fetches `/api/spec` once, in a mount effect, and nothing
polls, subscribes or re-fetches.

Evidence:

```
$ grep -o 'api/[a-z/]*' crates/inspect-server/assets/assets/*.js | sort -u
api/sim/setup
api/sim/step
api/spec
api/spec/source/
```

`/api/health` is served and never requested. There is no `/api/events` route at
all — the SSE endpoint the plan called for was not built.

So Ines saves `delivery.allium`, the server logs `reloaded 353 constructs, 526
edges`, and her browser goes on showing the graph from whenever she last
hard-refreshed. She has no way to tell the two apart.

It is worse when the spec breaks. Appending a malformed rule and saving:

```
$ curl -s localhost:7272/api/health
{"ok":false,...,"errors":18,"warnings":12}
```

The server knows there are eighteen errors. It keeps serving the last good graph
— which is the right call, and is [stipulation 1](../../CLAUDE.md) working — and
the browser displays it with no indication that it is stale or that the file on
disk no longer parses. That is precisely the failure Ines named: *"it kept
serving the last good one and said nothing."*

`--no-watch` exists as a flag to turn off a thing that, from the reader's seat,
is not on.

## F2 — Every diagnostic is unreachable, including the badged ones

**Ines and Tomas.** The graph carries fourteen diagnostics. None can be read in
the interface.

The inspector joins a diagnostic to the selected construct by line number:

```svelte
diagnostic.location?.line === selectedPosition.line
```

Allium reports a diagnostic on the offending line, which is inside the
construct, not on its declaration. Measured over this spec set:

| Construct | Declared at | Diagnostic at |
|---|---|---|
| `IdentityRetirement` | identity.allium:530 | 534, 534 |
| `Device` | identity.allium:126 | 201 |
| `Group` | membership.allium:50 | 72 |

The line never matches, so the **Reported** section never renders. The remaining
diagnostics sit on lines 4–6, where no construct is declared at all.

The badge makes it worse rather than better. `worstByNode` joins on
`diagnostic.node`, which the graph *does* carry, so three nodes are badged:

```
badgedNames: ["Device", "IdentityRetirement", "Group"]
title:       "info reported in this module"
```

Two join keys for one relationship. The canvas promises there is something to
read here; the panel, asked, has nothing to say. Tomas would rather it had never
badged them.

The two lost warnings on `IdentityRetirement` are the most valuable output in
the whole ingestion:

> Status 'pending' in entity 'IdentityRetirement' has no observed transition to
> a different status.
>
> Status 'effective' in entity 'IdentityRetirement' is never assigned by any
> rule ensures clause.

That is a broken lifecycle, stated plainly, thrown away by a `===`.

## F3 — Nine of the fourteen diagnostics are the tool's own artefact

**Tomas Berg.** He would have put these in a review, and he would have been
wrong, and it would have been the tool's fault.

Ingestion runs the CLI once per file. A file's `use ./identity.allium` therefore
resolves against a check set of one, and the CLI correctly says it does not
resolve. Over the whole set it resolves fine:

```
$ allium check ../friend-mesh/specs/          # the real question
"diagnostics": []
$ allium check ../friend-mesh/specs/archive.allium   # the question we ask
"message": "Use path \"./identity.allium\" does not resolve to a file in the
            current check set."
```

Nine of fourteen are this. The tool's own linker resolved every one of them —
`--print-graph` reports `unresolved imports 0` — and it reports them as warnings
anyway.

A tool whose purpose is to be trusted about a specification is manufacturing
defects in it.

## F4 — Analysis findings are a count, not a place

**Tomas Berg.** His first and highest-value question, and the answer is a
sentence he cannot click.

```html
<p class="address">5 analysis findings</p>
```

A `<p>`. Not a control, not a link, no route to what it counts. The five are
reachable only by selecting one of the constructs each names — which means
guessing which five of 353 to try.

They are worth reaching. All five are conflicts of the same shape:

> Rules 'VetoDeletion' and 'DeletionSettles' can both fire when entity
> 'DeleteIntent' is in state 'applied', setting status to conflicting values

Five pairs of rules that race on one entity, in a spec set about to be built.
This is the single most valuable thing the tool ingests, and its whole
presentation is the number 5 in the footer.

## F5 — The module checkboxes do nothing in the simulator

**Dan Reyes.** The rail keeps **Modules** in the Simulate view. Switching
`archive` off leaves `ArchiveControls` in the trigger picker; the count of
groups does not change. `hidden` feeds the canvas projection, which the
simulator does not use.

A control that is present, enabled, and inert. Dan will click it, see nothing
happen, and privately downgrade everything else the interface claims.

## F6 — The trace controls explain themselves only on hover

**Dan and Aoife.** The view controls are exemplary: every one carries the
question it answers, on screen.

```
Domain     what the spec holds
Flow       what happens, and in what order
Lifecycle  how each entity changes state
Journey    what follows from an action
Simulate   fire a trigger and watch
```

Four lines below, the trace controls are bare words — `All`, `Follows`,
`Leads here`, `Adjacent` — with their meanings in `title` attributes. Dan is
watching someone else's screen and will never see a tooltip. Aoife does not know
that "trace" is a thing one does to a spec.

The same interface got this right once and wrong once, eighty pixels apart.

## F7 — No path from a field to the rules that write it

**Priya Raman.** Before she changes how `OutboxEntry.status` is written she
needs the rules that write it and the invariants that constrain it. The pop-up's
**Leads here** gets her the rules that create the entity; nothing narrows to a
field. Her fallback is reading 112 rules or grepping, and grepping wins.

The data is there — a rule's `ensures` names the field — so this is a missing
projection rather than a missing fact.

## F8 — The trigger picker has no heading

**Dan Reyes.** The simulator's left column opens straight into
`ARCHIVECONTROLS · IDENTITY/IDENTITYOWNER` and a list of names, with no `<h2>`
and no sentence saying what any of it is. The two panels either side of it both
explain themselves; this one, the one you must use first, does not.

## F9 — A cross-module type in the inspector is inert text

**Priya Raman.** `OutboxEntry.message` has type `messaging/Message`. The
inspector renders it as text. She has the name, so search is two gestures — but
the tool has the resolved edge and could have taken her there in one.

## F10 — A missing space in an unresolved note

**Ines Okonjo.** Rendered:

> `JoinLookup` expressions are not simulated— Membership{group: group, member: adopter}

`Trace.svelte` puts `{note.reason}` immediately against `{#if note.expression}`,
and the block's leading whitespace is trimmed. Trivial, and on the panel whose
entire job is being believed.

---

## What passed

| Check | Persona | Result |
|---|---|---|
| Search by kind with no names known | Aoife | `surface` → all 30 surfaces, `12 of 30 — keep typing to narrow it` |
| Name to fields to source | Priya | two gestures; `delivery.allium:78` |
| Types shown in full | Priya | `Set<identity/Device>`, `messaging/Message` — not truncated |
| Obligations attached to the construct | Priya | `Owes 5 tests`, each a sentence she can act on |
| View labels answer a question | Aoife, Dan | every one; the best thing in the interface |
| `--print-graph` reproduces every number | Tomas | 353 / 526 / 55 cross-module / 712 / 5 |
| Undecided distinguishable from false | Dan | `COULD NOT BE DECIDED` on the card, per-clause reasons beneath |
| The pop-up follows a thread | Aoife | double-click walks construct to construct; the canvas behind holds still |
| Empty states say what to do | Aoife | "Pick a construct on the canvas…", "Nothing exists yet. Most rules act on entities that are already there…" |

## What this says overall

The parts built to be read are good, and the trace caveat — *"Allium has no
journey construct, so this is what follows, not what a person does"* — is the
tool at its best: it tells you what it derived and refuses to pass it off as
what the spec said.

The gap is everywhere the tool has been *told* something and does not pass it
on. It ingests `analyse` and shows a number. It ingests diagnostics, badges
three constructs with them, and then cannot produce them. It re-reads the spec
on change and never says so. Three of the four commands it goes to the trouble
of running end in a surface that drops what they returned.

F1 to F4 are one shape of mistake, and they are the ones to fix.

---

## Fixed

Same day, in the order the severities argued for. Each was re-checked against
`friend-mesh` with the persona's own test.

### F3 — 14 diagnostics became 4, and all four are real

`link()` now drops an `allium.use.unresolvedPath` warning whose import this tool
resolved to a module it holds. An import that genuinely names a file nobody
passed in keeps its warning, because that is exactly what Tomas needs told.

```
$ allium-inspect --print-graph ../friend-mesh/specs/
diagnostics now 4
  info    | identity : 201 | Field 'Device.is_last' is declared but not referenced
  warning | identity : 534 | Status 'pending' … has no observed transition
  warning | identity : 534 | Status 'effective' … is never assigned by any rule
  info    | membership : 72 | Field 'Group.members' is declared but not referenced
```

Every survivor carries a `node`.

### F2 — the panel joins on the key the badge already used

`reportedAgainst` matches `diagnostic.node`, which is the server's own
attribution and the same key `worstByNode` draws the badge from. The line-number
join is gone. The badge's tooltip now says "reported against this construct"
rather than "in this module", which is what it always meant.

Selecting each of the three badged constructs:

| | Panel now says |
|---|---|
| `IdentityRetirement` | **Reported** — both lifecycle warnings |
| `Device` | **Reported** — `Field 'Device.is_last' is declared but not referenced` |
| `Group` | **Reported** — `Field 'Group.members' is declared but not referenced` |

A diagnostic the server could not attribute to any construct now has somewhere
to be read, which is F4's dialog.

### F4 — the count is the way in

The rail's footer is a control. It opens **What allium found**: every analysis
finding with its kind, its module, its summary, and a button to each construct
it names — plus any diagnostic no construct can carry.

Clicking `VetoDeletion` in the fifth conflict closes the dialog, switches to the
Flow view because that is one that draws rules, selects it, and the source strip
reads `messaging.allium:609`. Tomas gets from a finding to `file:line` in two
clicks.

The names the analyser reports are disambiguated by the finding's own module
first, because two modules can both declare a `Device`.

### F1 — the browser asks, once a second

`/api/health` carries a `revision` that moves whenever the answer changes —
a reload, or a reload starting or stopping failing. The client polls it and
re-fetches the graph when it moves, dropping its cached source with it.

A poll rather than a stream, deliberately: one request a second to a loopback
socket costs nothing, it carries the state of the spec in the same response, and
there is no connection to lose, reconnect or leave half-open behind a laptop lid.

Ines's test, run against a copy of `friend-mesh`:

| She does | It says |
|---|---|
| appends a rule that does not parse | amber banner: *The spec has 8 errors. This is what allium could still read of it.* |
| reverts the file | banner clears |
| appends a rule that does parse | `PersonaProbeRule` is in search within seconds, no reload |

The two failures are told apart because her next move differs. A **failed read**
means the graph is from before the edit and is not to be trusted — red, and it
says so. A spec that **carries errors** means the graph is current, because
Allium still describes a file it could not fully parse — amber, and it says that
too. A server that has gone away says so and keeps asking.

Warnings get no banner. They are the normal state of a spec under development,
and one that was up permanently would stop meaning anything.
