# Priya Raman — the implementer

> "The spec says this exists. What am I on the hook for?"

Priya is building the delivery module against a spec she did not write. Her job
is to make the code satisfy it and to prove that it does. She runs
`allium propagate` to generate the test scaffolding, and the obligations it
produces — 712 of them across this spec set, in 22 categories — are her actual
backlog.

She is the persona with the most concrete relationship to the tool: she is
looking up facts, one at a time, dozens of times a day, in the middle of writing
code. Every second between "I need to know the type of `expires_at`" and knowing
it is a second she is not writing code.

## What she comes for

1. **Field-level truth.** What fields does `OutboxEntry` have, what are their
   types, which are derived, which is the identity? She needs the answer to be
   exact, because she is about to write it into a struct.
2. **Her obligations for one construct.** Not all 712 — the ones this entity or
   this rule owes, so she can check them off. "Verify transition queued →
   settled on `OutboxEntry.status` is reachable via a witnessing rule" is a test
   she has to write.
3. **The source, immediately.** She has a name and needs the lines. She would
   otherwise `rg 'entity OutboxEntry'` and open the file; the tool is only worth
   using if it is faster than that, and it must land on the right line.
4. **What touches this.** Before changing how a field is written, she needs the
   rules that write it and the invariants that constrain it. That set spans
   modules and does not appear in any one view.

## What she knows, and what she does not

She knows the codebase and her own module. She reads Allium well enough to work
from it, and she knows what an obligation is because a generator hands her one.

She does not know the other four modules. A reference to `membership/Group` is
a name, not a thing, and she needs to get from the name to the definition
without knowing which file it is in.

She does not know which of the 22 obligation categories matter for what she is
doing right now, and a flat list of 712 is not a backlog, it is a wall.

## What good looks like

- Name in, definition out, in one gesture, wherever it lives.
- The obligations attached to the thing they are about, not in a separate report.
- Types shown as the spec writes them — `Set<identity/Device>`, not `Set`.
- She can get from a field to every rule that writes it without reading all 112
  rules.

## What would make her stop

- **It is slower than `rg`.** That is the whole bar. If she has to pan a canvas
  to find a box she already knows the name of, she will use the editor.
- **It shows her a truncated type and makes her open the file anyway.** Then it
  has cost her a context switch and given her nothing.
- **The obligations are only a number.** A count she cannot act on is a reminder
  that she has work she cannot see.

## The checks she settles

- Time it: from a cold canvas, how many gestures from knowing the name
  `OutboxEntry` to reading its fields? To reading its source?
- Pick a field whose type is a qualified generic. Is the type readable in full
  anywhere in the tool, or only truncated?
- Select an entity. Are its obligations there, are they legible, and is it
  obvious which are about it rather than about its module?
- Take a field and find every rule that writes it. What does that cost?
- Follow a cross-module reference from a field to its definition. Does the tool
  take you, and does it bring the module back if it was switched off?
