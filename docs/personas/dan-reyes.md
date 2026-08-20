# Dan Reyes — the domain lead

> "Show me what happens when someone deletes a message. Not the code — what
> happens."

Dan runs the product side of the same mesh messaging team. He decided that a
deletion should be vetoable for twenty-four hours, that a device removed from a
group keeps nothing, and that a hub may hold a message it cannot read. Those
decisions are now written in Allium by someone else, and he is the only person
who can say whether what got written is what he meant.

He does not write Allium and will not learn it. He can read it slowly — it was
designed to be readable — but he reads `requires entry.status = queued` as a
sentence, not as an expression, and he will not notice that the comparison is
against a state that no longer exists.

He almost always sees this tool on someone else's screen, in a meeting, being
driven by Ines. That matters more than it sounds: he never chooses what to click.

## What he comes for

1. **Walking a scenario he already has in his head.** "Someone posts, someone
   else deletes it within the window, a third person vetoes." He wants to see
   the system do that, step by step, and either recognise it or not.
2. **Checking a boundary.** "What can a member actually do?" — the set of
   operations a surface offers an actor is a decision he made, and it is the one
   part of the spec he can audit directly.
3. **Finding out what happens next.** Given an action, what follows from it? He
   thinks in journeys. Allium has no journey construct, which means the honest
   answer to his question is derived, and he needs to be told that in words he
   would use.
4. **Sanity, at a glance.** Are there five things called almost the same thing?
   Is there a rule nobody can ever trigger? He will not go looking, but if the
   tool puts it in front of him he will ask about it, and that question is worth
   more than an hour of review.

## What he knows, and what he does not

He knows the domain completely. He will spot a wrong rule faster than anyone if
he can read it.

He does not know what an invariant is, in those words. He does not know what
"entity", "surface", "trigger" or "actor" mean *in Allium* as opposed to in
ordinary English, and the two are close enough to be dangerous — he will assume
"surface" means a screen.

He does not know what a state that "could not be checked" means. Told that an
invariant could not be checked, he will hear "the invariant failed" or "the tool
is broken", and both are wrong.

## What good looks like

- Every label is a phrase he would say out loud. The tool explains itself in the
  first five words, every time.
- When something is derived rather than stated, it says so, in those words.
- The simulator's answer is a sentence, not a verdict code.
- He can follow it while someone else drives, which means nothing important is
  behind a hover.

## What would make him stop

- **Jargon he has to ask about twice.** He will not ask a third time; he will
  stop attending the meeting and go back to reading a document.
- **A wall of everything.** Three hundred boxes is not a picture of his product,
  it is a picture of the tool's ambition.
- **A confident wrong answer.** If the simulator says a rule fired and it should
  not have, he will trust the spec less, not the tool less, and that is the most
  expensive failure this tool can have.

## The checks he settles

- Read every label, every heading and every empty state out loud. Which need a
  glossary? Which use an Allium term as if it were an English one?
- Open the simulator cold. Without knowing the spec, can you fire anything at
  all, and does the screen say what to do first?
- Fire something with a precondition the tool cannot decide. Is the result
  distinguishable from a failure by a person who does not know the difference?
- Pick a surface. Can you say, in English, who may do what, without opening the
  source?
- Trace forward from an action. Does the tool say anywhere that this is derived
  rather than specified — in words Dan would use, not "reconstructed from
  `provides` and `emits`"?
