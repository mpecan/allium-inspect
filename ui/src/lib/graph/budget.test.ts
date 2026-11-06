import { describe, expect, it } from "vitest";

import { affordable, DRAW_BUDGET, overBudget, weight } from "./budget";

describe("the drawing budget", () => {
  it("counts nodes and edges alike", () => {
    expect(weight(3, 4)).toBe(7);
  });

  it("draws anything up to the budget without asking", () => {
    expect(affordable(DRAW_BUDGET)).toBe(true);
    expect(affordable(DRAW_BUDGET + 1)).toBe(false);
  });

  // A consent is for a size, so it covers a narrower set and not a wider one.
  it("lets a consent cover the size it was given for, and no more", () => {
    const agreed = DRAW_BUDGET * 3;
    expect(affordable(agreed, agreed)).toBe(true);
    expect(affordable(DRAW_BUDGET * 2, agreed)).toBe(true);
    expect(affordable(agreed + 1, agreed)).toBe(false);
  });

  it("never lowers the budget below the default", () => {
    expect(affordable(DRAW_BUDGET, 10)).toBe(true);
  });

  it("takes a budget of its own", () => {
    expect(affordable(11, 0, 10)).toBe(false);
    expect(affordable(10, 0, 10)).toBe(true);
  });

  it("says how large the view is, in words a reader can act on", () => {
    const words = overBudget(4000, 7992, 200);
    expect(words.headline).toBe("4,000 constructs and 7,992 relationships from 200 files.");
    expect(words.detail).toMatch(/modules view/);
    expect(overBudget(1200, 0, 1).headline).toBe(
      "1,200 constructs and 0 relationships from one file.",
    );
    expect(overBudget(1234567, 12, 2).headline).toContain("1,234,567 constructs");
  });
});
