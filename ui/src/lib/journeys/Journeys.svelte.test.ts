// @vitest-environment happy-dom

import { render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";

import type { JourneyReport } from "../api/JourneyReport";
import type { Outcome } from "../api/Outcome";
import type { Verdict } from "../api/Verdict";
import type { Walk } from "../api/Walk";
import Journeys from "./Journeys.svelte";

function line(verdict: Verdict, about: string, detail: string | null = null): Outcome {
  return { line: 4, verdict, about, detail };
}

function walk(name: string, outcomes: Outcome[], stipulated: string[] = []): Walk {
  return {
    name,
    goal: ["she borrows a copy and brings it back"],
    ends: ["the copy is back on the shelf"],
    line: 6,
    steps: [{ number: 1, title: "she borrows it", outcomes }],
    stipulated,
  };
}

function report(walks: Walk[], error: string | null = null): JourneyReport {
  return {
    files: [
      {
        path: "specs/journeys/lending.journey",
        name: "lending.journey",
        error,
        walks,
      },
    ],
    holding: walks.filter((w) =>
      w.steps.every((s) => s.outcomes.every((o) => o.verdict === "specified")),
    ).length,
    total: walks.length,
  };
}

describe("Journeys", () => {
  it("tells a reader how to load journeys when none are loaded", () => {
    // The state most people meet first, and a blank panel would read as "you
    // have none" rather than "you did not ask for any".
    render(Journeys, { report: report([]), failure: null });
    expect(screen.getByText(/--journeys/)).toBeTruthy();
  });

  it("counts what holds rather than what does not", () => {
    // A journey is the demand written first, so most of it not being supported
    // yet is the ordinary case. Leading with a failure count would frame
    // ordinary progress as a broken build.
    render(Journeys, { report: report([walk("A", [line("specified", "then x = 1")])]), failure: null });
    expect(screen.getByText(/1 of 1 hold end to end/)).toBeTruthy();
  });

  it("shows the journey's own goal and ending, not just its verdicts", () => {
    // A list of verdicts with the intent stripped off cannot be read against
    // anything — the reader's question is whether the spec delivers *this*.
    render(Journeys, { report: report([walk("A", [line("specified", "then x = 1")])]), failure: null });
    expect(screen.getByText(/she borrows a copy and brings it back/)).toBeTruthy();
    expect(screen.getByText(/the copy is back on the shelf/)).toBeTruthy();
  });

  it("shows every line the journey wrote, not only the ones that failed", () => {
    // The terminal report shows only failures because forty passing lines do
    // not fit. The browser has the room, and seeing what the journey actually
    // claimed is half of reading the verdict.
    render(Journeys, {
      report: report([
        walk("A", [line("specified", "then loan.status = open"), line("refused", "then x = 1", "x is 2")]),
      ]),
      failure: null,
    });
    expect(screen.getByText("then loan.status = open")).toBeTruthy();
    expect(screen.getByText("then x = 1")).toBeTruthy();
    expect(screen.getByText("x is 2")).toBeTruthy();
  });

  it("leads with what the journey was told rather than shown", () => {
    // The guardrail. An agent can make any journey pass; it cannot make one
    // pass invisibly.
    render(Journeys, {
      report: report([walk("A", [line("specified", "then x = 1")], ["ada.is_at_limit = false"])]),
      failure: null,
    });
    expect(screen.getByText(/Told rather than shown/)).toBeTruthy();
    expect(screen.getByText("ada.is_at_limit = false")).toBeTruthy();
  });

  it("says nothing about stipulations when a journey made none", () => {
    render(Journeys, { report: report([walk("A", [line("specified", "then x = 1")])]), failure: null });
    expect(screen.queryByText(/Told rather than shown/)).toBeNull();
  });

  it("names a file that would not parse instead of dropping it", () => {
    // A journey silently vanishing from the list reads as "it passed", which is
    // the worst answer available.
    render(Journeys, { report: report([], "line 12: expected `<name>: <Type>`"), failure: null });
    expect(screen.getByText(/Would not parse/)).toBeTruthy();
    expect(screen.getByText(/line 12/)).toBeTruthy();
  });

  it("says which verdicts a journey carries and how many of each", () => {
    render(Journeys, {
      report: report([
        walk("A", [
          line("specified", "then a = 1"),
          line("specified", "then b = 2"),
          line("unspecified", "ada does Nope on Desk", "no trigger called `Nope`"),
        ]),
      ]),
      failure: null,
    });
    expect(screen.getByText("the spec does not have this yet")).toBeTruthy();
    expect(screen.getByText("2")).toBeTruthy();
  });

  it("shows where the journey is, so a reader can go and edit it", () => {
    render(Journeys, { report: report([walk("A", [line("specified", "then x = 1")])]), failure: null });
    expect(screen.getByText("specs/journeys/lending.journey:6")).toBeTruthy();
  });

  it("reports its own failure without claiming there are no journeys", () => {
    render(Journeys, { report: null, failure: "Could not load the journeys." });
    expect(screen.getByText("Could not load the journeys.")).toBeTruthy();
    expect(screen.queryByText(/--journeys/)).toBeNull();
  });
});
