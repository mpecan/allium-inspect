import { describe, expect, it } from "vitest";

import type { Edge } from "../api/Edge";
import type { Node } from "../api/Node";
import type { NodeKind } from "../api/NodeKind";
import { familyOf, layout, measure, optionsFor } from "./layout";
import type { ElkLike } from "./layout";
import { summaryRows } from "./rows";

function node(partial: Partial<Node> & Pick<Node, "id" | "kind" | "name">): Node {
  return {
    module: "catalogue",
    qualified: `catalogue/${partial.name}`,
    span: null,
    detail: { type: "none" },
    prose: { note: [], guidance: [] },
    ...partial,
  } as Node;
}

function entityOf(
  name: string,
  fields: { name: string; type_expr: string }[],
): Node {
  return node({
    id: `catalogue::entity::${name}`,
    kind: "entity",
    name,
    detail: {
      type: "entity",
      kind: "internal",
      parent: null,
      transitions: [],
      fields: fields.map((field) => ({
        ...field,
        enum_values: [],
        derived: false,
        relationship: false,
        when: null,
      note: [],
      })),
    },
  });
}

function edge(from: string, to: string): Edge {
  return { from, to, kind: "field", label: "x", span: null };
}

/** An ELK that stacks nodes vertically, so positions are predictable. */
const stackingElk: ElkLike = {
  async layout(graph) {
    return {
      ...graph,
      children: (graph.children ?? []).map((child, index) => ({
        ...child,
        x: 10,
        y: index * 100,
      })),
    };
  },
};

describe("measure", () => {
  it("sizes a node from its label so nothing is clipped", () => {
    const short = measure(node({ id: "a", kind: "entity", name: "Book" }));
    const long = measure(
      node({ id: "b", kind: "entity", name: "ReconciliationRequested" }),
    );
    expect(long.width).toBeGreaterThan(short.width);
  });

  it("floors the width so a one-letter name is still a box", () => {
    expect(measure(node({ id: "a", kind: "entity", name: "A" })).width).toBe(96);
  });

  it("never clips a name, however long", () => {
    // One entity with a long field list should not set the scale for the whole
    // canvas — but a construct whose name is cut off cannot be identified, and
    // identifying it is the one thing a box on a graph is for.
    // `AttestersAreDistinctIdentities` is a real invariant in a real spec.
    const long = "AttestersAreDistinctIdentities";
    const box = measure(node({ id: "a", kind: "invariant", name: long }));
    expect(box.width).toBeGreaterThanOrEqual(long.length * 9);
  });

  it("caps how wide the rows can make a node", () => {
    // The value column ellipsises, so a forty-character type name costs
    // nothing. Only the labels can grow the box, and they stop.
    const wide = entityOf(
      "Book",
      Array.from({ length: 3 }, (_, index) => ({
        name: `${"field_name".repeat(6)}${index}`,
        type_expr: "String".repeat(10),
      })),
    );
    expect(measure(wide).width).toBeLessThanOrEqual(260);
  });

  it("grows taller with each row it will show", () => {
    const bare = measure(entityOf("Book", []));
    const full = measure(entityOf("Book", [
      { name: "title", type_expr: "String" },
      { name: "medium", type_expr: "Medium" },
    ]));
    expect(full.height).toBeGreaterThan(bare.height);
  });

  it("reserves a row for every row the node will actually draw", () => {
    // The one property that matters: ELK packs to the sizes it is given, so a
    // node measured smaller than it is drawn overlaps its neighbour. Asking
    // `summaryRows` — the same function the component asks — is what keeps the
    // two from drifting apart.
    const fields = Array.from({ length: 40 }, (_, index) => ({
      name: `field_${index}`,
      type_expr: "String",
    }));
    const many = entityOf("Book", fields);
    const few = entityOf("Book", fields.slice(0, 2));
    const perRow =
      (measure(many).height - measure(few).height) /
      (summaryRows(many).length - summaryRows(few).length);
    expect(perRow).toBeGreaterThan(10);
    expect(measure(many).height).toBe(
      measure(entityOf("Book", [])).height + summaryRows(many).length * perRow,
    );
  });

  it("widens for a long row label, which is not ellipsised", () => {
    const terse = entityOf("Book", [{ name: "id", type_expr: "String" }]);
    const wordy = entityOf("Book", [
      { name: "acknowledged_by_the_lender", type_expr: "String" },
    ]);
    expect(measure(wordy).width).toBeGreaterThan(measure(terse).width);
  });
});

describe("optionsFor", () => {
  it("separates one entity's machine from the next by more than it separates states", () => {
    // The lifecycle view is a page of disconnected machines. If the gap between
    // two machines is the same as the gap between two states of one machine,
    // the reader cannot tell which pills belong together.
    const options = optionsFor("lifecycle");
    expect(options["elk.separateConnectedComponents"]).toBe("true");
    expect(Number(options["elk.spacing.componentComponent"])).toBeGreaterThan(
      Number(options["elk.spacing.nodeNode"]) * 2,
    );
  });

  it("reads every view in the direction the language writes it", () => {
    for (const view of ["domain", "flow", "lifecycle", "chain"] as const) {
      expect(optionsFor(view)["elk.direction"]).toBe("RIGHT");
    }
  });

  it("pins the seed so the same graph lands the same way twice", () => {
    // Without it a shared link and a screenshot diff both stop meaning
    // anything, because the picture moves between runs.
    expect(optionsFor("domain")["elk.randomSeed"]).toBe("1");
  });
});

describe("layout", () => {
  const nodes = [
    node({ id: "a", kind: "entity", name: "Book" }),
    node({ id: "b", kind: "entity", name: "Copy" }),
  ];

  it("places every node it was given", async () => {
    const result = await layout(stackingElk, "domain", nodes, [edge("a", "b")]);
    expect(result.nodes.map((n) => n.id)).toEqual(["a", "b"]);
    expect(result.nodes[0]).toMatchObject({ x: 10, y: 0 });
    expect(result.nodes[1]).toMatchObject({ x: 10, y: 100 });
  });

  it("reports the extent of what it placed", async () => {
    const result = await layout(stackingElk, "domain", nodes, []);
    expect(result.height).toBe(100 + result.nodes[1]!.height);
    expect(result.width).toBe(10 + result.nodes[0]!.width);
  });

  it("returns nothing for an empty view without calling the engine", async () => {
    const failing: ElkLike = {
      layout: () => Promise.reject(new Error("must not be called")),
    };
    await expect(layout(failing, "domain", [], [])).resolves.toEqual({
      nodes: [],
      routes: new Map(),
      width: 0,
      height: 0,
    });
  });

  it("drops edges whose endpoints this view filtered out", async () => {
    // A view is a filter, so dangling edges are expected rather than
    // exceptional — and ELK rejects a graph that contains one.
    let seen = -1;
    const counting: ElkLike = {
      async layout(graph) {
        seen = graph.edges?.length ?? 0;
        return { ...graph, children: (graph.children ?? []).map((c) => ({ ...c, x: 0, y: 0 })) };
      },
    };
    await layout(counting, "flow", nodes, [edge("a", "b"), edge("a", "gone")]);
    expect(seen).toBe(1);
  });

  it("still shows every node when layout fails", async () => {
    // A canvas that cannot lay itself out should show what it holds. Returning
    // nothing would read as an empty spec.
    const failing: ElkLike = {
      layout: () => Promise.reject(new Error("elk exploded")),
    };
    const result = await layout(failing, "domain", nodes, []);
    expect(result.nodes).toHaveLength(2);
    const places = result.nodes.map((n) => `${n.x},${n.y}`);
    expect(new Set(places).size).toBe(2);
  });

  it("falls back per node when the engine returns one without coordinates", async () => {
    // Two nodes at the origin sit on top of each other and read as one.
    const partial: ElkLike = {
      async layout(graph) {
        return {
          ...graph,
          children: (graph.children ?? []).map((child, index) =>
            index === 0 ? { ...child, x: 5, y: 5 } : { ...child },
          ),
        };
      },
    };
    const result = await layout(partial, "domain", nodes, []);
    expect(result.nodes[0]).toMatchObject({ x: 5, y: 5 });
    const second = result.nodes[1]!;
    expect(second.x === 5 && second.y === 5).toBe(false);
  });
});

describe("familyOf", () => {
  it("groups the four things a spec is made of", () => {
    const of = (kinds: NodeKind[]) => kinds.map(familyOf);
    expect(of(["entity", "value", "variant", "enum"])).toEqual([
      "thing",
      "thing",
      "thing",
      "thing",
    ]);
    expect(of(["rule", "trigger"])).toEqual(["behaviour", "behaviour"]);
    expect(of(["surface", "actor"])).toEqual(["boundary", "boundary"]);
    expect(of(["invariant", "config"])).toEqual(["constraint", "constraint"]);
  });

  it("covers every kind the server can send", () => {
    // A kind with no family would fall through the switch and render
    // unstyled, which reads as a rendering bug rather than a new construct.
    const every: NodeKind[] = [
      "entity", "value", "variant", "enum", "rule", "trigger",
      "surface", "actor", "config", "invariant", "external",
    ];
    for (const kind of every) {
      expect(familyOf(kind)).toBeTypeOf("string");
    }
  });

  it("marks an unresolved reference as its own family", () => {
    // It gets no fill on the canvas, so it reads as an absence rather than as
    // another kind of construct.
    expect(familyOf("external")).toBe("unresolved");
  });
});

describe("edge routing", () => {
  it("asks the engine to route the edges, not only place the nodes", () => {
    // Without this the canvas draws a bezier from handle to handle, straight
    // across whatever nodes lie between — which is what a dense view looks like
    // when nobody asked the layout engine the second question.
    for (const view of ["domain", "flow", "lifecycle", "chain"] as const) {
      expect(optionsFor(view)["elk.edgeRouting"]).toBe("ORTHOGONAL");
    }
  });

  it("hands back each route against the edge it was given", async () => {
    // Keyed by position in the *given* list, so that filtering some edges out
    // before layout does not shift every later route onto the wrong edge.
    const nodes = [
      node({ id: "a", kind: "entity", name: "A" }),
      node({ id: "b", kind: "entity", name: "B" }),
    ];
    const edges = [
      edge("a", "gone"),
      edge("a", "b"),
    ];
    const engine: ElkLike = {
      layout: (graph) =>
        Promise.resolve({
          ...graph,
          children: graph.children?.map((child) => ({ ...child, x: 0, y: 0 })),
          edges: graph.edges?.map((given) => ({
            ...given,
            sections: [
              { startPoint: { x: 1, y: 2 }, bendPoints: [{ x: 5, y: 2 }], endPoint: { x: 5, y: 9 } },
            ],
          })),
        }),
    };

    const placed = await layout(engine, "domain", nodes, edges);
    expect(placed.routes.get(0)).toBeUndefined();
    expect(placed.routes.get(1)).toEqual([
      { x: 1, y: 2 },
      { x: 5, y: 2 },
      { x: 5, y: 9 },
    ]);
  });

  it("has no routes when the layout failed and the grid took over", async () => {
    // The grid has no opinion about where an edge runs, and inventing one would
    // draw confident lines through a fallback nobody should trust.
    const broken: ElkLike = { layout: () => Promise.reject(new Error("no")) };
    const placed = await layout(broken, "domain", [node({ id: "a", kind: "entity", name: "A" })], []);
    expect(placed.routes.size).toBe(0);
    expect(placed.nodes).toHaveLength(1);
  });
});
