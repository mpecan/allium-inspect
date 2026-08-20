import { describe, expect, it } from "vitest";

import type { Edge } from "../api/Edge";
import type { EdgeKind } from "../api/EdgeKind";
import { isMeaningful, journey, neighbourhood, origins, walk } from "./trace";

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
const chain: Edge[] = [
  edge("surface:MemberShelf", "trigger:MemberBorrows", "provides"),
  edge("trigger:MemberBorrows", "rule:BorrowCopy", "triggers"),
  edge("rule:BorrowCopy", "entity:Loan", "creates"),
  edge("rule:BorrowCopy", "trigger:CopyBorrowed", "emits"),
  edge("trigger:CopyBorrowed", "rule:NotifyDesk", "triggers"),
  edge("entity:Loan", "entity:Member", "field"),
];

describe("walk", () => {
  it("includes the node it started from", () => {
    const trace = walk(chain, "rule:BorrowCopy");
    expect(trace.nodes.has("rule:BorrowCopy")).toBe(true);
  });

  it("follows edges forward", () => {
    const trace = walk(chain, "trigger:MemberBorrows", { direction: "forward" });
    expect([...trace.nodes]).toContain("rule:BorrowCopy");
    expect([...trace.nodes]).not.toContain("surface:MemberShelf");
  });

  it("follows edges backward", () => {
    const trace = walk(chain, "rule:BorrowCopy", { direction: "backward" });
    expect([...trace.nodes]).toContain("trigger:MemberBorrows");
    expect([...trace.nodes]).toContain("surface:MemberShelf");
    expect([...trace.nodes]).not.toContain("entity:Loan");
  });

  it("follows both directions when asked", () => {
    const trace = walk(chain, "rule:BorrowCopy", { direction: "both", depth: 1 });
    expect([...trace.nodes].sort()).toEqual([
      "entity:Loan",
      "rule:BorrowCopy",
      "trigger:CopyBorrowed",
      "trigger:MemberBorrows",
    ]);
  });

  it("stops at the depth it was given", () => {
    const one = walk(chain, "surface:MemberShelf", { depth: 1 });
    expect(one.nodes.size).toBe(2);
    expect(one.depth).toBe(1);

    const two = walk(chain, "surface:MemberShelf", { depth: 2 });
    expect([...two.nodes]).toContain("rule:BorrowCopy");
    expect([...two.nodes]).not.toContain("entity:Loan");
  });

  it("restricts itself to the edge kinds it was given", () => {
    const trace = walk(chain, "rule:BorrowCopy", { kinds: ["creates"] });
    expect([...trace.nodes].sort()).toEqual(["entity:Loan", "rule:BorrowCopy"]);
  });

  it("records the edges it followed, for drawing the path", () => {
    const trace = walk(chain, "trigger:MemberBorrows", { depth: 1 });
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
    const trace = walk(chain, "entity:Orphan");
    expect([...trace.nodes]).toEqual(["entity:Orphan"]);
    expect(trace.depth).toBe(0);
  });

  it("walks an empty edge set without complaint", () => {
    expect(walk([], "a").nodes.size).toBe(1);
  });
});

describe("journey", () => {
  it("follows the causal chain from a surface operation to its end", () => {
    const trace = journey(chain, "surface:MemberShelf");
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
    const trace = journey(chain, "surface:MemberShelf");
    expect([...trace.nodes]).not.toContain("entity:Member");
  });

  it("is bounded, so a cyclic spec cannot hang the canvas", () => {
    const loop = [edge("a", "b"), edge("b", "a")];
    expect(journey(loop, "a", 3).nodes.size).toBe(2);
  });
});

describe("origins", () => {
  it("answers what had to happen for something to be reached", () => {
    const trace = origins(chain, "rule:NotifyDesk");
    expect([...trace.nodes].sort()).toEqual([
      "rule:BorrowCopy",
      "rule:NotifyDesk",
      "surface:MemberShelf",
      "trigger:CopyBorrowed",
      "trigger:MemberBorrows",
    ]);
  });

  it("reports nothing upstream of an entry point", () => {
    expect(origins(chain, "surface:MemberShelf").nodes.size).toBe(1);
  });
});

describe("neighbourhood", () => {
  it("is one step in both directions, following any edge kind", () => {
    const trace = neighbourhood(chain, "entity:Loan");
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
    expect(isMeaningful(walk(chain, "entity:Orphan"))).toBe(false);
  });

  it("accepts a trace that actually goes somewhere", () => {
    expect(isMeaningful(journey(chain, "surface:MemberShelf"))).toBe(true);
  });
});
