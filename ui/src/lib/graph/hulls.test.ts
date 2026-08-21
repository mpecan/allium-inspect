import { describe, expect, it } from "vitest";

import type { Node } from "../api/Node";
import type { PlacedNode } from "./layout";
import { hullId, hulls } from "./hulls";

function node(name: string, module: string): Node {
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

function at(id: string, x: number, y: number): PlacedNode {
  return { id, x, y, width: 100, height: 40 };
}

describe("ringing each file", () => {
  it("boxes every construct of a module, whatever the layout did with them", () => {
    const nodes = [node("A", "one"), node("B", "one")];
    const placed = [at(nodes[0]!.id, 0, 0), at(nodes[1]!.id, 300, 200)];

    const [box] = hulls(nodes, placed);
    expect(box?.module).toBe("one");
    expect(box?.held).toBe(2);
    // Encloses both, with room to spare on every side.
    expect(box!.x).toBeLessThan(0);
    expect(box!.y).toBeLessThan(0);
    expect(box!.x + box!.width).toBeGreaterThan(400);
    expect(box!.y + box!.height).toBeGreaterThan(240);
  });

  it("leaves room above the box for its name", () => {
    const nodes = [node("A", "one"), node("B", "one")];
    const placed = [at(nodes[0]!.id, 0, 100), at(nodes[1]!.id, 0, 200)];
    const [box] = hulls(nodes, placed);
    // The label sits inside the top edge, so the top needs more clearance than
    // the sides — otherwise the name lands on the line it belongs to.
    const above = 100 - box!.y;
    const below = box!.y + box!.height - 240;
    expect(above).toBeGreaterThan(below);
  });

  it("draws nothing around a module with one construct", () => {
    // A boundary around one thing is a line that says what the box already
    // said.
    const nodes = [node("A", "one"), node("B", "two"), node("C", "two")];
    const placed = nodes.map((n, i) => at(n.id, i * 200, 0));
    expect(hulls(nodes, placed).map((box) => box.module)).toEqual(["two"]);
  });

  it("does not ring a module box, which is already a file", () => {
    const port: Node = { ...node("identity", "identity"), id: "identity::module" };
    const nodes = [port, node("A", "identity"), node("B", "identity")];
    const placed = nodes.map((n, i) => at(n.id, i * 50, 0));
    const [box] = hulls(nodes, placed);
    // Two constructs enclosed, not three: the destination box is outside.
    expect(box?.held).toBe(2);
  });

  it("orders the biggest box first so a nested one stays readable", () => {
    const nodes = [
      node("A", "big"),
      node("B", "big"),
      node("C", "small"),
      node("D", "small"),
    ];
    const placed = [
      at(nodes[0]!.id, 0, 0),
      at(nodes[1]!.id, 800, 600),
      at(nodes[2]!.id, 300, 300),
      at(nodes[3]!.id, 360, 340),
    ];
    expect(hulls(nodes, placed).map((box) => box.module)).toEqual(["big", "small"]);
  });

  it("names a hull in a namespace of its own", () => {
    expect(hullId("identity")).toBe("identity::hull");
  });
});
