// The id a file's boundary is drawn under.
//
// This module used to compute those boundaries itself, as bounding boxes around
// wherever ELK had put each module's constructs. That experiment answered its
// question and failed: the layered algorithm places by topology, topology
// crosses files freely, and the boxes came out nearly co-extensive on both a
// two-module fixture and a five-module set — five outlines around the same
// region tell a reader nothing.
//
// Grouping is a layout constraint now, in `layout.ts`, so the boundary is
// whatever ELK made the container. All that survives here is the name.

/** The id of a hull, in a namespace of its own. */
export function hullId(module: string): string {
  return `${module}::hull`;
}
