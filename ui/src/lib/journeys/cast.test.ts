import { describe, expect, it } from "vitest";

import type { CastMember } from "../api/CastMember";
import type { Instance } from "../api/Instance";
import type { Walk } from "../api/Walk";
import type { World } from "../api/World";
import {
  ORIGIN,
  ORIGIN_MEANING,
  appearsAt,
  configOf,
  fieldsOf,
  instanceOf,
  stateAt,
} from "./cast";

function instance(id: string, entity: string, fields: Instance["fields"] = {}): Instance {
  return { id, entity, module: "lending", fields };
}

function world(entities: Instance[], config: World["config"] = {}): World {
  return {
    entities: Object.fromEntries(entities.map((each) => [each.id, each])),
    config,
    now: 0,
    next_ordinal: {},
  };
}

function member(name: string, entity: string | null): CastMember {
  return { name, type_expr: "Member", entity, origin: "cast", line: 4 };
}

function walk(worlds: World[]): Walk {
  return {
    name: "J",
    cast: [],
    goal: [],
    ends: [],
    line: 1,
    stipulated: [],
    notes: [],
    steps: worlds.map((each, at) => ({
      number: at + 1,
      title: `step ${at + 1}`,
      line: 8 + at,
      outcomes: [],
      world: each,
    })),
  };
}

describe("stateAt", () => {
  const first = world([instance("Member#1", "Member", { name: { kind: "str", value: "Ada" } })]);
  const second = world([instance("Member#1", "Member", { name: { kind: "str", value: "Bea" } })]);

  it("reads the world the chosen step left behind", () => {
    // The whole reason worlds are kept per step: a single final state answers
    // "what is it now" while hiding the step that made it so.
    expect(stateAt(walk([first, second]), 0)).toBe(first);
    expect(stateAt(walk([first, second]), 1)).toBe(second);
  });

  it("clamps past the end rather than going blank", () => {
    // A walk that lost a step should show the state it reached. An empty panel
    // reads as "nothing exists", which is a different and wrong claim.
    expect(stateAt(walk([first, second]), 9)).toBe(second);
  });

  it("has nothing to show for a journey with no steps", () => {
    expect(stateAt(walk([]), 0)).toBeNull();
  });
});

describe("instanceOf", () => {
  it("finds the instance a name resolved to", () => {
    const ada = instance("Member#1", "Member");
    expect(instanceOf(world([ada]), member("ada", "Member#1"))).toBe(ada);
  });

  it("finds nothing for a name that resolved to nothing", () => {
    // The step meant to create it did not. Distinct from "not yet", and the
    // panel says so differently.
    expect(instanceOf(world([]), member("ghost", null))).toBeNull();
  });

  it("finds nothing for an instance this step does not have yet", () => {
    expect(instanceOf(world([]), member("loan", "Loan#1"))).toBeNull();
  });
});

describe("fieldsOf", () => {
  it("keeps the order the world holds, which is the spec's", () => {
    // Re-sorting would put `status` above `book` for no reason a reader could
    // name, and the field order is one of the few things the spec's author
    // chose deliberately.
    const held = instance("Loan#1", "Loan", {
      copy: { kind: "unknown" },
      member: { kind: "unknown" },
      status: { kind: "enum", value: "open" },
    });
    expect(fieldsOf(held).map(([name]) => name)).toEqual(["copy", "member", "status"]);
  });

  it("has no fields for nothing", () => {
    expect(fieldsOf(null)).toEqual([]);
  });
});

describe("configOf", () => {
  it("splits each parameter into the module that declares it", () => {
    // Two modules routinely name the same parameter, and `loan_limit` alone
    // would be ambiguous in a spec set of five files.
    const shown = configOf(
      world([], {
        "lending.loan_limit": { kind: "int", value: 5 },
        "catalogue.max_copies_per_book": { kind: "int", value: 20 },
      }),
    );
    expect(shown).toContainEqual({
      module: "lending",
      name: "loan_limit",
      value: { kind: "int", value: 5 },
    });
    expect(shown).toContainEqual({
      module: "catalogue",
      name: "max_copies_per_book",
      value: { kind: "int", value: 20 },
    });
  });

  it("splits on the first dot, so a dotted parameter name survives", () => {
    const shown = configOf(world([], { "lending.window.grace": { kind: "int", value: 1 } }));
    expect(shown[0]).toEqual({
      module: "lending",
      name: "window.grace",
      value: { kind: "int", value: 1 },
    });
  });

  it("keeps a parameter with no module rather than dropping it", () => {
    expect(configOf(world([], { bare: { kind: "int", value: 1 } }))[0]?.name).toBe("bare");
  });

  it("has nothing to show for no world", () => {
    expect(configOf(null)).toEqual([]);
  });
});

describe("appearsAt", () => {
  it("finds the step a caught instance first exists in", () => {
    // A caught name does not exist until the step that created it, and showing
    // its fields as empty in step one reads as "it has no values" rather than
    // "it does not exist yet".
    const before = world([]);
    const after = world([instance("Loan#1", "Loan")]);
    expect(appearsAt(walk([before, after]), member("loan", "Loan#1"))).toBe(1);
  });

  it("says nowhere for a name nothing was created for", () => {
    expect(appearsAt(walk([world([])]), member("ghost", null))).toBe(-1);
  });
});

describe("ORIGIN", () => {
  it("has a word and a meaning for each of the three ways a name is bound", () => {
    for (const origin of ["cast", "given", "caught"] as const) {
      expect(ORIGIN[origin]).toBeTruthy();
      expect(ORIGIN_MEANING[origin].length).toBeGreaterThan(10);
    }
  });

  it("gives the three different words, because they are read differently", () => {
    expect(new Set(Object.values(ORIGIN)).size).toBe(3);
  });
});
