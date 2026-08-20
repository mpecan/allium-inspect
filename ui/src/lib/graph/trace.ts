// Following a chain through the graph.
//
// This is the closest an Allium spec gets to describing a user journey, and it
// is worth being precise about why it is an approximation rather than the real
// thing. Allium has no journey construct: it says what each rule does when its
// trigger happens, and nothing anywhere says "and then the person does this
// next". What it *does* say is which triggers a surface offers an actor, and
// which triggers each rule emits — and chaining those two facts reconstructs
// the path a system takes after somebody acts.
//
// So a trace is derived, not specified. It answers "what follows from this?"
// and not "what does a person actually do?", and the UI labels it that way.

import type { Edge } from "../api/Edge";
import type { EdgeKind } from "../api/EdgeKind";

/** A reachable set, and the edges that reached it. */
export interface Trace {
  /** Every node on the path, including the one it started from. */
  nodes: Set<string>;
  /** Every edge followed. */
  edges: Set<Edge>;
  /** How many steps out the furthest node is. */
  depth: number;
}

/** Which way to walk. */
export type Direction = "forward" | "backward" | "both";

/** The edge kinds that carry causation, in the order a chain runs. */
const CAUSAL: readonly EdgeKind[] = [
  "provides",
  "triggers",
  "creates",
  "mutates",
  "emits",
];

/**
 * Walk out from `start` along `kinds`, at most `depth` steps.
 *
 * Breadth-first and cycle-safe: a spec whose rules feed each other in a loop is
 * ordinary — a message is queued, delivery retries, the retry re-queues — and a
 * naive walk over one would not terminate.
 */
export function walk(
  edges: Edge[],
  start: string,
  options: {
    direction?: Direction;
    depth?: number;
    kinds?: readonly EdgeKind[];
  } = {},
): Trace {
  const { direction = "forward", depth = Infinity, kinds } = options;
  const allowed = kinds ? new Set(kinds) : null;

  const nodes = new Set<string>([start]);
  const followed = new Set<Edge>();
  let frontier = [start];
  let reached = 0;

  while (frontier.length > 0 && reached < depth) {
    const next: string[] = [];
    for (const edge of edges) {
      if (allowed && !allowed.has(edge.kind)) {
        continue;
      }
      const goesOut = direction !== "backward" && frontier.includes(edge.from);
      const comesIn = direction !== "forward" && frontier.includes(edge.to);
      if (!goesOut && !comesIn) {
        continue;
      }
      followed.add(edge);
      const arrival = goesOut ? edge.to : edge.from;
      if (!nodes.has(arrival)) {
        nodes.add(arrival);
        next.push(arrival);
      }
    }
    if (next.length === 0) {
      break;
    }
    frontier = next;
    reached += 1;
  }

  return { nodes, edges: followed, depth: reached };
}

/**
 * The causal chain that follows from `start`.
 *
 * Restricted to the edge kinds that carry causation, so a trace does not wander
 * sideways into "this entity has a field of that type" — true, and not part of
 * what happens next.
 */
export function journey(edges: Edge[], start: string, depth = 12): Trace {
  return walk(edges, start, { direction: "forward", depth, kinds: CAUSAL });
}

/** What had to happen for `target` to be reached. */
export function origins(edges: Edge[], target: string, depth = 12): Trace {
  return walk(edges, target, { direction: "backward", depth, kinds: CAUSAL });
}

/** Everything one step from `node`, in either direction. */
export function neighbourhood(edges: Edge[], node: string): Trace {
  return walk(edges, node, { direction: "both", depth: 1 });
}

/**
 * Whether a trace is worth drawing as a trace.
 *
 * A trace of one node is the selection itself. Dimming the whole canvas to
 * highlight a single box tells the reader nothing and costs them the context.
 */
export function isMeaningful(trace: Trace): boolean {
  return trace.nodes.size > 1;
}

/**
 * A view narrowed to what `trace` reached.
 *
 * Dimming answers "which of these three hundred?" by leaving all three hundred
 * on the canvas. This answers it by removing the rest, so the layout can run
 * again over what is left and the chain reads as a chain. It happens in the
 * pop-up rather than in place: a reader who was only looking something up did
 * not ask for the canvas they were reading to rearrange itself.
 *
 * Every edge *between* the reached nodes is kept, not only the ones the walk
 * followed. How the traced constructs relate to each other is part of the
 * answer, and dropping those edges would draw a chain simpler than the spec's.
 */
export function narrow<N extends { id: string }>(
  nodes: N[],
  edges: Edge[],
  trace: Trace | null,
): { nodes: N[]; edges: Edge[] } {
  if (trace === null) {
    return { nodes, edges };
  }
  const reached = trace.nodes;
  return {
    nodes: nodes.filter((node) => reached.has(node.id)),
    edges: edges.filter((edge) => reached.has(edge.from) && reached.has(edge.to)),
  };
}
