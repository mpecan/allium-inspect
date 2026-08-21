import { describe, expect, it } from "vitest";

import type { Edge } from "../api/Edge";
import type { Node } from "../api/Node";
import type { NodeDetail } from "../api/NodeDetail";
import type { ViewKind } from "../client";
import { ANSWERS, inView, moduleId, ownerOf, project, stateId } from "./views";

const VIEWS: ViewKind[] = ["domain", "flow", "lifecycle", "chain", "modules"];

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
    prose: { note: [], guidance: [] },
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
    expect(inView(node("surface", "MemberShelf"), "chain")).toBe(true);
    expect(inView(node("actor", "Reader"), "chain")).toBe(true);
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
    expect(inView(node("external", "Phantom"), "chain")).toBe(false);
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

  it("hides a module's constructs but stands a box where its references arrive", () => {
    // The constructs go; the file does not vanish. Dropping the crossing edges
    // as well made whatever was left look self-contained, which is the one
    // answer a reader narrowing to a single module must not be given.
    const { nodes: kept, edges: keptEdges } = project(
      "domain",
      nodes,
      edges,
      new Set(["lending"]),
    );
    expect(kept.filter((n) => !n.id.endsWith("::module")).map((n) => n.name).sort()).toEqual([
      "Book",
      "Copy",
    ]);
    expect(kept.some((n) => n.id === moduleId("lending"))).toBe(true);
    // The one local edge, plus the crossing that now terminates on the box.
    expect(keptEdges).toHaveLength(2);
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

function wire(from: string, to: string): Edge {
  return { from, to, kind: "relationship", label: "", span: null };
}

describe("the modules view", () => {
  // One node per file, and edges weighted by how much crosses. Two modules
  // that reference each other both ways are a cycle, and a single edge running
  // back against many is the thing this view exists to surface.
  const graph = () => {
    const nodes = [
      node("entity", "Identity", { type: "none" }, "identity"),
      node("entity", "Device", { type: "none" }, "identity"),
      node("entity", "Group", { type: "none" }, "membership"),
      node("rule", "Join", { type: "none" }, "membership"),
      node("entity", "Message", { type: "none" }, "messaging"),
    ];
    const edges: Edge[] = [
      // membership leans on identity twice...
      wire("membership::entity::Group", "identity::entity::Identity"),
      wire("membership::rule::Join", "identity::entity::Device"),
      // ...and identity leans back exactly once. The interesting one.
      wire("identity::entity::Identity", "membership::entity::Group"),
      wire("messaging::entity::Message", "identity::entity::Identity"),
      // A reference that stays at home is not a crossing and must not appear.
      wire("membership::entity::Group", "membership::rule::Join"),
    ];
    return { nodes, edges };
  };

  it("draws one node per module, whatever it holds", () => {
    const { nodes, edges } = project("modules", graph().nodes, graph().edges);
    expect(nodes.map((n) => n.name)).toEqual(["identity", "membership", "messaging"]);
    expect(edges.every((e) => e.from !== e.to)).toBe(true);
  });

  it("counts what each file holds and how far it reaches", () => {
    const { nodes } = project("modules", graph().nodes, graph().edges);
    const identity = nodes.find((n) => n.name === "identity");
    expect(identity?.detail.type).toBe("config");
    if (identity?.detail.type !== "config") {
      throw new Error("the census renders through the config rows");
    }
    const census = Object.fromEntries(
      identity.detail.parameters.map((p) => [p.name, p.type_expr]),
    );
    expect(census).toEqual({
      constructs: "2",
      "references out": "1",
      "referenced by": "3",
    });
  });

  it("weights each crossing and keeps the two directions apart", () => {
    const { edges } = project("modules", graph().nodes, graph().edges);
    const weights = Object.fromEntries(
      edges.map((e) => [`${e.from} ${e.to}`, e.label]),
    );
    // Sixteen-against-one is the real shape this reproduces in miniature: the
    // pair is a cycle, and collapsing the directions would hide that one of
    // them is an exception.
    expect(weights[`${moduleId("membership")} ${moduleId("identity")}`]).toBe("2");
    expect(weights[`${moduleId("identity")} ${moduleId("membership")}`]).toBe("1");
    expect(weights[`${moduleId("messaging")} ${moduleId("identity")}`]).toBe("1");
    // Five edges in, three crossings out: the within-module one is not a
    // relationship between files.
    expect(edges).toHaveLength(3);
  });

  it("leaves out a module the rail switched off, and the crossings to it", () => {
    const { nodes, edges } = project(
      "modules",
      graph().nodes,
      graph().edges,
      new Set(["messaging"]),
    );
    expect(nodes.map((n) => n.name)).toEqual(["identity", "membership"]);
    // `messaging -> identity` had one end hidden. Drawing it would be an arrow
    // out of empty space.
    expect(edges.some((e) => e.from.startsWith("messaging"))).toBe(false);
    expect(edges).toHaveLength(2);
  });

  it("gives a module node no span, because a file is not a declaration", () => {
    const { nodes } = project("modules", graph().nodes, graph().edges);
    expect(nodes.every((n) => n.span === null)).toBe(true);
  });

  it("leaves a module id alone when asked what construct it belongs to", () => {
    // Unlike a lifecycle state, a module maps to no construct — there is no
    // narrower thing to select.
    expect(ownerOf(moduleId("identity"))).toBe(moduleId("identity"));
  });
});

describe("switched-off modules, as the boxes their references arrive from", () => {
  const spread = () => ({
    nodes: [
      node("entity", "Identity", { type: "none" }, "identity"),
      node("entity", "Group", { type: "none" }, "membership"),
      node("entity", "Roster", { type: "none" }, "membership"),
      node("entity", "Message", { type: "none" }, "messaging"),
    ],
    edges: [
      wire("membership::entity::Group", "identity::entity::Identity"),
      // The same construct reaching into the same file twice is one
      // relationship, not two arrows.
      wire("membership::entity::Group", "identity::entity::Identity"),
      wire("membership::entity::Roster", "identity::entity::Identity"),
      // Arriving from outside, which must terminate too.
      wire("messaging::entity::Message", "membership::entity::Group"),
    ],
  });

  it("draws nothing extra while every module is on", () => {
    const { nodes } = project("domain", spread().nodes, spread().edges);
    expect(nodes.some((n) => n.id.endsWith("::module"))).toBe(false);
  });

  it("terminates a reference that leaves the drawing on the file it went to", () => {
    // Narrowed to one file. Dropping these edges made `membership` look
    // self-contained, which is the most misleading answer available.
    const { nodes, edges } = project(
      "domain",
      spread().nodes,
      spread().edges,
      new Set(["identity", "messaging"]),
    );

    expect(nodes.filter((n) => n.id.endsWith("::module")).map((n) => n.name)).toEqual([
      "identity",
      "messaging",
    ]);
    // Group and Roster each reach identity once, and Group is reached from
    // messaging once: three lines, not four.
    expect(edges).toHaveLength(3);
    expect(edges.filter((e) => e.to === moduleId("identity"))).toHaveLength(2);
    expect(edges.filter((e) => e.from === moduleId("messaging"))).toHaveLength(1);
  });

  it("leaves out a relationship between two files nobody is looking at", () => {
    // `messaging -> identity` has both ends hidden. Drawing it would answer a
    // question the reader did not ask, on a canvas about `membership`.
    const nodes = [
      node("entity", "Group", { type: "none" }, "membership"),
      node("entity", "Identity", { type: "none" }, "identity"),
      node("entity", "Message", { type: "none" }, "messaging"),
    ];
    const edges = [wire("messaging::entity::Message", "identity::entity::Identity")];
    const drawn = project("domain", nodes, edges, new Set(["identity", "messaging"]));
    expect(drawn.nodes.map((n) => n.name)).toEqual(["Group"]);
    expect(drawn.edges).toEqual([]);
  });

  it("gives a destination box no census, unlike the modules view", () => {
    const { nodes } = project("domain", spread().nodes, spread().edges, new Set(["identity"]));
    const port = nodes.find((n) => n.id === moduleId("identity"));
    expect(port?.detail.type).toBe("none");
  });
});
