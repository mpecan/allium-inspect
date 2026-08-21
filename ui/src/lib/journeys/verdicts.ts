// How a journey verdict is shown.
//
// Kept out of the component so it can be tested without mounting anything, and
// because the mapping is a decision rather than markup: which of the six
// verdicts share a glyph is the same question as which of them a reader should
// treat the same way, and the answer is "none of them".

import type { Verdict } from "../api/Verdict";

/** The glyph kind each verdict is drawn with. */
export const MARK = {
  specified: "true",
  undecided: "unknown",
  refused: "false",
  unspecified: "unspecified",
  unexposed: "unexposed",
  remark: "info",
} as const satisfies Record<Verdict, string>;

/**
 * What each verdict means, in the reader's terms rather than the tool's.
 *
 * The wording matters more here than anywhere else in the interface. Three of
 * these six are ways of not passing, and a reader who reads them all as
 * "failed" will go and change a specification that is not wrong.
 */
export const MEANING = {
  specified: "the spec does this",
  undecided: "this tool could not tell",
  refused: "the spec does something else",
  unspecified: "the spec does not have this yet",
  unexposed: "it happens, and nobody is shown it",
  remark: "worth a look",
} as const satisfies Record<Verdict, string>;

/** Whether this verdict is one a reader has to do something about. */
export function needsAttention(verdict: Verdict): boolean {
  return verdict !== "specified" && verdict !== "remark";
}

/**
 * The verdicts in a journey, worst first, with how many carry each.
 *
 * What the list badge shows. A journey with one unsupported step among nine is
 * not summarised by "nine steps", and it is not summarised by "failed" either
 * — the useful sentence is which kind of gap, and how much of it.
 */
export function tally(verdicts: Verdict[]): { verdict: Verdict; count: number }[] {
  const ORDER: Verdict[] = [
    "refused",
    "unspecified",
    "unexposed",
    "undecided",
    "remark",
    "specified",
  ];
  return ORDER.map((verdict) => ({
    verdict,
    count: verdicts.filter((each) => each === verdict).length,
  })).filter((entry) => entry.count > 0);
}

/** The worst verdict among `verdicts`, in the order a reader cares about them. */
export function worst(verdicts: Verdict[]): Verdict {
  return tally(verdicts)[0]?.verdict ?? "specified";
}
