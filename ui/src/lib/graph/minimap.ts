// What the minimap paints each node.
//
// A function of its own rather than a closure in the canvas, because it is the
// one place that has to know **every** kind of node the flow holds — and since
// `Ring each file` arrived that is two kinds, not one. It read `data.node.kind`
// off whatever it was handed; a hull carries a module and has no `node` at all,
// so it threw. Once when ringing was switched on, once per relayout after, and
// then again on every pan and every zoom frame, because that is when a minimap
// asks. A canvas that stops responding while you drag it is what that looks
// like from the outside.
//
// `node.type` is the discriminant Svelte Flow already keeps, so the shape of
// `data` is read rather than assumed.

import type { Node as FlowNode } from "@xyflow/svelte";

import type { Node } from "../api/Node";
import { familyOf } from "./layout";

/** The colour the minimap draws `node` in. */
export function minimapColour(node: FlowNode): string {
  // A file's plate, the same one it wears on the canvas. Hulls come first in
  // the array, so the minimap paints them under the constructs and the
  // miniature says what the canvas says.
  if (node.type === "hull") {
    return "var(--ground-raised)";
  }
  return `var(--${familyOf((node.data as { node: Node }).node.kind)})`;
}
