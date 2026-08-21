import { describe, expect, it } from "vitest";

import type { Verdict } from "../api/Verdict";
import { MARK, MEANING, needsAttention, tally, worst } from "./verdicts";

const ALL: Verdict[] = [
  "specified",
  "undecided",
  "refused",
  "unspecified",
  "unexposed",
  "remark",
];

describe("MARK", () => {
  it("gives the three kinds of not-passing three different marks", () => {
    // The distinction this whole feature exists to draw. "The spec forbids it",
    // "the spec has never heard of it" and "this tool could not tell" are three
    // different pieces of work, and a reader who sees one glyph for all three
    // will go and change a specification that is not wrong.
    const marks = [MARK.refused, MARK.unspecified, MARK.undecided];
    expect(new Set(marks).size).toBe(3);
  });

  it("marks only the two gaps the same, because they are the same kind of gap", () => {
    expect(MARK.unspecified).not.toBe(MARK.unexposed);
  });

  it("has a mark and a meaning for every verdict the server can send", () => {
    // A verdict added to the Rust enum and not here renders as undefined, which
    // shows as a blank cell rather than as an error.
    for (const verdict of ALL) {
      expect(MARK[verdict]).toBeTruthy();
      expect(MEANING[verdict].length).toBeGreaterThan(8);
    }
  });

  it("says what each verdict means without using the word failed", () => {
    // A journey is written before the spec it demands, so most of these are
    // ordinary states of work in progress. Wording them as failures would make
    // the view read as a broken test suite.
    for (const verdict of ALL) {
      expect(MEANING[verdict]).not.toMatch(/fail|error|wrong/i);
    }
  });
});

describe("needsAttention", () => {
  it("leaves a holding line and a remark alone", () => {
    expect(needsAttention("specified")).toBe(false);
    expect(needsAttention("remark")).toBe(false);
  });

  it("flags every verdict a reader has to do something about", () => {
    for (const verdict of ["refused", "unspecified", "unexposed", "undecided"] as const) {
      expect(needsAttention(verdict)).toBe(true);
    }
  });
});

describe("tally", () => {
  it("counts each verdict and drops the ones nothing carries", () => {
    const counted = tally(["specified", "specified", "refused"]);
    expect(counted).toEqual([
      { verdict: "refused", count: 1 },
      { verdict: "specified", count: 2 },
    ]);
  });

  it("puts what the spec forbids before what it never said", () => {
    // Both fail and a reader can only act on one at a time. A refusal is a
    // disagreement about behaviour that is specified; the rest are gaps.
    const counted = tally(["specified", "undecided", "unspecified", "refused"]);
    expect(counted.map((entry) => entry.verdict)).toEqual([
      "refused",
      "unspecified",
      "undecided",
      "specified",
    ]);
  });

  it("counts nothing as nothing", () => {
    expect(tally([])).toEqual([]);
  });
});

describe("worst", () => {
  it("reports the worst verdict in the list, whichever end it is at", () => {
    expect(worst(["specified", "refused"])).toBe("refused");
    expect(worst(["refused", "specified"])).toBe("refused");
    expect(worst(["specified", "undecided", "specified"])).toBe("undecided");
  });

  it("ranks every pair the same way round either way round", () => {
    const order: Verdict[] = [
      "refused",
      "unspecified",
      "unexposed",
      "undecided",
      "remark",
      "specified",
    ];
    for (const [at, worse] of order.entries()) {
      for (const better of order.slice(at + 1)) {
        expect(worst([better, worse])).toBe(worse);
        expect(worst([worse, better])).toBe(worse);
      }
    }
  });

  it("calls an empty journey specified rather than inventing a complaint", () => {
    expect(worst([])).toBe("specified");
  });
});
