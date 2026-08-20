# Personas

Five people who would open `allium-inspect`, written down so that a review has
something to be a review *of*.

They are not marketing personas and they are not composites of survey data. Each
one is a job that the tool either does or does not do, made concrete enough to
argue with: a named person, a real spec set, a specific reason for opening a
browser instead of staying in their editor, and a specific reason they would
stop.

## Why they are here

A surface review without a reader is a list of opinions. "The rail is cluttered"
and "the rail is thorough" are the same observation from two people with
different jobs, and the only way to settle it is to say whose job. These five
say whose.

They are also a scope boundary. Allium has agents for distilling a spec out of
code, generating tests from it, and finding where code and spec have diverged.
This tool does none of that, and the personas are drawn so that the boundary is
visible: Priya runs `allium propagate` in a terminal and comes here for the
obligations it will hold her to; she does not expect this tool to write her
tests.

## The five

| | Comes for | Would leave because |
|---|---|---|
| [Ines Okonjo](ines-okonjo.md), spec author | did my edit land, and what did it break | it lied about the state of her file |
| [Dan Reyes](dan-reyes.md), domain lead | does this match how the business works | it spoke to him in a language he does not write |
| [Priya Raman](priya-raman.md), implementer | what exactly am I on the hook for | she could not get from a name to its source |
| [Tomas Berg](tomas-berg.md), reviewer | where is this spec set wrong | it hid what the analyser found |
| [Aoife Nwosu](aoife-nwosu.md), new joiner | what is all this | it assumed she knew the vocabulary |

## Using them

Pick one, do their tasks in order, and write down what actually happened rather
than what should have. Each file ends with the checks that persona settles —
questions only they are placed to ask.

Two rules keep this honest:

- **Drive the real tool over a real spec set.** `just run ../friend-mesh/specs/`
  is five modules and 6,700 lines. A tool that works on the two-file fixture and
  falls over at that size has not been reviewed.
- **Record the evidence, not the verdict.** "Findings are hard to reach" is an
  opinion. "The rail says `5 analysis findings` and the text is not a control"
  is a fact, and it survives someone disagreeing with the opinion.

Reviews live in [`../reviews/`](../reviews/).
