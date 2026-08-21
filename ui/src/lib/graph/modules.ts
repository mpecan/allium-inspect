// What a file is, read from the outside.
//
// Allium has no `pub`. A module's interface is therefore not something the spec
// declares — it is whatever the other files reached for, and that list is only
// knowable by looking at every edge in the set. Which makes it exactly the kind
// of thing this tool should compute and the author should not have to.
//
// Kept apart from `views.ts` because that module answers "what does this view
// draw"; this one answers "what is true about this file", and the panel wants
// the second without drawing anything.

import type { Edge } from "../api/Edge";
import type { Node } from "../api/Node";

/** One construct other files reached for, and who reached. */
export interface Exported {
  node: Node;
  /** The modules that reference it, in name order. */
  from: string[];
  /** How many references arrive, across all of them. */
  count: number;
}

/** One neighbouring file, and how much passes between the two. */
export interface Neighbour {
  module: string;
  /** References from this module into the neighbour. */
  out: number;
  /** References from the neighbour into this module. */
  into: number;
}

/** Everything the panel says about one file. */
export interface ModuleReport {
  module: string;
  /** How many constructs the file declares. */
  held: number;
  /**
   * The constructs other files reach for, most-referenced first.
   *
   * The file's public surface, whether or not anybody wrote it down. A short
   * list against a large `held` is a file that keeps its business to itself;
   * a long one is a file whose every part is somebody else's business too.
   */
  exported: Exported[];
  /** Who this file touches and who touches it, in name order. */
  neighbours: Neighbour[];
}

/**
 * Read `module` from the outside, using every edge in the set.
 *
 * Deliberately not filtered by the current view or by what the rail has
 * switched off: this is a fact about the specification rather than about what
 * is on screen, and a panel that changed its answer when you hid a module
 * would be reporting the interface as smaller than it is.
 */
export function reportOn(module: string, nodes: Node[], edges: Edge[]): ModuleReport {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const held = nodes.filter((node) => node.module === module);

  const arrivals = new Map<string, Map<string, number>>();
  const neighbours = new Map<string, Neighbour>();
  const touch = (name: string): Neighbour => {
    const found = neighbours.get(name) ?? { module: name, out: 0, into: 0 };
    neighbours.set(name, found);
    return found;
  };

  for (const edge of edges) {
    const from = byId.get(edge.from);
    const to = byId.get(edge.to);
    if (!from || !to || from.module === to.module) {
      continue;
    }
    if (from.module === module) {
      touch(to.module).out += 1;
    }
    if (to.module === module) {
      touch(from.module).into += 1;
      const who = arrivals.get(to.id) ?? new Map<string, number>();
      who.set(from.module, (who.get(from.module) ?? 0) + 1);
      arrivals.set(to.id, who);
    }
  }

  const exported: Exported[] = [...arrivals.entries()]
    .flatMap(([id, who]) => {
      const node = byId.get(id);
      if (!node) {
        return [];
      }
      const count = [...who.values()].reduce((sum, n) => sum + n, 0);
      return [{ node, from: [...who.keys()].sort(), count }];
    })
    // Most reached-for first: that is the part of the file hardest to change,
    // and the reason to read this list at all.
    .sort((a, b) => b.count - a.count || (a.node.name < b.node.name ? -1 : 1));

  return {
    module,
    held: held.length,
    exported,
    neighbours: [...neighbours.values()].sort((a, b) =>
      a.module < b.module ? -1 : a.module > b.module ? 1 : 0,
    ),
  };
}
