import { describe, expect, it } from "vitest";

import type { EntityField } from "../api/EntityField";
import type { Node } from "../api/Node";
import type { NodeDetail } from "../api/NodeDetail";
import { summaryRows } from "./rows";

function field(partial: Partial<EntityField> & Pick<EntityField, "name">): EntityField {
  return {
    type_expr: "String",
    enum_values: [],
    derived: false,
    relationship: false,
    when: null,
    note: [],
    ...partial,
  };
}

function node(kind: Node["kind"], detail: NodeDetail, name = "Thing"): Node {
  return {
    id: `m::${kind}::${name}`,
    kind,
    name,
    module: "m",
    qualified: `m/${name}`,
    span: null,
    detail,
    prose: { note: [], guidance: [] },
  };
}

describe("an entity", () => {
  it("shows its fields with their types", () => {
    const rows = summaryRows(
      node("entity", {
        type: "entity",
        kind: "internal",
        parent: null,
        transitions: [],
        fields: [field({ name: "title" }), field({ name: "added_at", type_expr: "Timestamp" })],
      }),
    );
    expect(rows).toEqual([
      { label: "title", value: "String", muted: false },
      { label: "added_at", value: "Timestamp", muted: false },
    ]);
  });

  it("shows a status field's states rather than its type expression", () => {
    const rows = summaryRows(
      node("entity", {
        type: "entity",
        kind: "internal",
        parent: null,
        transitions: [],
        fields: [
          field({
            name: "status",
            type_expr: "listed | withdrawn",
            enum_values: ["listed", "withdrawn"],
          }),
        ],
      }),
    );
    expect(rows[0]?.value).toBe("listed | withdrawn");
  });

  it("abbreviates a long list of states rather than clipping it", () => {
    const rows = summaryRows(
      node("entity", {
        type: "entity",
        kind: "internal",
        parent: null,
        transitions: [],
        fields: [
          field({
            name: "status",
            enum_values: ["available", "on_loan", "lost", "missing"],
          }),
        ],
      }),
    );
    expect(rows[0]?.value).toBe("available | on_loan +2");
  });

  it("marks a derived field so it does not read as stored", () => {
    const rows = summaryRows(
      node("entity", {
        type: "entity",
        kind: "internal",
        parent: null,
        transitions: [],
        fields: [field({ name: "copy_count", type_expr: "copies.count", derived: true })],
      }),
    );
    expect(rows[0]?.muted).toBe(true);
  });

  it("says how many fields it did not show", () => {
    // Silently truncating makes a twelve-field entity look like a five-field
    // one, which is a worse error than the crowding it avoids.
    const fields = Array.from({ length: 12 }, (_, index) => field({ name: `f${index}` }));
    const rows = summaryRows(
      node("entity", { type: "entity", kind: "internal", parent: null, transitions: [], fields }),
    );
    expect(rows).toHaveLength(6);
    expect(rows.at(-1)).toEqual({ label: "+7 more fields", muted: true });
  });

  it("does not add an overflow row when everything fit", () => {
    const fields = [field({ name: "only" })];
    const rows = summaryRows(
      node("entity", { type: "entity", kind: "internal", parent: null, transitions: [], fields }),
    );
    expect(rows).toHaveLength(1);
  });

  it("uses the singular when exactly one field is hidden", () => {
    const fields = Array.from({ length: 6 }, (_, index) => field({ name: `f${index}` }));
    const rows = summaryRows(
      node("entity", { type: "entity", kind: "internal", parent: null, transitions: [], fields }),
    );
    expect(rows.at(-1)?.label).toBe("+1 more field");
  });
});

describe("an enum and a config block", () => {
  it("shows an enum's values", () => {
    const rows = summaryRows(node("enum", { type: "enum", values: ["print", "audio"] }));
    expect(rows.map((row) => row.label)).toEqual(["print", "audio"]);
  });

  it("shows a config parameter's default, which is what a reader wants", () => {
    const rows = summaryRows(
      node("config", {
        type: "config",
        parameters: [{ name: "loan_limit", type_expr: "Integer", default_expr: "5" }],
      }),
    );
    expect(rows[0]).toEqual({ label: "loan_limit", value: "5" });
  });

  it("falls back to the type when a parameter has no default", () => {
    const rows = summaryRows(
      node("config", {
        type: "config",
        parameters: [{ name: "limit", type_expr: "Integer", default_expr: null }],
      }),
    );
    expect(rows[0]?.value).toBe("Integer");
  });
});

describe("a rule", () => {
  const rule = (partial: Partial<Extract<NodeDetail, { type: "rule" }>>) =>
    node("rule", {
      type: "rule",
      trigger: "MemberBorrows",
      source: "external",
      clauses: [],
      creates: [],
      emits: [],
      ...partial,
    });

  it("is read by what it produces", () => {
    const rows = summaryRows(rule({ creates: ["Loan"], emits: ["CopyBorrowed"] }));
    expect(rows).toEqual([
      { label: "creates", value: "Loan" },
      { label: "emits", value: "CopyBorrowed" },
    ]);
  });

  it("counts its preconditions rather than quoting them", () => {
    // A clause is a sentence and a node is a box.
    const clauses = [
      { keyword: "when", text: "MemberBorrows(m, c)", span: null },
      { keyword: "requires", text: "c.status = available", span: null },
      { keyword: "requires", text: "not m.is_at_limit", span: null },
    ];
    const rows = summaryRows(rule({ clauses }));
    expect(rows[0]).toEqual({ label: "requires", value: "2", muted: true });
  });

  it("shows no precondition row when it has none", () => {
    expect(summaryRows(rule({}))).toEqual([]);
  });
});

describe("a trigger", () => {
  it("distinguishes an external stimulus from a state condition", () => {
    // They are driven by completely different things: one by a person, one by
    // the world changing. It is the most important fact about a trigger.
    const stimulus = summaryRows(
      node("trigger", {
        type: "trigger",
        source: "external",
        parameters: ["member", "copy"],
        condition: null,
        entity: null,
      }),
    );
    expect(stimulus[0]?.label).toBe("stimulus");
    expect(stimulus[0]?.value).toBe("member, copy");

    const condition = summaryRows(
      node("trigger", {
        type: "trigger",
        source: "temporal",
        parameters: ["loan"],
        condition: null,
        entity: "Loan",
      }),
    );
    expect(condition[0]?.label).toBe("temporal of");
    expect(condition[0]?.value).toBe("Loan");
  });
});

describe("a surface, an actor and an invariant", () => {
  it("names who a surface faces", () => {
    const rows = summaryRows(
      node("surface", {
        type: "surface",
        actor: "Reader",
        actor_binding: "reader",
        context: null,
        exposes: [],
        provides: [
          { trigger: "MemberBorrows", parameters: [], when: null },
          { trigger: "MemberReturns", parameters: [], when: null },
        ],
        guarantees: [],
      }),
    );
    expect(rows).toEqual([
      { label: "facing", value: "Reader" },
      { label: "provides", value: "2", muted: true },
    ]);
  });

  it("names the entity an actor is an instance of", () => {
    const rows = summaryRows(
      node("actor", { type: "actor", entity: "Staff", condition: null, within: null }),
    );
    expect(rows).toEqual([{ label: "is", value: "Staff" }]);
  });

  it("says when an invariant is prose only", () => {
    // It is part of the spec and shown as such; what must not happen is it
    // looking like something that was checked.
    const rows = summaryRows(
      node("invariant", { type: "invariant", expression: null, entities: [] }),
    );
    expect(rows).toEqual([{ label: "prose only", muted: true }]);
  });

  it("names what an invariant constrains", () => {
    const rows = summaryRows(
      node("invariant", {
        type: "invariant",
        expression: "m.open_loan_count <= config.loan_limit",
        entities: ["Member"],
      }),
    );
    expect(rows).toEqual([{ label: "over", value: "Member" }]);
  });
});

describe("an unresolved reference", () => {
  it("says that nothing declares it, which is why it is drawn at all", () => {
    const rows = summaryRows(node("external", { type: "none" }, "Phantom"));
    expect(rows).toEqual([{ label: "not declared", muted: true }]);
  });

  it("adds nothing to a node that simply has no detail", () => {
    expect(summaryRows(node("entity", { type: "none" }))).toEqual([]);
  });
});
