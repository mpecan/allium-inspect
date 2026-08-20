import type { Node as FlowNode } from "@xyflow/svelte";
import { describe, expect, it } from "vitest";

import { paint } from "./paint";
import type { Trace } from "./trace";

function placed(id: string): FlowNode {
  return {
    id,
    type: "construct",
    position: { x: 0, y: 0 },
    data: { node: { id }, severity: null, dimmed: false },
  };
}

/** A node as Svelte Flow leaves it once it has measured the box. */
function canvasWrote(id: string): FlowNode {
  return { ...placed(id), measured: { width: 120, height: 40 }, width: 120, height: 40 };
}

const trace: Trace = { nodes: new Set(["a"]), edges: new Set(), depth: 1 };

describe("paint", () => {
  it("carries the canvas's measurements across a repaint", () => {
    // Without this the edges vanish on every click: a node with no `measured`
    // has its handle bounds reset, and an edge with no handle to attach to is
    // not drawn at all.
    const [node] = paint([placed("a")], [canvasWrote("a")], null, null);
    expect(node?.measured).toEqual({ width: 120, height: 40 });
    expect(node?.width).toBe(120);
  });

  it("leaves a node the canvas has not measured yet unmeasured", () => {
    // Inventing a size here would tell Svelte Flow the node is ready and stop
    // it ever measuring the real one.
    const [node] = paint([placed("a")], [], null, null);
    expect(node?.measured).toBeUndefined();
  });

  it("does not carry a measurement onto a different node", () => {
    const [node] = paint([placed("b")], [canvasWrote("a")], null, null);
    expect(node?.measured).toBeUndefined();
  });

  it("takes the position from the layout, not from the canvas", () => {
    // Layout is the authority on where a node goes; the canvas is the
    // authority on how big it is. Mixing those up makes a relayout a no-op.
    const moved = { ...placed("a"), position: { x: 40, y: 90 } };
    const [node] = paint([moved], [canvasWrote("a")], null, null);
    expect(node?.position).toEqual({ x: 40, y: 90 });
  });

  it("marks the selected node and only that one", () => {
    const painted = paint([placed("a"), placed("b")], [], "b", null);
    expect(painted.map((node) => node.selected)).toEqual([false, true]);
  });

  it("dims everything a trace does not reach", () => {
    const painted = paint([placed("a"), placed("b")], [], null, trace);
    expect(painted.map((node) => node.data.dimmed)).toEqual([false, true]);
  });

  it("dims nothing when no trace is running", () => {
    const painted = paint([placed("a"), placed("b")], [], null, null);
    expect(painted.every((node) => node.data.dimmed === false)).toBe(true);
  });
});
