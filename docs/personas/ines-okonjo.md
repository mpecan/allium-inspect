# Ines Okonjo — the spec author

> "I have just written twenty lines. Tell me what I did."

Ines maintains the specification for a mesh messaging product: five modules,
about 6,700 lines, cross-referenced (`membership/Group`, `identity/Identity`).
She has been writing Allium for eight months and reads it faster than she reads
the code it describes. Her editor is open on `delivery.allium` all day, and
`allium check` runs on save.

She is the person the language was designed for, and she is the one most likely
to catch the tool being wrong, because she already knows the answer to most of
what it shows her. That makes her the hardest reader to satisfy and the most
useful one to satisfy first.

## What she comes for

She does not come to browse. She comes with a specific thing she just changed
and a specific worry about it.

1. **She added a rule and wants to see it join the chain.** `QueueOnSend`
   consumes `MessageSent` from another module. The CLI will tell her it parses;
   it will not tell her the chain now runs surface → trigger → rule → entity →
   trigger across a module boundary. That is a picture, and it is why she opens
   a browser at all.
2. **She renamed a field and wants to know what broke.** Twelve rules read
   `Message.status`. Grep finds the string; it does not find the three rules
   whose `requires` now compares an enum against a state that no longer exists.
3. **She wants the reachability answer she cannot get by reading.** Is this rule
   dead? Can this transition ever fire? `allium analyse` knows. She wants it
   attached to the construct rather than in a JSON blob.
4. **She wants to check a suspicion in seconds.** "If a member is at their loan
   limit, does `BorrowCopy` actually refuse, or did I write the comparison
   backwards?" That is the simulator, and it is worth opening only if it takes
   less time than reasoning it out.

## What she knows, and what she does not

She knows the language, the module layout, and her own naming conventions. She
knows what `ensures` means and does not need it explained.

She does not know what this tool derived versus what her spec states. That
distinction is the whole trust relationship: a trace is reconstructed from
`provides` and `emits`, not specified, and if the tool ever presents a
derivation as a fact she will stop believing the rest of it.

She also does not know, and should not have to work out, why an expression could
not be evaluated. "Undecided" without a reason is indistinguishable from a bug
in the tool.

## What good looks like

- She saves the file and the browser is correct within a second, without
  touching it.
- A rule she just wrote is one search away, not one pan-and-zoom away.
- What the spec says and what the tool worked out are visibly different things.
- The text she reads back is the text she wrote, at the byte offsets the parser
  reported.

## What would make her stop

- **It shows her a stale graph.** She saves a broken file, the tool keeps
  serving the last good one and says nothing, and she spends ten minutes reading
  a picture of a spec that no longer exists.
- **It normalises her prose.** She wrote a clause across three lines with a
  comment in the middle. A reconstructed one-line version of her own rule is a
  small lie, and she will notice it before she notices anything else.
- **It guesses.** A precondition reported as blocking when nothing evaluated it
  is worse than no simulator.

## The checks she settles

- Edit a spec while the tool is open. Does the browser follow, and how fast?
- Break a spec — remove a closing brace — and save. What does it say, and does
  it still show the last good graph, and is it clear which of the two you are
  looking at?
- Add a rule that consumes a trigger emitted in another module. Does the chain
  join up in the Flow view without a reload?
- Select any clause. Is the text byte-identical to the file, including a clause
  that wraps and a spec containing a non-ASCII character?
- Fire a rule whose precondition reads a derived value. Does it say what it
  could not decide and why, or does it pick a side?
