// Every route starts and ends on the construct it joins.
//
// Against the real elkjs, not a stub, because the thing being pinned is
// *elkjs's own behaviour*. Grouping asks ELK to lay each file out inside a
// container, and ELK then returns each route measured from the lowest common
// ancestor of its two ends — from the container when both ends share one, from
// the origin when they do not. Both conventions arrive in a single list.
//
// That rule is inferred from what the library emits, not promised by its
// documentation. An upgrade could change it, and the symptom would be edges
// that appear to join things they do not — arrowheads pointing at nothing,
// which is the one drawing mistake worse than leaving the line out. The
// failure is visible rather than silent, but only if somebody happens to look
// at a grouped view, so it is worth a test that looks for us.

import ELK from "elkjs/lib/elk.bundled.js";
import { describe, expect, it } from "vitest";

import type { Edge } from "../api/Edge";
import type { Node } from "../api/Node";
import type { ElkLike, PlacedNode } from "./layout";
import { layout } from "./layout";

// elkjs ships looser types than this module needs — `layout` there returns a
// graph whose `id` is optional. The double cast is the seam, and it is safe in
// exactly one direction: this test only ever *reads* what comes back.
const elk = new ELK() as unknown as ElkLike;

function entity(name: string, module: string): Node {
  return {
    id: `${module}::entity::${name}`,
    kind: "entity",
    name,
    module,
    qualified: `${module}/${name}`,
    span: null,
    detail: { type: "none" },
    prose: { note: [], guidance: [] },
  };
}

function wire(from: Node, to: Node): Edge {
  return { from: from.id, to: to.id, kind: "relationship", label: "", span: null };
}

/** Two files, with edges inside each and one crossing between them. */
function library() {
  const book = entity("Book", "catalogue");
  const copy = entity("Copy", "catalogue");
  const shelf = entity("Shelf", "catalogue");
  const loan = entity("Loan", "lending");
  const member = entity("Member", "lending");

  return {
    nodes: [book, copy, shelf, loan, member],
    edges: [
      wire(book, copy),
      wire(copy, shelf),
      wire(member, loan),
      // The crossing. This is the one measured differently from the others.
      wire(loan, copy),
    ],
  };
}

/** How far off a route may start before it reads as detached. */
const SLACK = 4;

/** Whether `point` lands on or inside `box`, give or take a few pixels. */
function touches(point: { x: number; y: number }, box: PlacedNode): boolean {
  return (
    point.x >= box.x - SLACK &&
    point.x <= box.x + box.width + SLACK &&
    point.y >= box.y - SLACK &&
    point.y <= box.y + box.height + SLACK
  );
}

describe("routes land on the constructs they join", () => {
  it("does, when each file is laid out in a container", async () => {
    const { nodes, edges } = library();
    const result = await layout(elk, "domain", nodes, edges, true);

    const placed = new Map(result.nodes.map((node) => [node.id, node]));
    expect(result.groups?.length).toBe(2);
    expect(result.routes.size).toBeGreaterThan(0);

    const detached: string[] = [];
    edges.forEach((edge, index) => {
      const route = result.routes.get(index);
      const from = placed.get(edge.from);
      const to = placed.get(edge.to);
      if (!route || !from || !to) {
        return;
      }
      const start = route[0];
      const end = route[route.length - 1];
      if (start && !touches(start, from)) {
        detached.push(`${edge.from} -> start ${start.x},${start.y} not on ${from.x},${from.y}`);
      }
      if (end && !touches(end, to)) {
        detached.push(`${edge.to} -> end ${end.x},${end.y} not on ${to.x},${to.y}`);
      }
    });

    expect(detached).toEqual([]);
  });

  it("does when they are not, which is the control", async () => {
    // The same assertion without grouping. If this one ever fails too, the
    // fault is in reading ELK at all rather than in the container offsets.
    const { nodes, edges } = library();
    const result = await layout(elk, "domain", nodes, edges);
    const placed = new Map(result.nodes.map((node) => [node.id, node]));

    const detached = edges.filter((edge, index) => {
      const route = result.routes.get(index);
      const from = placed.get(edge.from);
      return route?.[0] && from ? !touches(route[0], from) : false;
    });

    expect(result.groups ?? []).toEqual([]);
    expect(detached).toEqual([]);
  });
});
