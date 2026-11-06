import { readFileSync } from "node:fs";
import { join } from "node:path";

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

describe("measure, for a pill", () => {
  const enumeration = (values: string[]): Node => ({
    id: "catalogue::enum::Medium",
    kind: "enum",
    name: "Medium",
    module: "catalogue",
    qualified: "catalogue/Medium",
    span: null,
    prose: { note: [], guidance: [] },
    detail: { type: "enum", values },
  });

  const box = (values: string[]): Node => ({
    ...enumeration(values),
    kind: "value",
    detail: { type: "enum", values },
  });

  it("gives an enum more room than a rectangle holding the same text", () => {
    // An enum is a stadium, so the corners a rectangle would have are inside
    // the curve. Measured as if it were square, the first and last row sit on
    // the outline — which is what a five-value enum looked like.
    const pill = measure(enumeration(["print", "audio"]));
    const rectangle = measure(box(["print", "audio"]));
    expect(pill.width).toBeGreaterThan(rectangle.width);
    expect(pill.height).toBeGreaterThan(rectangle.height);
  });

  it("grows with its values, so a long list is not squeezed into a circle", () => {
    const two = measure(enumeration(["print", "audio"]));
    const many = measure(enumeration(["a", "b", "c", "d", "e"]));
    expect(many.height).toBeGreaterThan(two.height);
  });

  it("stops paying for a stadium once it has stopped being one", () => {
    // A stadium's ends are semicircles of half its height, so the padding that
    // keeps the first and last row off the outline grows with the row count
    // until the box is mostly empty. Past `PILL_ROWS` the shape is a rounded
    // rectangle and the padding comes back down — the CSS switches at the same
    // count, and the two have to agree or ELK is told the wrong size.
    // The same longest value in both, so the width difference is the shape
    // alone: a pill pays for its curve and a rounded box does not.
    const pill = measure(enumeration(["same"]));
    const lens = measure(enumeration(["same", "b", "c", "d"]));
    expect(pill.width).toBeGreaterThan(lens.width);
    expect(lens.height).toBeGreaterThan(pill.height);
  });

  it("does not let one long value set the width of the canvas", () => {
    // The row is ellipsised in the CSS at the same cap this applies, and the
    // two have to agree: ELK spaces nodes by the size it is given, so a box
    // that draws wider than it measured overlaps its neighbour.
    const long = measure(enumeration(["a".repeat(200)]));
    expect(long.width).toBeLessThan(320);
  });

  it("still fits the name, however long, because that is what names it", () => {
    const named = measure({ ...enumeration(["x"]), name: "AVeryLongEnumerationName" });
    expect(named.width).toBeGreaterThan(measure(enumeration(["x"])).width);
  });
});

describe("the measured floor and the drawn floor", () => {
  // ELK routes every edge to the boundary of the size it was given, so a node
  // that draws narrower than it measured has its arrows stop short of it in
  // mid-air. That is what the pill padding did: it went into `measure` and the
  // CSS `min-width` stayed where it was, and every state machine in the
  // Lifecycle view came apart.
  //
  // The two cannot share a constant across the language boundary, so this reads
  // the stylesheet and checks they agree.
  const component = readFileSync(
    join(import.meta.dirname, "./ConstructBody.svelte"),
    "utf8",
  );

  /**
   * The `min-width` declared for `selector`, in pixels.
   *
   * Every block for the selector, not the first: a kind is declared twice —
   * once for its accent and once for its shape — and only one of the two
   * carries a floor.
   */
  function declaredFloor(selector: string): number {
    const blocks = component
      .split(`  ${selector} {`)
      .slice(1)
      .map((rest) => rest.slice(0, rest.indexOf("\n  }")));
    for (const block of blocks) {
      const found = /min-width:\s*(\d+)px/.exec(block);
      if (found?.[1]) {
        return Number(found[1]);
      }
    }
    throw new Error(`${selector} declares no min-width`);
  }

  /** The narrowest an enum of `values` can be measured at. */
  const floorOf = (values: string[]) =>
    measure({
      id: "m::enum::E",
      kind: "enum",
      name: "E",
      module: "m",
      qualified: "m/E",
      span: null,
      prose: { note: [], guidance: [] },
      detail: { type: "enum", values },
    }).width;

  it("measures a pill no wider than the CSS lets it draw", () => {
    expect(floorOf([])).toBe(declaredFloor(".kind-enum"));
  });

  it("measures a rounded enum no wider than the CSS lets it draw", () => {
    expect(floorOf(["a", "b", "c", "d"])).toBe(declaredFloor(".kind-enum.lens"));
  });

  it("leaves every other kind on the shared floor", () => {
    // 96px, which is `PADDING + MIN_CONTENT`. Only the enum pays for a shape.
    expect(declaredFloor(".construct")).toBe(
      measure({
        id: "m::value::V",
        kind: "value",
        name: "V",
        module: "m",
        qualified: "m/V",
        span: null,
        prose: { note: [], guidance: [] },
        detail: { type: "none" },
      }).width,
    );
  });
});

describe("routing when files are grouped", () => {
  // Two files. `Book` and `Copy` share one, so ELK measures their edge from
  // that file's corner; `Loan` is in the other, so its edge to `Copy` is
  // measured from the origin. Both conventions arrive in one list, and getting
  // that wrong draws arrowheads pointing at nothing.
  const grouped = () => {
    const nodes = [
      node({ id: "catalogue::entity::Book", kind: "entity", name: "Book" }),
      node({ id: "catalogue::entity::Copy", kind: "entity", name: "Copy" }),
      node({
        id: "lending::entity::Loan",
        kind: "entity",
        name: "Loan",
        module: "lending",
      }),
    ];
    const edges: Edge[] = [
      { from: "catalogue::entity::Book", to: "catalogue::entity::Copy", kind: "relationship", label: "", span: null },
      { from: "lending::entity::Loan", to: "catalogue::entity::Copy", kind: "relationship", label: "", span: null },
    ];
    return { nodes, edges };
  };

  /** An ELK that reproduces the two coordinate conventions ELK really uses. */
  const twoConventions: ElkLike = {
    async layout(graph) {
      // catalogue's container sits at (800, 10); lending's at (0, 200).
      const containers = (graph.children ?? []).map((child) => {
        const at = child.id.startsWith("catalogue") ? { x: 800, y: 10 } : { x: 0, y: 200 };
        return {
          ...child,
          ...at,
          width: 400,
          height: 300,
          children: (child.children ?? []).map((inner, index) => ({
            ...inner,
            x: 20,
            y: 20 + index * 100,
          })),
        };
      });
      return {
        ...graph,
        children: containers,
        edges: [
          // Within catalogue: measured from that container's corner. Book sits
          // at 800+20=820 absolute, so its own corner reads as 20.
          {
            id: "e0",
            sources: ["catalogue::entity::Book"],
            targets: ["catalogue::entity::Copy"],
            sections: [{ startPoint: { x: 20, y: 40 }, endPoint: { x: 20, y: 120 } }],
          },
          // Across files: measured from the origin. Loan is at 0+20=20.
          {
            id: "e1",
            sources: ["lending::entity::Loan"],
            targets: ["catalogue::entity::Copy"],
            sections: [{ startPoint: { x: 20, y: 220 }, endPoint: { x: 820, y: 130 } }],
          },
        ],
      };
    },
  };

  it("measures a within-file route from its container and a crossing one from the origin", async () => {
    const { nodes, edges } = grouped();
    const result = await layout(twoConventions, "domain", nodes, edges, true);

    // e0 was given (20,40) inside catalogue, which sits at (800,10).
    expect(result.routes.get(0)?.[0]).toEqual({ x: 820, y: 50 });
    // e1 was given (20,220) at the root, and must not be moved.
    expect(result.routes.get(1)?.[0]).toEqual({ x: 20, y: 220 });
  });

  it("reports each container so the boundary is ELK's, not a guess", async () => {
    const { nodes, edges } = grouped();
    const result = await layout(twoConventions, "domain", nodes, edges, true);
    expect(result.groups?.map((box) => `${box.module} ${box.x},${box.y}`).sort()).toEqual([
      "catalogue 800,10",
      "lending 0,200",
    ]);
  });
});
