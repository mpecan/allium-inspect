// Which constructs belong in which view.
//
// A view is a filter over one graph, not a separate graph. The server sends
// everything once; switching views is a predicate, so it is instant and the
// selection survives the switch.
//
// Each view exists to answer one question, and its membership follows from
// that question rather than from what happens to look tidy:
//
//   domain     what the spec holds        the things, and how they relate
//   flow       what happens, in order     triggers, rules, and what they touch
//   lifecycle  how an entity changes      only entities that have a lifecycle
//   journey    what follows from an act   the boundary, and the chain from it
//
// An `external` node appears only in the domain view. Everywhere else it would
// be a dead end in the middle of a chain, and the chain is the point.

import type { Node } from "../api/Node";
import type { NodeKind } from "../api/NodeKind";
import type { ViewKind } from "../client";

/** The kinds each view draws, before any per-node condition. */
const MEMBERS: Record<ViewKind, readonly NodeKind[]> = {
  domain: ["entity", "value", "variant", "enum", "config", "external"],
  flow: ["rule", "trigger", "entity", "value", "variant"],
  lifecycle: ["entity"],
  journey: ["surface", "actor", "trigger", "rule", "entity"],
};

/** Whether `node` belongs in `view`. */
export function inView(node: Node, view: ViewKind): boolean {
  if (!MEMBERS[view].includes(node.kind)) {
    return false;
  }
  // The lifecycle view is about state machines, and an entity with no
  // transitions has none. Drawing it anyway fills the canvas with boxes that
  // have nothing to show and buries the three that do.
  if (view === "lifecycle") {
    return node.detail.type === "entity" && node.detail.transitions.length > 0;
  }
  return true;
}

/** What each view answers, for the rail. */
export const ANSWERS: Record<ViewKind, string> = {
  domain: "what the spec holds",
  flow: "what happens, and in what order",
  lifecycle: "how each entity changes state",
  journey: "what follows from an action",
};

/**
 * The nodes and edges `view` should draw, from a whole graph.
 *
 * Edges are filtered to those with both ends present. An edge to a node this
 * view excluded would render as an arrow into empty space.
 */
export function project<N extends Node, E extends { from: string; to: string }>(
  view: ViewKind,
  nodes: N[],
  edges: E[],
  hiddenModules: ReadonlySet<string> = new Set(),
): { nodes: N[]; edges: E[] } {
  const kept = nodes.filter(
    (node) => !hiddenModules.has(node.module) && inView(node, view),
  );
  const present = new Set(kept.map((node) => node.id));
  return {
    nodes: kept,
    edges: edges.filter((edge) => present.has(edge.from) && present.has(edge.to)),
  };
}
