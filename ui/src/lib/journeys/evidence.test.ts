import { describe, expect, it } from "vitest";

import type { Frame } from "../api/Frame";
import type { Resolution } from "../api/Resolution";
import type { StepEvidence } from "../api/StepEvidence";
import {
  MEANING,
  WORD,
  at,
  axes,
  matches,
  narrow,
  needsAttention,
  pictureUrl,
  summary,
  tally,
  worthShowing,
} from "./evidence";

function frame(
  image: string,
  said = "1. a step",
  tags: Record<string, string> = {},
): Frame {
  return {
    step: "R.1",
    image,
    caption: null,
    passed: true,
    taken_at: "2026-08-24T09:00:00Z",
    source: null,
    said,
    tags,
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

  describe("narrowing", () => {
    const dark = frame("01-dark.png", "1. a step", { theme: "dark", platform: "macos" });
    const light = frame("01-light.png", "1. a step", { theme: "light", platform: "macos" });
    const untagged = frame("01.png");

    const found = resolution({
      "R.1": evidence("shown", [dark, light]),
      "R.2": evidence("shown", [untagged]),
    });

    it("finds a question for every tag key, and every answer given to it", () => {
      expect(axes(found)).toEqual([
        { key: "platform", values: ["macos"] },
        { key: "theme", values: ["dark", "light"] },
      ]);
    });

    it("has nothing to offer when nothing is tagged", () => {
      expect(axes(resolution({ "R.1": evidence("shown", [untagged]) }))).toEqual([]);
      expect(axes(undefined)).toEqual([]);
    });

    it("keeps one entry per value however many pictures carry it", () => {
      const many = resolution({
        "R.1": evidence("shown", [dark, light]),
        "R.2": evidence("shown", [
          frame("02-dark.png", "2. a step", { theme: "dark" }),
          frame("02-light.png", "2. a step", { theme: "light" }),
        ]),
      });
      expect(axes(many).find((axis) => axis.key === "theme")?.values).toEqual(["dark", "light"]);
    });

    it("shows only what answers to the question as asked", () => {
      expect(matches(dark, { theme: "dark" })).toBe(true);
      expect(matches(light, { theme: "dark" })).toBe(false);
    });

    it("narrows on every axis at once", () => {
      expect(matches(dark, { theme: "dark", platform: "macos" })).toBe(true);
      expect(matches(dark, { theme: "dark", platform: "ios" })).toBe(false);
    });

    /**
     * Silence is not disagreement. A harness that never said which platform it
     * photographed has not thereby said it photographed the other one, and
     * hiding its picture would lose evidence to a question nobody asked of it.
     */
    it("keeps a picture that says nothing on the axis being narrowed", () => {
      expect(matches(untagged, { theme: "dark" })).toBe(true);
      expect(matches(untagged, { theme: "light" })).toBe(true);
    });

    it("shows everything when nothing has been narrowed", () => {
      expect(narrow(evidence("shown", [dark, light]), {})).toEqual([dark, light]);
    });

    it("narrows a step down to the one picture asked for", () => {
      expect(narrow(evidence("shown", [dark, light]), { theme: "light" })).toEqual([light]);
    });

    /** The case the panel has to say something about rather than render blank. */
    it("can narrow a photographed step down to nothing", () => {
      const onlyDark = evidence("shown", [dark]);
      expect(narrow(onlyDark, { theme: "light" })).toEqual([]);
      expect(onlyDark.frames.length).toBeGreaterThan(0);
    });
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
