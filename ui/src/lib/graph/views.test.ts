import { describe, expect, it } from "vitest";

import type { Edge } from "../api/Edge";
import type { Node } from "../api/Node";
import type { NodeDetail } from "../api/NodeDetail";
import type { ViewKind } from "../client";
import { ANSWERS, inView, ownerOf, project, stateId } from "./views";

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
  const edge = (from: string, to: string): Edge => ({
    from,
    to,
    kind: "field",
    label: "x",
    span: null,
  });
  const edges = [
    edge("catalogue::entity::Copy", "catalogue::entity::Book"),
    edge("catalogue::rule::AddBook", "catalogue::entity::Book"),
    edge("lending::entity::Loan", "catalogue::entity::Copy"),
  ];

  it("keeps only the nodes the view draws", () => {
    const { nodes: kept } = project("domain", nodes, edges);
    expect(kept.map((n) => n.name).sort()).toEqual(["Book", "Copy", "Loan"]);
  });

  it("expands the lifecycle view into the states themselves", () => {
    // The one view that is not a filter. Its subtitle promises "how each
    // entity changes state", and drawing the entities leaves the states
    // visible only in the inspector — which is not the view it claims to be.
    const { nodes: drawn, edges: wires } = project("lifecycle", nodes, edges);
    expect(drawn.map((n) => n.name).sort()).toEqual(["available", "Copy", "lost"].sort());
    expect(drawn.filter((n) => n.kind === "enum")).toHaveLength(2);

    // The machine head carries no field rows: eight of them under every entity
    // would bury the states this view exists to show.
    expect(drawn.find((n) => n.name === "Copy")?.detail).toEqual({ type: "none" });

    const transition = wires.find((wire) => wire.kind === "mutates");
    expect(transition?.from).toContain("available");
    expect(transition?.to).toContain("lost");
  });

  it("joins the entity to the state its machine starts in", () => {
    // Without it a reader sees a pile of pills and cannot tell which machine
    // is which, or where any of them begins.
    const { edges: wires } = project("lifecycle", nodes, edges);
    const entering = wires.filter((wire) => wire.kind === "field");
    expect(entering).toHaveLength(1);
    expect(entering[0]?.from).toBe("catalogue::entity::Copy");
    expect(entering[0]?.to).toContain("available");
    expect(entering[0]?.label).toBe("status");
  });

  it("joins the entity to a cyclic machine, which has no unreached state", () => {
    // fetching -> held -> fetching reaches every state it declares. Without a
    // fallback the entity box floats free of its own states and ELK packs it
    // next to some other entity's machine, which reads as belonging to that one.
    const cyclic = node("entity", "Attachment", {
      type: "entity",
      kind: "internal",
      parent: null,
      fields: [],
      transitions: [
        {
          field: "status",
          states: ["fetching", "held"],
          edges: [
            { from: "fetching", to: "held" },
            { from: "held", to: "fetching" },
          ],
          terminal: [],
        },
      ],
    });
    const { edges: wires } = project("lifecycle", [cyclic], []);
    const entering = wires.filter((wire) => wire.from === cyclic.id);
    expect(entering).toHaveLength(1);
    expect(entering[0]?.to).toBe(stateId(cyclic, "status", "fetching"));
  });

  it("marks a terminal state on the node rather than only in the inspector", () => {
    const { nodes: drawn } = project("lifecycle", nodes, edges);
    const lost = drawn.find((node) => node.name === "lost");
    expect(lost?.detail).toEqual({ type: "enum", values: ["terminal"] });
    const available = drawn.find((node) => node.name === "available");
    expect(available?.detail).toEqual({ type: "enum", values: [] });
  });

  it("draws nothing for an entity whose lifecycle has no transitions", () => {
    const { nodes: drawn } = project("lifecycle", [withoutLifecycle], []);
    expect(drawn).toEqual([]);
  });

  it("gives every state a distinct id, across entities and fields", () => {
    const second = node("entity", "Loan", {
      type: "entity",
      kind: "internal",
      parent: null,
      fields: [],
      transitions: [
        {
          field: "status",
          states: ["available", "lost"],
          edges: [{ from: "available", to: "lost" }],
          terminal: [],
        },
      ],
    }, "lending");
    const { nodes: drawn } = project("lifecycle", [withLifecycle, second], []);
    const ids = drawn.map((n) => n.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("drops edges whose other end the view excluded", () => {
    // An arrow into empty space is worse than no arrow.
    const { edges: kept } = project("domain", nodes, edges);
    expect(kept).toHaveLength(2);
    expect(kept.every((edge) => !edge.from.includes("rule"))).toBe(true);
  });

  it("hides a module in the lifecycle view too", () => {
    expect(project("lifecycle", nodes, edges, new Set(["catalogue"])).nodes).toEqual([]);
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

describe("ownerOf", () => {
  it("resolves a state back to the entity that declares it", () => {
    // Selecting a pill has to land on something with source to show. A state is
    // a value in a transition list, so the entity is the nearest real construct.
    const id = stateId(withLifecycle, "status", "lost");
    expect(ownerOf(id)).toBe(withLifecycle.id);
  });

  it("leaves every other id alone", () => {
    for (const id of ["catalogue::rule::AddBook", "", "odd", "a::b::c::d"]) {
      expect(ownerOf(id)).toBe(id);
    }
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
