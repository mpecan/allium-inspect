# Tomas Berg — the reviewer

> "Before this gets built, tell me where it is wrong."

Tomas is asked to review a spec set before the team commits to it. He has done
this for other teams and does not know this domain, which is the point: he is
looking for the defects that are visible in the structure rather than in the
business logic. Rules that can both fire on the same entity. A transition
nothing witnesses. A module that reaches into another one thirty times. A
reference that does not resolve.

He has a few hours, not a few days. He will use the tool, and he will also pipe
`--print-graph` into `jq`, because a claim he can count is one he can put in a
review and defend.

## What he comes for

1. **The analyser's findings, first.** `allium analyse` reports reachability,
   deadlock, conflict and data-flow problems. This spec set has five conflicts —
   pairs of rules that can both fire on one entity. That is the single highest
   value thing in the tool for him, and it is the first thing he looks for.
2. **The shape of the whole.** Five modules, 353 constructs, 526 edges, 55 of
   them crossing a module boundary. He wants the coupling, and he wants to be
   able to switch a module off and see what is left standing.
3. **What did not resolve.** A reference the linker could not place is either a
   typo or a missing dependency, and both belong in the review.
4. **Numbers he can quote.** "Seventeen rules have no `requires` at all" is a
   review comment. "It felt under-constrained" is not.

## What he knows, and what he does not

He knows Allium and he knows what a specification review is for. He has strong
opinions about coupling and about specs that describe a happy path only.

He does not know this domain, so he cannot tell a wrong rule from a right one.
Everything he finds has to be structural, which means the tool showing him
structure honestly is the whole of his experience.

He does not trust a tool he cannot check. Anything the tool derived, he wants to
be able to derive himself from the JSON, or he will leave it out of the review.

## What good looks like

- The findings are a place he can go, not a number he is told.
- Every count in the interface is a count he can reproduce from `--print-graph`.
- Cross-module edges are visible as such, rather than being the same grey as
  everything else.
- He can leave with a list, and each item on it names a construct and a file.

## What would make him stop

- **The analyser's output is a footnote.** If the tool ingests `analyse` and
  then shows him a count, it has buried its most valuable input, and he will run
  the CLI himself and not come back.
- **It presents derived structure as specified structure.** A journey trace is a
  reconstruction; if it is drawn identically to a declared relationship, he
  cannot review either.
- **He cannot get a construct's location.** A finding he cannot turn into
  `file:line` is a finding he cannot file.

## The checks he settles

- From a cold start, how do you get to the five analysis findings? Count the
  gestures. Is the count in the rail a control?
- Switch off four of the five modules. Is what remains coherent, and is it clear
  what has been hidden rather than what does not exist?
- Find a reference that did not resolve. Does the tool distinguish "not declared
  here" from "does not exist"?
- Take any number the interface states and reproduce it from
  `--print-graph | jq`. Do they agree?
- Find the five conflicting rule pairs. Can you get from each to both rules and
  both source locations?
