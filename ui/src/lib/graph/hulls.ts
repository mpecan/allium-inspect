// Where each file's constructs ended up, as a box around them.
//
// Deliberately drawn *after* the layout rather than fed into it. ELK places by
// topology; asking it to also contain each module would change every position
// on the canvas, and the first thing worth knowing is whether the picture
// already groups by file on its own. If it does, a boundary costs nothing and
// tells the truth. If it does not, the boxes overlap — and that overlap is the
// finding, not a bug in the drawing.
//
// So this is an experiment with an honest failure mode: it cannot make the
// layout worse, because it does not touch it.

import type { Node } from "../api/Node";
import type { PlacedNode } from "./layout";

/** A file's constructs, boxed. */
export interface Hull {
  module: string;
  x: number;
  y: number;
  width: number;
  height: number;
  /** How many of this module's constructs the box encloses. */
  held: number;
}

/** Room left between the outermost construct and the boundary. */
const PADDING = 22;

/** Room above the box for its name, so the label sits clear of the line. */
const LABEL = 16;

/**
 * One box per module, around wherever that module's constructs landed.
 *
 * Synthesised module nodes are left out. A destination box already *is* a
 * file, and ringing it with a boundary of one would say a module contains
 * itself.
 *
 * A module with a single construct gets no box either: a boundary around one
 * thing draws a line that adds nothing to the thing already drawn.
 */
export function hulls(nodes: Node[], placed: PlacedNode[]): Hull[] {
  const home = new Map(nodes.map((node) => [node.id, node]));
  const boxes = new Map<string, Hull>();

  for (const place of placed) {
    const node = home.get(place.id);
    if (!node || node.id.endsWith("::module")) {
      continue;
    }
    const found = boxes.get(node.module);
    if (!found) {
      boxes.set(node.module, {
        module: node.module,
        x: place.x,
        y: place.y,
        width: place.width,
        height: place.height,
        held: 1,
      });
      continue;
    }
    const right = Math.max(found.x + found.width, place.x + place.width);
    const bottom = Math.max(found.y + found.height, place.y + place.height);
    found.x = Math.min(found.x, place.x);
    found.y = Math.min(found.y, place.y);
    found.width = right - found.x;
    found.height = bottom - found.y;
    found.held += 1;
  }

  return [...boxes.values()]
    .filter((box) => box.held > 1)
    .map((box) => ({
      ...box,
      x: box.x - PADDING,
      y: box.y - PADDING - LABEL,
      width: box.width + PADDING * 2,
      height: box.height + PADDING * 2 + LABEL,
    }))
    // Biggest first, so a box that happens to enclose another still leaves the
    // smaller one clickable and its label readable on top.
    .sort((a, b) => b.width * b.height - a.width * a.height);
}

/** The id of a hull, in a namespace of its own. */
export function hullId(module: string): string {
  return `${module}::hull`;
}
