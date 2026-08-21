import { describe, expect, it } from "vitest";

import type { Edge } from "../api/Edge";
import type { Node } from "../api/Node";
import { reportOn } from "./modules";

function node(kind: Node["kind"], name: string, module: string): Node {
  return {
    id: `${module}::${kind}::${name}`,
    kind,
    name,
    module,
    qualified: `${module}/${name}`,
    span: null,
    detail: { type: "none" },
    prose: { note: [], guidance: [] },
  };
}

function wire(from: string, to: string): Edge {
  return { from, to, kind: "relationship", label: "", span: null };
}

// `identity` holds two constructs. `Identity` is reached for by two other
// files; `Secret` is reached for by nobody, which is the distinction this
// whole panel exists to draw.
const NODES = [
  node("entity", "Identity", "identity"),
  node("entity", "Secret", "identity"),
  node("entity", "Group", "membership"),
  node("entity", "Message", "messaging"),
];

const EDGES = [
  wire("membership::entity::Group", "identity::entity::Identity"),
  wire("membership::entity::Group", "identity::entity::Identity"),
  wire("messaging::entity::Message", "identity::entity::Identity"),
  wire("identity::entity::Identity", "membership::entity::Group"),
  wire("identity::entity::Secret", "identity::entity::Identity"),
];

describe("reading a module from the outside", () => {
  it("counts what the file holds", () => {
    expect(reportOn("identity", NODES, EDGES).held).toBe(2);
  });

  it("names only the constructs other files reached for", () => {
    // Allium has no `pub`, so this list is not readable from the declaration —
    // it is whatever the rest of the set happened to name.
    const report = reportOn("identity", NODES, EDGES);
    expect(report.exported.map((e) => e.node.name)).toEqual(["Identity"]);
    // `Secret` is referenced, but only from inside its own file. A reference
    // that never crosses is not part of an interface.
    expect(report.exported.some((e) => e.node.name === "Secret")).toBe(false);
  });

  it("counts every arrival and says which files they came from", () => {
    const report = reportOn("identity", NODES, EDGES);
    const identity = report.exported[0];
    expect(identity?.count).toBe(3);
    expect(identity?.from).toEqual(["membership", "messaging"]);
  });

  it("keeps the two directions of a neighbour apart", () => {
    // The pair that reaches both ways is the one worth seeing: `identity` is
    // reached from `membership` twice and reaches back once.
    const report = reportOn("identity", NODES, EDGES);
    expect(report.neighbours).toEqual([
      { module: "membership", out: 1, into: 2 },
      { module: "messaging", out: 0, into: 1 },
    ]);
  });

  it("reports a leaf as reached for by nobody", () => {
    const report = reportOn("messaging", NODES, EDGES);
    expect(report.exported).toEqual([]);
    expect(report.neighbours).toEqual([{ module: "identity", out: 1, into: 0 }]);
  });

  it("orders the surface by how much depends on it", () => {
    const busier = [...NODES, node("entity", "Device", "identity")];
    const edges = [
      ...EDGES,
      wire("membership::entity::Group", "identity::entity::Device"),
    ];
    // Most reached-for first: that is the part of the file hardest to change.
    expect(reportOn("identity", busier, edges).exported.map((e) => e.node.name)).toEqual([
      "Identity",
      "Device",
    ]);
  });
});
