// @vitest-environment happy-dom

import { render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";

import type { Effect } from "../api/Effect";
import type { RuleOutcome } from "../api/RuleOutcome";
import type { StepOutcome } from "../api/StepOutcome";
import type { World } from "../api/World";
import Trace from "./Trace.svelte";
import WorldPanel from "./WorldPanel.svelte";

// Two siblings woken by `Paint` that wrote `colour` differently (issue #24):
// both fired, and the world holds an unknown naming both.
const CLASH =
  "`PaintBlue` and `PaintRed` were both woken by `Paint`: `PaintBlue` set `Lamp#1.colour` to " +
  '"blue" and `PaintRed` set it to "red"; the specification does not say which of the two ' +
  "runs first, so which value stands is undecided";

function painted(to: string): Effect {
  return {
    kind: "assigned",
    id: "Lamp#1",
    field: "colour",
    from: { kind: "unknown" },
    to: { kind: "str", value: to },
  };
}

function fired(name: string, to: string, unapplied: boolean): RuleOutcome {
  const notes = unapplied ? [{ reason: CLASH, expression: null, span: null }] : [];
  return {
    rule: `siblings::rule::${name}`,
    name,
    module: "siblings",
    disposition: "fired",
    requires: [],
    effects: [painted(to)],
    unresolved: notes,
    unapplied: notes,
  };
}

const world: World = {
  entities: {
    "Lamp#1": {
      id: "Lamp#1",
      entity: "Lamp",
      module: "siblings",
      fields: { colour: { kind: "unknown" } },
      why: { colour: [{ reason: CLASH, expression: null, span: null }] },
    },
  },
  config: {},
  answers: [],
  now: 0,
  next_ordinal: { Lamp: 2 },
};

const outcome: StepOutcome = {
  world,
  event: { trigger: "Paint", module: "siblings", arguments: {}, positional: [], emitted: false },
  rules: [fired("PaintBlue", "blue", false), fired("PaintRed", "red", true)],
  invariants: [],
  newly_enabled: [],
  emitted: [],
};

describe("Trace", () => {
  it("says what a fired rule did not apply, and marks both writes as not settled", () => {
    render(Trace, { props: { outcome, onfire: vi.fn() } });
    expect(screen.getByText("Part of what it ensures was not applied")).toBeTruthy();
    expect(screen.getByText(CLASH)).toBeTruthy();
    expect(screen.getAllByText("set, not settled")).toHaveLength(2);
    expect(screen.queryByText("set")).toBeNull();
  });
});

describe("WorldPanel", () => {
  it("shows why a field a rule left undecided is undecided", () => {
    render(WorldPanel, {
      props: {
        world,
        entities: [
          {
            entity: "Lamp",
            module: "siblings",
            fields: [{ name: "colour", type_expr: "String", states: [], derived: false }],
          },
        ],
        onchange: vi.fn(),
      },
    });
    expect(screen.getByText(CLASH)).toBeTruthy();
  });
});
