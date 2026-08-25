import type { Node as FlowNode } from "@xyflow/svelte";
import { describe, expect, it } from "vitest";

import { minimapColour } from "./minimap";

function construct(kind: string): FlowNode {
  return {
    id: "lending::entity::Loan",
    type: "construct",
    position: { x: 0, y: 0 },
    data: { node: { kind }, severity: null, dimmed: false },
  };
}

/** A hull as `Canvas.svelte` builds it: a module, and no construct anywhere. */
function hull(): FlowNode {
  return {
    id: "lending::hull",
    type: "hull",
    position: { x: 0, y: 0 },
    data: { module: "lending", held: 12, width: 400, height: 300 },
  };
}

describe("minimapColour", () => {
  it("paints a construct the colour of its family", () => {
    expect(minimapColour(construct("entity"))).toBe("var(--thing)");
    expect(minimapColour(construct("rule"))).toBe("var(--behaviour)");
  });

  // The bug this exists for. The minimap asks *every* node in the flow, and
  // since `Ring each file` arrived not every node is a construct — a hull has
  // no `node` in its data at all. Reading `.kind` off that threw, once when
  // ringing was switched on and then on every pan and zoom frame after,
  // because that is when a minimap asks.
  it("does not go looking for a construct inside a hull", () => {
    expect(() => minimapColour(hull())).not.toThrow();
  });

  it("paints a hull the plate it wears on the canvas", () => {
    expect(minimapColour(hull())).toBe("var(--ground-raised)");
  });
});
