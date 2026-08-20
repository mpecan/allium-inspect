// Turning laid-out nodes into the array Svelte Flow owns.
//
// Svelte Flow does not just read that array — it writes back to it, recording
// the box it measured each node to be. Those measurements are what edge
// routing is computed from: a node whose `measured` is missing has its handle
// bounds reset and is re-measured, and until that lands there is nowhere for an
// edge to attach, so the edge is not drawn.
//
// So repainting merges. The appearance comes from this side; the measurements
// are carried over from what the canvas last wrote. That is why this is a
// function rather than three lines inside the component, and why it is worth a
// test: getting it wrong does not break the nodes, it silently empties the
// graph of its edges.

import type { Node as FlowNode } from "@xyflow/svelte";

import type { Trace } from "./trace";

/**
 * `placed` repainted for the current selection and trace.
 *
 * `previous` is the array Svelte Flow has been writing to.
 */
export function paint(
  placed: FlowNode[],
  previous: FlowNode[],
  selected: string | null,
  trace: Trace | null,
): FlowNode[] {
  const measured = new Map(
    previous.flatMap((node) => (node.measured ? [[node.id, node] as const] : [])),
  );
  return placed.map((node) => {
    const carried = measured.get(node.id);
    return {
      ...node,
      measured: carried?.measured,
      width: carried?.width,
      height: carried?.height,
      selected: node.id === selected,
      data: {
        ...node.data,
        dimmed: trace !== null && !trace.nodes.has(node.id),
      },
    };
  });
}
