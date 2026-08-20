// Drawing an edge along the path the layout engine chose for it.
//
// ELK routes every edge as part of laying the graph out: it reserves channels
// between the layers and threads the edges through them, around the nodes
// rather than across them. That work was being thrown away — the canvas drew a
// bezier from one node's handle to the other's, which on a dense view means
// lines cutting through the boxes they pass and two edges leaving the same
// point in opposite directions.
//
// So this turns ELK's polyline into an SVG path. The corners are rounded
// because a hairline right-angle at a small zoom reads as a break in the line,
// and because the rest of the interface has no square corners in it.

/** A point in layout coordinates. */
export interface Point {
  x: number;
  y: number;
}

/** How far back from a corner the curve starts. */
const RADIUS = 7;

function distance(from: Point, to: Point): number {
  return Math.hypot(to.x - from.x, to.y - from.y);
}

/** The point `by` along the way from `from` towards `to`. */
function towards(from: Point, to: Point, by: number): Point {
  const span = distance(from, to);
  if (span === 0) {
    return { x: from.x, y: from.y };
  }
  const share = by / span;
  return { x: from.x + (to.x - from.x) * share, y: from.y + (to.y - from.y) * share };
}

/**
 * An SVG path through `points`, with the corners rounded.
 *
 * The radius shrinks to fit a short segment, so two bends close together curve
 * less rather than overshooting each other and drawing a knot.
 */
export function pathThrough(points: readonly Point[], radius = RADIUS): string {
  const [first, ...rest] = points;
  if (first === undefined || rest.length === 0) {
    return "";
  }

  let path = `M ${first.x},${first.y}`;
  for (let at = 1; at < points.length - 1; at += 1) {
    const previous = points[at - 1];
    const corner = points[at];
    const next = points[at + 1];
    if (!previous || !corner || !next) {
      continue;
    }
    const into = towards(corner, previous, Math.min(radius, distance(previous, corner) / 2));
    const out = towards(corner, next, Math.min(radius, distance(corner, next) / 2));
    path += ` L ${into.x},${into.y} Q ${corner.x},${corner.y} ${out.x},${out.y}`;
  }

  const last = points[points.length - 1];
  return last === undefined ? path : `${path} L ${last.x},${last.y}`;
}

/**
 * The point half way along the polyline.
 *
 * By length rather than by vertex count: an edge that runs a long way and then
 * turns twice in quick succession would otherwise be labelled at the turns,
 * which is where it is least readable and furthest from where the eye is.
 */
export function midpoint(points: readonly Point[]): Point {
  const [first] = points;
  if (first === undefined) {
    return { x: 0, y: 0 };
  }
  const total = points.reduce(
    (sum, point, at) => (at === 0 ? 0 : sum + distance(points[at - 1] as Point, point)),
    0,
  );

  let walked = 0;
  for (let at = 1; at < points.length; at += 1) {
    const from = points[at - 1] as Point;
    const to = points[at] as Point;
    const span = distance(from, to);
    if (walked + span >= total / 2) {
      return towards(from, to, total / 2 - walked);
    }
    walked += span;
  }
  return { x: first.x, y: first.y };
}
