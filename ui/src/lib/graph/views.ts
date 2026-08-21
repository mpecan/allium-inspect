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
//   chain      what follows from an act   the boundary, and the chain from it
//   modules    how the files depend        one node per file, weighted by crossings
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
  chain: ["surface", "actor", "trigger", "rule", "entity"],
  // Synthesised rather than filtered, like `lifecycle`. Every kind counts
  // toward its module's census, so nothing is excluded here.
  modules: [],
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
  chain: "what follows from an action",
  modules: "how the files lean on each other",
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
  if (view === "modules") {
    return moduleGraph(visible, edges);
  }

  const kept = visible.filter((node) => inView(node, view));
  const present = new Set(kept.map((node) => node.id));
  const drawn = edges.filter((edge) => present.has(edge.from) && present.has(edge.to));
  const { nodes: ports, edges: crossings } = neighbours(nodes, edges, present, hiddenModules);
  return { nodes: [...kept, ...ports], edges: [...drawn, ...crossings] };
}

/**
 * The files switched off, drawn as the boxes their references arrive from.
 *
 * Switching a module off in the rail used to delete its constructs *and*
 * silently drop every edge that reached them, so narrowing to one file made it
 * look self-contained — the most misleading possible answer, because the whole
 * reason to look at one file is to understand what it needs from the others.
 *
 * So a reference that leaves the drawing terminates on the module it went to,
 * and the boundary becomes something on screen rather than an edge that ends
 * nowhere. Narrow the rail to a single module and you get that file in full,
 * ringed by the files it leans on and the files that lean on it.
 *
 * Inert when nothing is hidden: every edge has both ends on the canvas, so
 * there is nothing to terminate.
 */
function neighbours(
  all: Node[],
  edges: Edge[],
  present: ReadonlySet<string>,
  hidden: ReadonlySet<string>,
): { nodes: Node[]; edges: Edge[] } {
  if (hidden.size === 0) {
    return { nodes: [], edges: [] };
  }
  const home = new Map(all.map((node) => [node.id, node.module]));
  const reached = new Set<string>();
  const wires = new Map<string, Edge>();

  for (const edge of edges) {
    const from = home.get(edge.from);
    const to = home.get(edge.to);
    if (from === undefined || to === undefined) {
      continue;
    }
    // Exactly one end on the canvas. Both ends hidden is a relationship
    // between two files the reader is not looking at, and drawing it would
    // answer a question nobody asked.
    const here = present.has(edge.from);
    const there = present.has(edge.to);
    if (here === there) {
      continue;
    }
    const away = here ? to : from;
    if (!hidden.has(away)) {
      continue;
    }
    reached.add(away);
    // One line per construct per neighbour. A construct that reaches into the
    // same file three times has one relationship with it, and three parallel
    // arrows say nothing the one does not.
    const wire: Edge = here
      ? { ...edge, to: moduleId(away) }
      : { ...edge, from: moduleId(away) };
    wires.set(`${wire.from} ${wire.to}`, wire);
  }

  return {
    nodes: [...reached].sort().map(portNode),
    edges: [...wires.values()],
  };
}

/** A switched-off module, as the box its references arrive from. */
function portNode(name: string): Node {
  return {
    id: moduleId(name),
    kind: "config",
    name,
    module: name,
    qualified: name,
    span: null,
    // No census here. In the modules view the numbers are the content; here
    // the box is a destination, and rows would give a file nobody is looking
    // at more weight on the canvas than the ones they are.
    detail: { type: "none" },
    prose: { note: [], guidance: [] },
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

/** The id of a module node. Not a construct, so it has a namespace of its own. */
export function moduleId(name: string): string {
  return `${name}::module`;
}

/**
 * Whether a canvas id is a module rather than a construct.
 *
 * The namespace is the marker. A module borrows the `config` kind to be drawn,
 * so `kind` cannot answer this and the id has to.
 */
export function isModuleNode(id: string): boolean {
  return id.endsWith("::module");
}

/**
 * The spec set as its own files, and how much each leans on the others.
 *
 * The one view that is about the *decomposition* rather than about what is in
 * it. Every other view treats a module as a filter — a checkbox in the rail —
 * and throws away what the split means. But putting `Group` in `membership`
 * and `Message` in `messaging` was a decision, and the references that cross
 * between them are the interface those files have with each other, whether or
 * not the spec ever says the word.
 *
 * Two things this makes visible that nothing else does. **Weight**: an edge
 * labelled 16 and an edge labelled 1 are not the same relationship, and the
 * construct-level views draw them as the same number of arrows scattered
 * across the canvas. **Direction**: a pair of modules that reference each other
 * both ways is a cycle in the dependency graph, and a single edge running back
 * against sixteen is either a deliberate exception or something nobody has
 * noticed.
 *
 * Drawn as `config` nodes. A module is not a configuration block, but it is the
 * one kind in the vocabulary that means "a named container of declarations"
 * rather than a thing with behaviour of its own — and its row renderer is the
 * name-and-number shape a census wants. The same borrowing the lifecycle view
 * makes when it draws states as pills.
 */
function moduleGraph(nodes: Node[], edges: Edge[]): { nodes: Node[]; edges: Edge[] } {
  const home = new Map(nodes.map((node) => [node.id, node.module]));

  const held = new Map<string, number>();
  for (const node of nodes) {
    held.set(node.module, (held.get(node.module) ?? 0) + 1);
  }

  // Crossings, counted per ordered pair. An edge with an end this view cannot
  // place — one of the modules is switched off in the rail — is not a crossing
  // anybody can see, so it is left out rather than drawn into empty space.
  const crossings = new Map<string, number>();
  const out = new Map<string, number>();
  const into = new Map<string, number>();
  for (const edge of edges) {
    const from = home.get(edge.from);
    const to = home.get(edge.to);
    if (from === undefined || to === undefined || from === to) {
      continue;
    }
    const pair = `${from} ${to}`;
    crossings.set(pair, (crossings.get(pair) ?? 0) + 1);
    out.set(from, (out.get(from) ?? 0) + 1);
    into.set(to, (into.get(to) ?? 0) + 1);
  }

  const drawn: Node[] = [...held.keys()].sort().map((name) => ({
    id: moduleId(name),
    kind: "config",
    name,
    module: name,
    qualified: name,
    // A module is a file rather than a declaration inside one, so there is no
    // span to point at. The source strip has nothing narrower to show than the
    // whole file, and saying so beats sending a reader to line 1.
    span: null,
    detail: {
      type: "config",
      parameters: [
        census("constructs", held.get(name) ?? 0),
        census("references out", out.get(name) ?? 0),
        census("referenced by", into.get(name) ?? 0),
      ],
    },
    prose: { note: [], guidance: [] },
  }));

  const wires: Edge[] = [...crossings.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([pair, count]) => {
      const [from = "", to = ""] = pair.split(" ");
      return {
        from: moduleId(from),
        to: moduleId(to),
        // `imports` is what this is: the edge kind the model already uses
        // for one file depending on another, here weighted by how much.
        kind: "imports" as Edge["kind"],
        label: String(count),
        span: null,
      };
    });

  return { nodes: drawn, edges: wires };
}

/** One census line, in the shape the config row renderer reads. */
function census(name: string, count: number) {
  return { name, type_expr: String(count), default_expr: null };
}
