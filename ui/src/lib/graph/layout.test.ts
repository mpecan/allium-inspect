import { describe, expect, it } from "vitest";

import type { Edge } from "../api/Edge";
import type { Node } from "../api/Node";
import type { NodeKind } from "../api/NodeKind";
import { familyOf, layout, measure, optionsFor, rowsOf } from "./layout";
import type { ElkLike } from "./layout";

function node(partial: Partial<Node> & Pick<Node, "id" | "kind" | "name">): Node {
  return {
    module: "catalogue",
    qualified: `catalogue/${partial.name}`,
    span: null,
    detail: { type: "none" },
    ...partial,
  } as Node;
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

  it("caps the width so one long name does not set the canvas scale", () => {
    const huge = measure(
      node({ id: "a", kind: "entity", name: "A".repeat(200) }),
    );
    expect(huge.width).toBe(260);
  });

  it("grows with the rows it will show", () => {
    const bare = measure(node({ id: "a", kind: "entity", name: "Book" }), 0);
    const full = measure(node({ id: "a", kind: "entity", name: "Book" }), 5);
    expect(full.height).toBeGreaterThan(bare.height);
  });

  it("caps the rows so a wide entity does not become a column", () => {
    const eight = measure(node({ id: "a", kind: "entity", name: "Book" }), 8);
    const forty = measure(node({ id: "a", kind: "entity", name: "Book" }), 40);
    expect(forty.height).toBe(eight.height);
  });
});

describe("rowsOf", () => {
  it("counts an entity's fields", () => {
    const entity = node({
      id: "a",
      kind: "entity",
      name: "Book",
      detail: {
        type: "entity",
        kind: "internal",
        parent: null,
        transitions: [],
        fields: [
          { name: "title", type_expr: "String", enum_values: [], derived: false, relationship: false, when: null },
          { name: "medium", type_expr: "Medium", enum_values: [], derived: false, relationship: false, when: null },
        ],
      },
    });
    expect(rowsOf(entity)).toBe(2);
  });

  it("counts an enum's values and a config's parameters", () => {
    expect(
      rowsOf(node({ id: "a", kind: "enum", name: "Medium", detail: { type: "enum", values: ["print", "audio"] } })),
    ).toBe(2);
    expect(
      rowsOf(
        node({
          id: "b",
          kind: "config",
          name: "config",
          detail: { type: "config", parameters: [{ name: "loan_limit", type_expr: "Integer", default_expr: "5" }] },
        }),
      ),
    ).toBe(1);
  });

  it("caps a rule at four rows because a clause is a sentence", () => {
    const clauses = Array.from({ length: 9 }, () => ({
      keyword: "requires",
      text: "x",
      span: null,
    }));
    const rule = node({
      id: "a",
      kind: "rule",
      name: "BorrowCopy",
      detail: { type: "rule", trigger: "T", source: "external", clauses, creates: [], emits: [] },
    });
    expect(rowsOf(rule)).toBe(4);
  });

  it("gives a node with no detail no rows", () => {
    expect(rowsOf(node({ id: "a", kind: "external", name: "Phantom" }))).toBe(0);
  });
});

describe("optionsFor", () => {
  it("reads a lifecycle downward, from initial state to terminal", () => {
    expect(optionsFor("lifecycle")["elk.direction"]).toBe("DOWN");
  });

  it("reads a causal chain in the direction the language writes it", () => {
    for (const view of ["domain", "flow", "journey"] as const) {
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
