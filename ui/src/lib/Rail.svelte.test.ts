// @vitest-environment happy-dom

import { render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";

import Rail from "./Rail.svelte";

function rail(over: Partial<Record<string, unknown>> = {}) {
  render(Rail, {
    props: {
      mode: "domain",
      modules: [],
      hidden: new Set<string>(),
      traceMode: "off",
      hasSelection: true,
      traceIsEmpty: false,
      chainable: true,
      reports: 0,
      version: "allium 3.5.3",
      nodes: [],
      grouped: false,
      onmode: vi.fn(),
      onmodule: vi.fn(),
      ongrouping: vi.fn(),
      ontrace: vi.fn(),
      onfind: vi.fn(),
      onreports: vi.fn(),
      ...over,
    },
  });
  const button = (name: string) =>
    screen.getByRole("button", { name: new RegExp(name) }) as HTMLButtonElement;
  return { button, off: (name: string) => button(name).disabled };
}

describe("the trace controls", () => {
  it("offers all four once something is selected", () => {
    const { off } = rail();
    for (const name of ["All", "Follows", "Leads here", "Adjacent"]) {
      expect(off(name)).toBe(false);
    }
  });

  it("offers none but All with nothing selected", () => {
    const { off } = rail({ hasSelection: false });
    expect(off("All")).toBe(false);
    for (const name of ["Follows", "Leads here", "Adjacent"]) {
      expect(off(name)).toBe(true);
    }
  });

  // The domain view draws what the spec *has*: entities, values, enums. Every
  // causal edge has a rule, a trigger or a surface at one end, so none survive
  // that projection — and `Follows` and `Leads here` could never answer there,
  // whichever construct was picked. They looked available and were not.
  it("turns off the two that need a chain when the view holds none", () => {
    const { off } = rail({ chainable: false });
    expect(off("Follows")).toBe(true);
    expect(off("Leads here")).toBe(true);
    // `Adjacent` follows every edge kind, so it still answers — which is why it
    // kept working in the domain view while the other two could not.
    expect(off("Adjacent")).toBe(false);
    expect(off("All")).toBe(false);
  });

  it("says why, on the control itself", () => {
    const { button } = rail({ chainable: false });
    expect(button("Follows").getAttribute("title")).toMatch(/no chain here to follow/);
    expect(button("Adjacent").getAttribute("title")).toBeNull();
  });
});

describe("what it says when a trace found nothing", () => {
  // One sentence served all three directions, so asking what *leads here* was
  // answered with "nothing follows from this one" — the opposite question.
  it("answers the direction that was asked", () => {
    rail({ traceMode: "forward", traceIsEmpty: true });
    expect(screen.getByText(/Nothing follows from this one/)).toBeTruthy();
  });

  it("answers the other direction with the other sentence", () => {
    rail({ traceMode: "backward", traceIsEmpty: true });
    expect(screen.getByText(/Nothing leads to this one/)).toBeTruthy();
  });

  it("answers adjacency with its own", () => {
    rail({ traceMode: "near", traceIsEmpty: true });
    expect(screen.getByText(/Nothing is next to this one/)).toBeTruthy();
  });

  // And a view with no chain in it is a fact about the *view*, not about the
  // construct — which is what "nothing follows from this one" claimed.
  it("blames the view when the view is what has no chain", () => {
    rail({ traceMode: "forward", chainable: false });
    expect(screen.getByText(/no chain here to follow/)).toBeTruthy();
  });
});
