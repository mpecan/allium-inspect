import { describe, expect, it } from "vitest";

import { midpoint, pathThrough, type Point } from "./route";

const p = (x: number, y: number): Point => ({ x, y });

describe("pathThrough", () => {
  it("draws a straight line between two points", () => {
    expect(pathThrough([p(0, 0), p(100, 0)])).toBe("M 0,0 L 100,0");
  });

  it("rounds a corner rather than turning square", () => {
    // A hairline right angle at a small zoom reads as a break in the line.
    const path = pathThrough([p(0, 0), p(100, 0), p(100, 100)], 10);
    expect(path).toBe("M 0,0 L 90,0 Q 100,0 100,10 L 100,100");
  });

  it("shrinks the radius to fit a short segment", () => {
    // Two bends ten pixels apart with a twenty-pixel radius would overshoot
    // each other and draw a knot where the line should be.
    const path = pathThrough([p(0, 0), p(10, 0), p(10, 10), p(10, 40)], 20);
    expect(path).not.toContain("NaN");
    // Each corner gives back at most half the segment it shares.
    expect(path).toContain("Q 10,0");
    expect(path).toContain("Q 10,10");
  });

  it("survives a repeated point without producing NaN", () => {
    // ELK emits a zero-length segment when a bend lands on an endpoint, and a
    // path with NaN in it renders as nothing at all.
    expect(pathThrough([p(0, 0), p(0, 0), p(50, 0)])).not.toContain("NaN");
  });

  it("draws nothing for a route with fewer than two points", () => {
    expect(pathThrough([])).toBe("");
    expect(pathThrough([p(3, 4)])).toBe("");
  });
});

describe("midpoint", () => {
  it("finds the middle of a straight run", () => {
    expect(midpoint([p(0, 0), p(100, 0)])).toEqual({ x: 50, y: 0 });
  });

  it("measures by length, not by how many bends there are", () => {
    // Three quick turns at one end must not drag the label down there — the
    // label belongs where the edge is easiest to read.
    const middle = midpoint([p(0, 0), p(200, 0), p(210, 0), p(210, 10), p(220, 10)]);
    expect(middle.y).toBe(0);
    expect(middle.x).toBeCloseTo(115, 0);
  });

  it("turns the corner when the halfway mark is past it", () => {
    expect(midpoint([p(0, 0), p(10, 0), p(10, 90)])).toEqual({ x: 10, y: 40 });
  });

  it("has an answer for a route with no length", () => {
    expect(midpoint([])).toEqual({ x: 0, y: 0 });
    expect(midpoint([p(7, 8)])).toEqual({ x: 7, y: 8 });
  });
});
