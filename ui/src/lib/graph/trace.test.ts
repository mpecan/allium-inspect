import { describe, expect, it } from "vitest";

import type { Edge } from "../api/Edge";
import type { EdgeKind } from "../api/EdgeKind";
import { chain, isMeaningful, narrow, neighbourhood, origins, walk } from "./trace";

function edge(from: string, to: string, kind: EdgeKind = "triggers"): Edge {
  return { from, to, kind, label: `${from}->${to}`, span: null };
}

/**
 * A borrowing chain, as the fixtures actually shape one:
 *
 *   MemberShelf -provides-> MemberBorrows -triggers-> BorrowCopy
 *     BorrowCopy -creates-> Loan
 *     BorrowCopy -emits-> CopyBorrowed -triggers-> NotifyDesk
 *
 * plus one non-causal edge, so the filtering has something to exclude.
 */
const borrowing: Edge[] = [
  edge("surface:MemberShelf", "trigger:MemberBorrows", "provides"),
  edge("trigger:MemberBorrows", "rule:BorrowCopy", "triggers"),
  edge("rule:BorrowCopy", "entity:Loan", "creates"),
  edge("rule:BorrowCopy", "trigger:CopyBorrowed", "emits"),
  edge("trigger:CopyBorrowed", "rule:NotifyDesk", "triggers"),
  edge("entity:Loan", "entity:Member", "field"),
];

describe("walk", () => {
  it("includes the node it started from", () => {
    const trace = walk(borrowing, "rule:BorrowCopy");
    expect(trace.nodes.has("rule:BorrowCopy")).toBe(true);
  });

  it("follows edges forward", () => {
    const trace = walk(borrowing, "trigger:MemberBorrows", { direction: "forward" });
    expect([...trace.nodes]).toContain("rule:BorrowCopy");
    expect([...trace.nodes]).not.toContain("surface:MemberShelf");
  });

  it("follows edges backward", () => {
    const trace = walk(borrowing, "rule:BorrowCopy", { direction: "backward" });
    expect([...trace.nodes]).toContain("trigger:MemberBorrows");
    expect([...trace.nodes]).toContain("surface:MemberShelf");
    expect([...trace.nodes]).not.toContain("entity:Loan");
  });

  it("follows both directions when asked", () => {
    const trace = walk(borrowing, "rule:BorrowCopy", { direction: "both", depth: 1 });
    expect([...trace.nodes].sort()).toEqual([
      "entity:Loan",
      "rule:BorrowCopy",
      "trigger:CopyBorrowed",
      "trigger:MemberBorrows",
    ]);
  });

  it("stops at the depth it was given", () => {
    const one = walk(borrowing, "surface:MemberShelf", { depth: 1 });
    expect(one.nodes.size).toBe(2);
    expect(one.depth).toBe(1);

    const two = walk(borrowing, "surface:MemberShelf", { depth: 2 });
    expect([...two.nodes]).toContain("rule:BorrowCopy");
    expect([...two.nodes]).not.toContain("entity:Loan");
  });

  it("restricts itself to the edge kinds it was given", () => {
    const trace = walk(borrowing, "rule:BorrowCopy", { kinds: ["creates"] });
    expect([...trace.nodes].sort()).toEqual(["entity:Loan", "rule:BorrowCopy"]);
  });

  it("records the edges it followed, for drawing the path", () => {
    const trace = walk(borrowing, "trigger:MemberBorrows", { depth: 1 });
    expect(trace.edges.size).toBe(1);
    expect([...trace.edges][0]?.to).toBe("rule:BorrowCopy");
  });

  it("terminates on a cycle", () => {
    // Ordinary in a real spec: a message is queued, delivery retries, the
    // retry re-queues. A naive walk over one does not come back.
    const loop = [
      edge("a", "b"),
      edge("b", "c"),
      edge("c", "a"),
    ];
    const trace = walk(loop, "a");
    expect([...trace.nodes].sort()).toEqual(["a", "b", "c"]);
  });

  it("reports a node with no edges as a trace of one", () => {
    const trace = walk(borrowing, "entity:Orphan");
    expect([...trace.nodes]).toEqual(["entity:Orphan"]);
    expect(trace.depth).toBe(0);
  });

  it("walks an empty edge set without complaint", () => {
    expect(walk([], "a").nodes.size).toBe(1);
  });
});

describe("chain", () => {
  it("follows the causal chain from a surface operation to its end", () => {
    const trace = chain(borrowing, "surface:MemberShelf");
    expect([...trace.nodes].sort()).toEqual([
      "entity:Loan",
      "rule:BorrowCopy",
      "rule:NotifyDesk",
      "surface:MemberShelf",
      "trigger:CopyBorrowed",
      "trigger:MemberBorrows",
    ]);
  });

  it("does not wander into edges that are true but not causal", () => {
    // `Loan.member: Member` is a fact about the domain, not something that
    // happens next. Following it would make every trace reach everything.
    const trace = chain(borrowing, "surface:MemberShelf");
    expect([...trace.nodes]).not.toContain("entity:Member");
  });

  it("is bounded, so a cyclic spec cannot hang the canvas", () => {
    const loop = [edge("a", "b"), edge("b", "a")];
    expect(chain(loop, "a", 3).nodes.size).toBe(2);
  });
});

describe("origins", () => {
  it("answers what had to happen for something to be reached", () => {
    const trace = origins(borrowing, "rule:NotifyDesk");
    expect([...trace.nodes].sort()).toEqual([
      "rule:BorrowCopy",
      "rule:NotifyDesk",
      "surface:MemberShelf",
      "trigger:CopyBorrowed",
      "trigger:MemberBorrows",
    ]);
  });

  it("reports nothing upstream of an entry point", () => {
    expect(origins(borrowing, "surface:MemberShelf").nodes.size).toBe(1);
  });
});

describe("neighbourhood", () => {
  it("is one step in both directions, following any edge kind", () => {
    const trace = neighbourhood(borrowing, "entity:Loan");
    expect([...trace.nodes].sort()).toEqual([
      "entity:Loan",
      "entity:Member",
      "rule:BorrowCopy",
    ]);
  });
});

describe("isMeaningful", () => {
  it("rejects a trace of one node", () => {
    // Dimming the whole canvas to highlight a single box tells the reader
    // nothing and costs them the context.
    expect(isMeaningful(walk(borrowing, "entity:Orphan"))).toBe(false);
  });

  it("accepts a trace that actually goes somewhere", () => {
    expect(isMeaningful(chain(borrowing, "surface:MemberShelf"))).toBe(true);
  });
});

describe("narrow", () => {
  const nodes = [
    { id: "surface:MemberShelf" },
    { id: "trigger:MemberBorrows" },
    { id: "rule:BorrowCopy" },
    { id: "entity:Loan" },
    { id: "trigger:CopyBorrowed" },
    { id: "rule:NotifyDesk" },
    { id: "entity:Member" },
    { id: "rule:Unrelated" },
  ];

  it("leaves the view alone when nothing is being traced", () => {
    // Reflow is opt-in, and off it must be exactly as if it were not there.
    const same = narrow(nodes, borrowing, null);
    expect(same.nodes).toBe(nodes);
    expect(same.edges).toBe(borrowing);
  });

  it("keeps only what the trace reached", () => {
    const trace = chain(borrowing, "surface:MemberShelf");
    const kept = narrow(nodes, borrowing, trace).nodes.map((node) => node.id);
    expect(kept).not.toContain("rule:Unrelated");
    expect(kept).toContain("rule:NotifyDesk");
    expect(kept).toHaveLength(trace.nodes.size);
  });

  it("keeps every edge between the nodes it kept, not only the ones walked", () => {
    // `Loan -field-> Member` is not causal, so the walk did not follow it. Both
    // ends are on the chain, though, and how the traced constructs relate is
    // part of the answer — dropping it would draw a chain simpler than the spec.
    const trace = neighbourhood(borrowing, "entity:Loan");
    const { edges } = narrow(nodes, borrowing, trace);
    expect(edges.map((edge) => edge.label)).toContain("entity:Loan->entity:Member");
  });

  it("drops an edge with only one end on the chain", () => {
    // An arrow into empty space is worse than no arrow, and reflow creates
    // exactly that opportunity by removing the other end.
    const trace = chain(borrowing, "trigger:CopyBorrowed");
    const { nodes: kept, edges } = narrow(nodes, borrowing, trace);
    const present = new Set(kept.map((node) => node.id));
    expect(edges.length).toBeGreaterThan(0);
    for (const edge of edges) {
      expect(present.has(edge.from) && present.has(edge.to)).toBe(true);
    }
  });
});
