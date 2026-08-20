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

import type { Edge } from "../api/Edge";
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
  //
  // `project` expands those entities into their states rather than filtering
  // to them; this predicate answers the narrower question of which entities
  // have a machine at all.
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
export function project(
  view: ViewKind,
  nodes: Node[],
  edges: Edge[],
  hiddenModules: ReadonlySet<string> = new Set(),
): { nodes: Node[]; edges: Edge[] } {
  const visible = nodes.filter((node) => !hiddenModules.has(node.module));

  // The lifecycle view is the one that is not a filter. Every other view shows
  // a subset of the constructs; this one draws what is *inside* them — the
  // states an entity moves between, which exist in the graph as a field's
  // transition list rather than as nodes of their own.
  if (view === "lifecycle") {
    return stateMachines(visible);
  }

  const kept = visible.filter((node) => inView(node, view));
  const present = new Set(kept.map((node) => node.id));
  return {
    nodes: kept,
    edges: edges.filter((edge) => present.has(edge.from) && present.has(edge.to)),
  };
}

/** The id of one state of one field of one entity. */
export function stateId(node: Node, field: string, state: string): string {
  return `${node.module}::state::${node.name}.${field}.${state}`;
}

/**
 * The construct a canvas id belongs to.
 *
 * The lifecycle view draws states, and a state is not something the spec
 * declares — it is a value inside an entity's transition list. So selecting one
 * selects the entity, which is the thing that has source to show and clauses to
 * read. Every other id is already its own construct and passes through.
 */
export function ownerOf(id: string): string {
  const [module, kind, path] = id.split("::");
  if (path === undefined || kind !== "state" || id.split("::").length !== 3) {
    return id;
  }
  return `${module}::entity::${path.split(".")[0]}`;
}

/**
 * Every entity's lifecycle, drawn as the state machine it is.
 *
 * The entity itself is kept and joined to the states nothing else leads to, so
 * a reader can see which machine they are looking at and where it starts. The
 * states are pills, which is what the shape vocabulary already uses for a
 * closed set of values.
 */
function stateMachines(nodes: Node[]): { nodes: Node[]; edges: Edge[] } {
  const drawn: Node[] = [];
  const wires: Edge[] = [];

  for (const node of nodes) {
    if (node.kind !== "entity" || node.detail.type !== "entity") {
      continue;
    }
    const lifecycles = node.detail.transitions.filter((graph) => graph.edges.length > 0);
    if (lifecycles.length === 0) {
      continue;
    }
    // The fields are not what this view is about, and eight rows of them under
    // every machine head buries the states. The inspector still has them.
    drawn.push({ ...node, detail: { type: "none" } });

    for (const lifecycle of lifecycles) {
      const reached = new Set(lifecycle.edges.map((step) => step.to));
      // A cycle reaches every state, so there is no state the machine visibly
      // begins at. Joining the entity to the first one it declares is the only
      // answer that comes from the spec rather than from this tool, and without
      // some join the entity box floats away from its own states entirely.
      const entry = lifecycle.states.filter((state) => !reached.has(state));
      const enters = new Set(entry.length > 0 ? entry : lifecycle.states.slice(0, 1));

      for (const state of lifecycle.states) {
        const terminal = lifecycle.terminal.includes(state);
        drawn.push({
          id: stateId(node, lifecycle.field, state),
          kind: "enum",
          name: state,
          module: node.module,
          qualified: `${node.qualified}.${lifecycle.field}`,
          span: node.span,
          // A terminal state lists itself as its only value, which the node's
          // row renderer shows — so "nothing follows this" is visible on the
          // canvas rather than only in the inspector.
          detail: { type: "enum", values: terminal ? ["terminal"] : [] },
          // A state is a value in a transition list, not a declaration, so
          // there is nothing written above it. What the author wrote about the
          // machine is on the field, and the entity carries that.
          prose: { note: [], guidance: [] },
        });

        // An entering edge joins the entity to its own machine.
        if (enters.has(state)) {
          wires.push({
            from: node.id,
            to: stateId(node, lifecycle.field, state),
            kind: "field",
            label: lifecycle.field,
            span: node.span,
          });
        }
      }

      for (const step of lifecycle.edges) {
        wires.push({
          from: stateId(node, lifecycle.field, step.from),
          to: stateId(node, lifecycle.field, step.to),
          kind: "mutates",
          label: "",
          span: node.span,
        });
      }
    }
  }

  return { nodes: drawn, edges: wires };
}
