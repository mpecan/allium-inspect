import { describe, expect, it } from "vitest";

import type { Frame } from "../api/Frame";
import type { Resolution } from "../api/Resolution";
import type { StepEvidence } from "../api/StepEvidence";
import { MEANING, WORD, at, needsAttention, pictureUrl, summary, tally, worthShowing } from "./evidence";

function frame(image: string, said = "1. a step"): Frame {
  return {
    step: "R.1",
    image,
    caption: null,
    passed: true,
    taken_at: "2026-08-24T09:00:00Z",
    source: null,
    said,
  };
}

function evidence(standing: StepEvidence["standing"], frames: Frame[] = []): StepEvidence {
  return { standing, frames, claims: [], says_now: null };
}

function resolution(steps: Record<string, StepEvidence>): Resolution {
  return { steps, unknown: [] };
}

describe("evidence", () => {
  it("finds a step by the id a marker would spell", () => {
    const found = resolution({ "Reading.3": evidence("shown") });
    expect(at(found, "Reading", 3)?.standing).toBe("shown");
    expect(at(found, "Reading", 4)).toBeUndefined();
    expect(at(undefined, "Reading", 3)).toBeUndefined();
  });

  it("marks the three standings a reader must act on", () => {
    expect(needsAttention("failing")).toBe(true);
    expect(needsAttention("stale")).toBe(true);
    expect(needsAttention("claimed")).toBe(true);
    expect(needsAttention("shown")).toBe(false);
    // Most steps of most journeys are demand nobody has built yet.
    expect(needsAttention("unclaimed")).toBe(false);
  });

  it("gives every standing a word and a meaning", () => {
    for (const standing of ["shown", "failing", "stale", "claimed", "unclaimed"] as const) {
      expect(MEANING[standing].length).toBeGreaterThan(0);
      expect(WORD[standing].length).toBeGreaterThan(0);
    }
  });

  it("says nothing at all about a step nobody has shown", () => {
    expect(worthShowing(evidence("unclaimed"))).toBe(false);
    expect(worthShowing(undefined)).toBe(false);
    expect(worthShowing(evidence("claimed"))).toBe(true);
    expect(worthShowing(evidence("shown"))).toBe(true);
  });

  it("counts what has been shown out of what there is", () => {
    const found = resolution({
      "R.1": evidence("shown"),
      "R.2": evidence("stale"),
      "R.3": evidence("unclaimed"),
    });
    expect(tally(found)).toEqual({ shown: 1, total: 3 });
    expect(tally(undefined)).toEqual({ shown: 0, total: 0 });
  });

  it("summarises a journey worst first, and leaves out what nobody has shown", () => {
    const found = resolution({
      "R.1": evidence("shown"),
      "R.2": evidence("stale"),
      "R.3": evidence("unclaimed"),
      "R.4": evidence("failing"),
      "R.5": evidence("claimed"),
    });

    expect(summary(found, "R", [1, 2, 3, 4, 5])).toEqual([
      { standing: "failing", count: 1 },
      { standing: "stale", count: 1 },
      { standing: "claimed", count: 1 },
      { standing: "shown", count: 1 },
    ]);
  });

  it("summarises only the steps of the journey it was asked about", () => {
    const found = resolution({
      "R.1": evidence("shown"),
      "Other.1": evidence("failing"),
    });
    expect(summary(found, "R", [1])).toEqual([{ standing: "shown", count: 1 }]);
  });

  it("has nothing to summarise when nothing has been shown", () => {
    expect(summary(resolution({ "R.1": evidence("unclaimed") }), "R", [1])).toEqual([]);
    expect(summary(undefined, "R", [1])).toEqual([]);
  });

  it("asks the server for a picture by the name the manifest carries", () => {
    expect(pictureUrl(frame("03-browser.png"))).toBe("/api/evidence/03-browser.png");
  });

  /**
   * A harness derives a file name from a caption, and a caption is a sentence.
   * An unencoded space or slash would be a different path, or a broken one.
   */
  it("encodes a name a harness was free to choose", () => {
    expect(pictureUrl(frame("her empty list.png"))).toBe("/api/evidence/her%20empty%20list.png");
    expect(pictureUrl(frame("a/b.png"))).toBe("/api/evidence/a%2Fb.png");
  });
});
