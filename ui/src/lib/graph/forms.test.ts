import { describe, expect, it } from "vitest";

import type { Edge } from "../api/Edge";
import type { EdgeKind } from "../api/EdgeKind";
import { applies, FORMS, walkForm, type Form } from "./forms";
import type { Node } from "../api/Node";

function edge(from: string, to: string, kind: EdgeKind = "triggers"): Edge {
  return { from, to, kind, label: `${from}->${to}`, span: null };
}

/** MemberShelf -> MemberBorrows -> BorrowCopy -> Loan, plus a field edge. */
const chain: Edge[] = [
  edge("surface:MemberShelf", "trigger:MemberBorrows", "provides"),
  edge("trigger:MemberBorrows", "rule:BorrowCopy", "triggers"),
  edge("rule:BorrowCopy", "entity:Loan", "creates"),
  edge("entity:Loan", "entity:Member", "field"),
];

const reached = (form: Form, id: string) => [...walkForm(form, chain, id).nodes].sort();

describe("walkForm", () => {
  it("follows the chain forward", () => {
    expect(reached("forward", "trigger:MemberBorrows")).toEqual([
      "entity:Loan",
      "rule:BorrowCopy",
      "trigger:MemberBorrows",
    ]);
  });

  it("follows the chain backward", () => {
    expect(reached("backward", "entity:Loan")).toEqual([
      "entity:Loan",
      "rule:BorrowCopy",
      "surface:MemberShelf",
      "trigger:MemberBorrows",
    ]);
  });

  it("goes one step either way, along any kind of edge", () => {
    // Adjacent is not restricted to causal edges: `Loan.member` is how these
    // two constructs are connected, and a reader asking what is next to a loan
    // means that as much as they mean what created it.
    expect(reached("near", "entity:Loan")).toEqual([
      "entity:Loan",
      "entity:Member",
      "rule:BorrowCopy",
    ]);
  });

  it("always includes the construct that was asked about", () => {
    // The pop-up is a view *of* that construct. A form that dropped it would
    // answer a question about something the reader cannot see.
    for (const { form } of FORMS) {
      expect(walkForm(form, chain, "rule:BorrowCopy").nodes.has("rule:BorrowCopy")).toBe(true);
    }
  });

  it("offers a label and a plain-language hint for each form", () => {
    // "Adjacent" alone does not say what it will show, and the reader is one
    // click from finding out the expensive way.
    expect(FORMS).toHaveLength(4);
    for (const option of FORMS) {
      expect(option.label.length).toBeGreaterThan(3);
      expect(option.hint.length).toBeGreaterThan(8);
    }
  });

  it("phrases the empty case as a sentence about the construct", () => {
    // It is read as `Nothing in the spec ${empty} Attestation`, so it has to end
    // where a name goes and not read as a question or a fragment.
    for (const option of FORMS) {
      const sentence = `Nothing in the spec ${option.empty} Attestation.`;
      expect(sentence).toMatch(/^Nothing in the spec [a-z ]+ Attestation\.$/);
      expect(option.empty).not.toContain("what");
    }
  });

  it("leads with the form that answers for any connected construct", () => {
    // Forward from a terminal entity is empty, and so is backward from a
    // surface. Adjacent is empty only for something joined to nothing, which
    // makes it the one worth opening on.
    expect(FORMS[0]?.form).toBe("near");
    expect(walkForm("forward", chain, "entity:Member").nodes.size).toBe(1);
    expect(walkForm("near", chain, "entity:Member").nodes.size).toBeGreaterThan(1);
  });
});

describe("the direction each label promises", () => {
  it("names the end of the chain the reader will be shown", () => {
    // A button reading "Leads to" beside a construct is read as "this leads to
    // …", which is the *forward* direction — so it named the wrong one. Each
    // label now has to survive being read as a sentence about the construct.
    const forward = FORMS.find((option) => option.form === "forward");
    const backward = FORMS.find((option) => option.form === "backward");
    expect(forward?.label).toBe("Follows");
    expect(backward?.label).toBe("Leads here");
    expect(backward?.label).not.toBe("Leads to");
  });

  it("walks the direction its label promises", () => {
    // The label and the walk are set in different files, and this is the only
    // thing that keeps them agreeing.
    const surface = "surface:MemberShelf";
    const loan = "entity:Loan";
    // A surface is where a chain starts: things follow from it, nothing leads
    // to it. A created entity is where one ends.
    expect(walkForm("forward", chain, surface).nodes.size).toBeGreaterThan(1);
    expect(walkForm("backward", chain, surface).nodes.size).toBe(1);
    expect(walkForm("forward", chain, loan).nodes.size).toBe(1);
    expect(walkForm("backward", chain, loan).nodes.size).toBeGreaterThan(1);
  });
});

describe("applies", () => {
  function entity(name: string, transitions: number): Node {
    return {
      id: `catalogue::entity::${name}`,
      kind: "entity",
      name,
      module: "catalogue",
      qualified: `catalogue/${name}`,
      span: null,
      detail: {
        type: "entity",
        kind: "internal",
        parent: null,
        fields: [],
        transitions: Array.from({ length: transitions }, () => ({
          field: "status",
          states: ["a", "b"],
          edges: [{ from: "a", to: "b" }],
          terminal: ["b"],
        })),
      },
    };
  }

  const rule: Node = {
    id: "catalogue::rule::AddBook",
    kind: "rule",
    name: "AddBook",
    module: "catalogue",
    qualified: "catalogue/AddBook",
    span: null,
    detail: { type: "none" },
  };

  it("offers the lifecycle only where there is one", () => {
    // A state machine belongs to an entity that declares transitions. Offering
    // the button for a rule would be offering an empty answer.
    expect(applies("lifecycle", entity("Copy", 1))).toBe(true);
    expect(applies("lifecycle", entity("Member", 0))).toBe(false);
    expect(applies("lifecycle", rule)).toBe(false);
  });

  it("offers the three directions for anything", () => {
    // Whether one comes back empty is a fact about the spec, not about the
    // kind — a surface has nothing leading to it and that is worth being told.
    for (const form of ["near", "forward", "backward"] as const) {
      expect(applies(form, rule)).toBe(true);
      expect(applies(form, entity("Member", 0))).toBe(true);
    }
  });
});

describe("walkForm for a lifecycle", () => {
  it("reaches only the construct itself, because it is not a walk", () => {
    // The states come from the entity's own transition list, which the caller
    // projects. Walking edges for them would find the wrong thing entirely.
    const reached = walkForm("lifecycle", chain, "entity:Loan");
    expect([...reached.nodes]).toEqual(["entity:Loan"]);
    expect(reached.edges.size).toBe(0);
  });
});
