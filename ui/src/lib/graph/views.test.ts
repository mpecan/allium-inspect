import { describe, expect, it } from "vitest";

import type { Node } from "../api/Node";
import type { NodeDetail } from "../api/NodeDetail";
import type { ViewKind } from "../client";
import { ANSWERS, inView, project } from "./views";

const VIEWS: ViewKind[] = ["domain", "flow", "lifecycle", "journey"];

function node(
  kind: Node["kind"],
  name: string,
  detail: NodeDetail = { type: "none" },
  module = "catalogue",
): Node {
  return {
    id: `${module}::${kind}::${name}`,
    kind,
    name,
    module,
    qualified: `${module}/${name}`,
    span: null,
    detail,
  };
}

const withLifecycle = node("entity", "Copy", {
  type: "entity",
  kind: "internal",
  parent: null,
  fields: [],
  transitions: [
    {
      field: "status",
      states: ["available", "lost"],
      edges: [{ from: "available", to: "lost" }],
      terminal: ["lost"],
    },
  ],
});

const withoutLifecycle = node("entity", "Member", {
  type: "entity",
  kind: "internal",
  parent: null,
  fields: [],
  transitions: [],
});

describe("inView", () => {
  it("puts the things a spec holds in the domain view", () => {
    for (const kind of ["entity", "value", "variant", "enum", "config"] as const) {
      expect(inView(node(kind, "X"), "domain")).toBe(true);
    }
  });

  it("keeps rules and surfaces out of the domain view", () => {
    expect(inView(node("rule", "AddBook"), "domain")).toBe(false);
    expect(inView(node("surface", "Desk"), "domain")).toBe(false);
  });

  it("puts triggers and rules in the flow view", () => {
    expect(inView(node("trigger", "MemberBorrows"), "flow")).toBe(true);
    expect(inView(node("rule", "BorrowCopy"), "flow")).toBe(true);
  });

  it("puts the boundary in the journey view, which is where a chain starts", () => {
    expect(inView(node("surface", "MemberShelf"), "journey")).toBe(true);
    expect(inView(node("actor", "Reader"), "journey")).toBe(true);
  });

  it("draws only entities that actually have a lifecycle", () => {
    // Otherwise the canvas fills with boxes that have nothing to show and
    // buries the three that do.
    expect(inView(withLifecycle, "lifecycle")).toBe(true);
    expect(inView(withoutLifecycle, "lifecycle")).toBe(false);
  });

  it("keeps everything but entities out of the lifecycle view", () => {
    expect(inView(node("rule", "R"), "lifecycle")).toBe(false);
    expect(inView(node("enum", "Medium"), "lifecycle")).toBe(false);
  });

  it("shows an unresolved reference only where it is not a dead end", () => {
    // In the domain view it is a fact worth seeing. In a causal chain it would
    // be a stub in the middle of the thing the reader is trying to follow.
    expect(inView(node("external", "Phantom"), "domain")).toBe(true);
    expect(inView(node("external", "Phantom"), "flow")).toBe(false);
    expect(inView(node("external", "Phantom"), "journey")).toBe(false);
  });

  it("puts every construct in at least one view", () => {
    // A kind that appears nowhere is invisible in the tool, which reads as a
    // spec that does not contain it.
    const kinds = [
      "entity", "value", "variant", "enum", "rule", "trigger",
      "surface", "actor", "config", "invariant", "external",
    ] as const;
    const homeless = kinds.filter((kind) => {
      const sample =
        kind === "entity" ? withLifecycle : node(kind, "X");
      return !VIEWS.some((view) => inView(sample, view));
    });
    // Invariants are shown against the constructs they constrain rather than as
    // nodes of their own, so they are the one deliberate exception.
    expect(homeless).toEqual(["invariant"]);
  });
});

describe("project", () => {
  const nodes = [
    node("entity", "Book"),
    withLifecycle,
    node("rule", "AddBook"),
    node("entity", "Loan", { type: "none" }, "lending"),
  ];
  const edges = [
    { from: "catalogue::entity::Copy", to: "catalogue::entity::Book" },
    { from: "catalogue::rule::AddBook", to: "catalogue::entity::Book" },
    { from: "lending::entity::Loan", to: "catalogue::entity::Copy" },
  ];

  it("keeps only the nodes the view draws", () => {
    const { nodes: kept } = project("domain", nodes, edges);
    expect(kept.map((n) => n.name).sort()).toEqual(["Book", "Copy", "Loan"]);
  });

  it("drops edges whose other end the view excluded", () => {
    // An arrow into empty space is worse than no arrow.
    const { edges: kept } = project("domain", nodes, edges);
    expect(kept).toHaveLength(2);
    expect(kept.every((edge) => !edge.from.includes("rule"))).toBe(true);
  });

  it("hides the modules it was told to hide, and their edges with them", () => {
    const { nodes: kept, edges: keptEdges } = project(
      "domain",
      nodes,
      edges,
      new Set(["lending"]),
    );
    expect(kept.map((n) => n.name).sort()).toEqual(["Book", "Copy"]);
    expect(keptEdges).toHaveLength(1);
  });

  it("returns nothing when every module is hidden", () => {
    const hidden = new Set(["catalogue", "lending"]);
    expect(project("domain", nodes, edges, hidden)).toEqual({ nodes: [], edges: [] });
  });
});

describe("ANSWERS", () => {
  it("says what every view is for", () => {
    // A reader opening this for the first time does not know what "flow" means
    // here, and a five-word subtitle costs less than the click it saves.
    for (const view of VIEWS) {
      expect(ANSWERS[view].length).toBeGreaterThan(8);
    }
  });
});
