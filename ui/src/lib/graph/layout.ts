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
import { summaryRows, type Row } from "./rows";
import type { Point } from "./route";

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
  /**
   * The polyline ELK routed each edge along, keyed by its position in the edge
   * list it was given. Empty when the layout failed and the grid took over.
   */
  routes: Map<number, Point[]>;
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
  edges?: ElkEdge[];
}

export interface ElkEdge {
  id: string;
  sources: string[];
  targets: string[];
  /** Present only in a result: where ELK decided the edge should run. */
  sections?: {
    startPoint: Point;
    endPoint: Point;
    bendPoints?: Point[];
  }[];
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

/**
 * The metrics a node is drawn at, in pixels.
 *
 * These mirror ConstructBody's stylesheet, and they are measured rather than
 * guessed: the name is set at `--t-title` in a monospace face whose advance is
 * 9.05px, the rows at `--t-micro` where it is 6.05px. Reserving less than the
 * box actually needs is not a cosmetic error — ELK packs the layout to the
 * sizes it is given, so every node it under-measures overlaps its neighbour.
 */
const NAME_CHAR = 9.05;
const ROW_CHAR = 6.05;
/** Horizontal padding either side, plus the border. */
const PADDING = 20;
/** `--gap-3` between a row's label and its value. */
const ROW_GAP = 12;
/** `.row-value`'s `max-width: 11ch` — a long type name is ellipsised, not wide. */
const VALUE_CAP = 11;
/** The kind eyebrow, the name, and the rule above the rows. */
const HEADER_HEIGHT = 58;
const ROW_HEIGHT = 15;
const MIN_CONTENT = 76;
/**
 * The widest a node's *rows* may make it.
 *
 * The name is deliberately not capped. One entity with a long field list should
 * not set the scale for the whole canvas, but a construct whose name is clipped
 * cannot be identified at all, which is the one thing a box on a graph is for.
 */
const MAX_ROW_WIDTH = 240;

/**
 * How many values an enum may hold and still be drawn as a stadium.
 *
 * A stadium's ends are semicircles of half its height, so every row away from
 * the middle is inset further than the last. Two or three rows is a pill; six
 * is a lens with the first and last line clipped by the curve, and the amount
 * of padding that would fix it grows with the height until the shape is mostly
 * empty. Past this the enum is a generously rounded rectangle instead — the
 * same family, without the wasted corners.
 */
export const PILL_ROWS = 3;

/**
 * The extra room the two enum shapes need over a plain rectangle.
 *
 * These are the paddings the CSS adds for `.kind-enum`, and they have to stay
 * in step with it: ELK trusts the sizes it is given, so a node that draws
 * bigger than it measured overlaps its neighbour and its edges attach in the
 * wrong place.
 */
const PILL_WIDTH = 30;
const PILL_HEIGHT = 8;
const ROUNDED_WIDTH = 12;

/** How wide one row is drawn, with its value ellipsised as the CSS does. */
function rowWidth(row: Row): number {
  const value = Math.min(row.value?.length ?? 0, VALUE_CAP);
  const label = row.label.length * ROW_CHAR;
  return value === 0 ? label : label + ROW_GAP + value * ROW_CHAR;
}

/**
 * How wide and tall to draw a node.
 *
 * Sized from its content rather than fixed, because a node whose text is
 * clipped forces the reader into the inspector to find out what they are
 * looking at — which defeats having a graph. It asks `summaryRows` for the rows
 * rather than being told how many there are, so that what is measured and what
 * is drawn cannot drift apart.
 */
export function measure(node: Node): { width: number; height: number } {
  const rows = summaryRows(node);
  const widest = rows.reduce((max, row) => Math.max(max, rowWidth(row)), 0);
  const content = Math.max(
    MIN_CONTENT,
    node.name.length * NAME_CHAR,
    node.kind.length * ROW_CHAR,
    Math.min(widest, MAX_ROW_WIDTH),
  );
  const extra = enumShape(node, rows.length);
  return {
    width: Math.ceil(PADDING + content + extra.width),
    height: HEADER_HEIGHT + rows.length * ROW_HEIGHT + extra.height,
  };
}

/** The room an enum's shape costs it, over a rectangle of the same content. */
function enumShape(node: Node, rows: number): { width: number; height: number } {
  if (node.kind !== "enum") {
    return { width: 0, height: 0 };
  }
  return rows <= PILL_ROWS
    ? { width: PILL_WIDTH, height: PILL_HEIGHT }
    : { width: ROUNDED_WIDTH, height: PILL_HEIGHT };
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
    // Route the edges as well as place the nodes. ELK reserves channels between
    // the layers and threads the edges through them; without this the canvas
    // draws a bezier from handle to handle, straight across whatever is in the
    // way.
    "elk.edgeRouting": "ORTHOGONAL",
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
    return { nodes: [], routes: new Map(), width: 0, height: 0 };
  }

  const sized = nodes.map((node) => ({ id: node.id, ...measure(node) }));

  // Only edges whose endpoints are both in this view. ELK rejects a graph with
  // an edge to an unknown node, and a view is a filter, so dangling edges are
  // expected rather than exceptional.
  const present = new Set(sized.map((node) => node.id));
  // Numbered by position in the *given* list, not in the filtered one, so the
  // caller can look a route up against the edge it passed in.
  const usable = edges
    .map((edge, index) => ({ edge, index }))
    .filter(({ edge }) => present.has(edge.from) && present.has(edge.to));

  try {
    const result = await elk.layout({
      id: "root",
      layoutOptions: optionsFor(view),
      children: sized,
      edges: usable.map(({ edge, index }) => ({
        id: `e${index}`,
        sources: [edge.from],
        targets: [edge.to],
      })),
    });
    return collect(result.children ?? [], sized, result.edges ?? []);
  } catch {
    return grid(sized);
  }
}

/** The polyline ELK routed each edge along, by the number it was given. */
function routesOf(placed: ElkEdge[]): Map<number, Point[]> {
  const routes = new Map<number, Point[]>();
  for (const edge of placed) {
    const section = edge.sections?.[0];
    const index = Number(edge.id.slice(1));
    if (!section || !Number.isInteger(index)) {
      continue;
    }
    routes.set(index, [section.startPoint, ...(section.bendPoints ?? []), section.endPoint]);
  }
  return routes;
}

/** Read positions back out of an ELK result, falling back per node. */
function collect(placed: ElkNode[], sized: SizedNode[], routed: ElkEdge[]): Layout {
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

  return { ...bounds(nodes), routes: routesOf(routed) };
}

/** Place nodes on a simple grid, row wrapping. */
function grid(sized: SizedNode[]): Layout {
  const COLUMNS = 4;
  const COLUMN = 300;
  const ROW = 140;
  const nodes = sized.map((node, index) => ({
    ...node,
    x: (index % COLUMNS) * COLUMN,
    y: Math.floor(index / COLUMNS) * ROW,
  }));
  // No routes: a layout that failed has no opinion about where the edges run,
  // and the canvas falls back to drawing them straight.
  return { ...bounds(nodes), routes: new Map() };
}

function bounds(nodes: PlacedNode[]): Omit<Layout, "routes"> {
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
 * The band a kind belongs to: what a spec *has*, what it *does*, where it
 * *touches the world*, and what *limits* it.
 *
 * A distinction a reader learns in a minute, and the one the layout and the
 * palette are both organised around. Each kind carries its own hue *within*
 * its band — see `--kind-*` in `theme.css` — so a trigger is tellable from a
 * rule without either leaving the band it belongs to. This function is the
 * grouping; the hue is the differentiation, and neither does the other's job.
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
