// Turning a spec graph into placed nodes.
//
// Layout is a decision about how a reader should scan the picture, so it is
// made per view rather than once. Every view here runs left to right, which is
// the direction the language itself is written in — a domain relationship, a
// causal chain and a state transition all read as "this, then that".
//
// The lifecycle view is many small disconnected machines rather than one graph,
// so what it needs from ELK is not a direction but separation: enough space
// between components that a reader can see where one entity's machine ends.
//
// ELK does the placement. The part worth testing is everything around it —
// which options each view asks for, how sizes are chosen, how a disconnected
// node is kept on screen, and what happens when layout fails — so that is what
// is factored out here and covered.

import type { Edge } from "../api/Edge";
import type { Node } from "../api/Node";
import type { NodeKind } from "../api/NodeKind";
import type { ViewKind } from "../client";

/** A node with a place and a size, ready to render. */
export interface PlacedNode {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The result of laying a view out. */
export interface Layout {
  nodes: PlacedNode[];
  width: number;
  height: number;
}

/** The minimal shape this module needs from an ELK implementation.
 *
 * Structural rather than an import of elkjs's own types: it keeps the engine
 * swappable in tests, and every dimension is optional here because elkjs
 * declares them that way — a node it returns without coordinates is a case this
 * module has to handle rather than one the type system rules out. */
export interface ElkLike {
  layout(graph: ElkGraph): Promise<ElkGraph>;
}

export interface ElkGraph {
  id: string;
  layoutOptions?: Record<string, string>;
  children?: ElkNode[];
  edges?: { id: string; sources: string[]; targets: string[] }[];
}

export interface ElkNode {
  id: string;
  width?: number;
  height?: number;
  x?: number;
  y?: number;
}

/** A node this module sized itself, so its dimensions are known. */
interface SizedNode extends ElkNode {
  width: number;
  height: number;
}

/** Character width and padding used to size a node from its label. */
const CHAR_WIDTH = 7.2;
const PADDING = 28;
const MIN_WIDTH = 96;
const MAX_WIDTH = 260;
const ROW_HEIGHT = 17;
const HEADER_HEIGHT = 34;

/**
 * How wide and tall to draw a node.
 *
 * Sized from its content rather than fixed, because a node whose text is
 * clipped forces the reader into the inspector to find out what they are
 * looking at — which defeats having a graph. The cap is what stops one entity
 * with a long field list from setting the scale for the whole canvas.
 */
export function measure(node: Node, rows = 0): { width: number; height: number } {
  const label = Math.max(node.name.length, node.kind.length + 2);
  const width = Math.min(
    MAX_WIDTH,
    Math.max(MIN_WIDTH, Math.round(label * CHAR_WIDTH + PADDING)),
  );
  const height = HEADER_HEIGHT + Math.min(rows, 8) * ROW_HEIGHT;
  return { width, height };
}

/**
 * The ELK options a view wants.
 *
 * The lifecycle view is the one that differs, and not in its direction. It is a
 * page of separate state machines, so the reader's problem is telling them
 * apart — hence component separation wide enough to read as a gap, and rows
 * packed left to right so each machine is one band across the page.
 */
export function optionsFor(view: ViewKind): Record<string, string> {
  const shared = {
    "elk.algorithm": "layered",
    "elk.direction": "RIGHT",
    "elk.layered.considerModelOrder.strategy": "NODES_AND_EDGES",
    // Deterministic placement. Without it the same graph lands differently
    // between runs, which makes a screenshot diff and a shared link useless.
    "elk.randomSeed": "1",
  };
  if (view === "lifecycle") {
    return {
      ...shared,
      "elk.spacing.nodeNode": "20",
      "elk.layered.spacing.nodeNodeBetweenLayers": "48",
      "elk.separateConnectedComponents": "true",
      // Many times the spacing within a machine. One entity's states have to
      // read as belonging together before the gap between machines means
      // anything.
      "elk.spacing.componentComponent": "140",
      "elk.layered.components.direction": "RIGHT",
    };
  }
  return {
    ...shared,
    "elk.spacing.nodeNode": "24",
    "elk.layered.spacing.nodeNodeBetweenLayers": "72",
  };
}

/** How many detail rows a node shows on the canvas, for sizing. */
export function rowsOf(node: Node): number {
  const detail = node.detail;
  switch (detail.type) {
    case "entity":
      return detail.fields.length;
    case "enum":
      return detail.values.length;
    case "config":
      return detail.parameters.length;
    case "rule":
      // The clause count, not the clauses: a rule node shows how many
      // preconditions and postconditions it has and leaves the text to the
      // inspector, because a clause is a sentence and a node is a box.
      return Math.min(detail.clauses.length, 4);
    case "surface":
      return detail.provides.length;
    default:
      return 0;
  }
}

/**
 * Place `nodes` for `view`.
 *
 * Returns nodes in the order given, each with a position. On failure every node
 * is still returned, placed on a simple grid: a canvas that cannot lay itself
 * out should still show what it holds.
 */
export async function layout(
  elk: ElkLike,
  view: ViewKind,
  nodes: Node[],
  edges: Edge[],
): Promise<Layout> {
  if (nodes.length === 0) {
    return { nodes: [], width: 0, height: 0 };
  }

  const sized = nodes.map((node) => ({
    id: node.id,
    ...measure(node, rowsOf(node)),
  }));

  // Only edges whose endpoints are both in this view. ELK rejects a graph with
  // an edge to an unknown node, and a view is a filter, so dangling edges are
  // expected rather than exceptional.
  const present = new Set(sized.map((node) => node.id));
  const usable = edges.filter(
    (edge) => present.has(edge.from) && present.has(edge.to),
  );

  try {
    const result = await elk.layout({
      id: "root",
      layoutOptions: optionsFor(view),
      children: sized,
      edges: usable.map((edge, index) => ({
        id: `e${index}`,
        sources: [edge.from],
        targets: [edge.to],
      })),
    });
    return collect(result.children ?? [], sized);
  } catch {
    return grid(sized);
  }
}

/** Read positions back out of an ELK result, falling back per node. */
function collect(placed: ElkNode[], sized: SizedNode[]): Layout {
  const byId = new Map(placed.map((node) => [node.id, node]));
  const fallback = grid(sized);
  const fallbackById = new Map(fallback.nodes.map((node) => [node.id, node]));

  const nodes = sized.map((node) => {
    const result = byId.get(node.id);
    // A node ELK returned without coordinates would land on top of every other
    // such node at the origin. Its grid slot is a worse layout but a readable
    // one.
    if (result?.x === undefined || result.y === undefined) {
      return fallbackById.get(node.id) ?? { ...node, x: 0, y: 0 };
    }
    return { ...node, x: result.x, y: result.y };
  });

  return bounds(nodes);
}

/** Place nodes on a simple grid, row wrapping. */
function grid(sized: SizedNode[]): Layout {
  const COLUMNS = 4;
  const COLUMN = MAX_WIDTH + 40;
  const ROW = 140;
  const nodes = sized.map((node, index) => ({
    ...node,
    x: (index % COLUMNS) * COLUMN,
    y: Math.floor(index / COLUMNS) * ROW,
  }));
  return bounds(nodes);
}

function bounds(nodes: PlacedNode[]): Layout {
  const width = nodes.reduce((max, node) => Math.max(max, node.x + node.width), 0);
  const height = nodes.reduce(
    (max, node) => Math.max(max, node.y + node.height),
    0,
  );
  return { nodes, width, height };
}

/** The construct family a node kind belongs to, which is what colours it. */
export type Family = "thing" | "behaviour" | "boundary" | "constraint" | "unresolved";

/**
 * Four families rather than eleven hues.
 *
 * Eleven colours is a rainbow nobody can hold in their head. Four groups —
 * what a spec *has*, what it *does*, where it *touches the world*, and what
 * *limits* it — is a distinction a reader learns in a minute, and form
 * distinguishes the kinds within a group.
 */
export function familyOf(kind: NodeKind): Family {
  switch (kind) {
    case "entity":
    case "value":
    case "variant":
    case "enum":
      return "thing";
    case "rule":
    case "trigger":
      return "behaviour";
    case "surface":
    case "actor":
      return "boundary";
    case "invariant":
    case "config":
      return "constraint";
    case "external":
      return "unresolved";
  }
}
