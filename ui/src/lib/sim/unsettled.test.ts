import { describe, expect, it } from "vitest";

import type { Effect } from "../api/Effect";
import type { Instance } from "../api/Instance";
import type { World } from "../api/World";
import { notSettled, whyUndecided } from "./unsettled";

const clash = {
  reason:
    "`PaintBlue` and `PaintRed` were both woken by `Paint`: which value stands is undecided",
  expression: null,
  span: null,
};

function lamp(colour: Instance["fields"][string], why?: Instance["why"]): Instance {
  return { id: "Lamp#1", entity: "Lamp", module: "siblings", fields: { colour }, why };
}

function world(instance: Instance): World {
  return { entities: { [instance.id]: instance }, config: {}, answers: [], now: 0, next_ordinal: {} };
}

function painted(to: string): Effect {
  return {
    kind: "assigned",
    id: "Lamp#1",
    field: "colour",
    from: { kind: "unknown" },
    to: { kind: "str", value: to },
  };
}

describe("whyUndecided", () => {
  it("gives the reason a write kept, and nothing when none was kept", () => {
    expect(whyUndecided(lamp({ kind: "unknown" }, { colour: [clash] }), "colour")).toEqual([
      clash,
    ]);
    expect(whyUndecided(lamp({ kind: "unknown" }), "colour")).toEqual([]);
    expect(whyUndecided(undefined, "colour")).toEqual([]);
  });
});

describe("notSettled", () => {
  it("says why a settled write did not stand when the field was left undecided", () => {
    const after = world(lamp({ kind: "unknown" }, { colour: [clash] }));
    expect(notSettled(painted("red"), after)).toEqual([clash]);
  });

  it("says nothing for a write that stood, or one that wrote the unknown itself", () => {
    const stood = world(lamp({ kind: "str", value: "red" }));
    expect(notSettled(painted("red"), stood)).toEqual([]);
    const after = world(lamp({ kind: "unknown" }, { colour: [clash] }));
    const unknown: Effect = { ...painted("red"), to: { kind: "unknown" } } as Effect;
    expect(notSettled(unknown, after)).toEqual([]);
    expect(notSettled({ kind: "removed", id: "Lamp#1", entity: "Lamp" }, after)).toEqual([]);
  });

  it("says nothing about an instance the step removed", () => {
    const gone: World = { ...world(lamp({ kind: "unknown" })), entities: {} };
    expect(notSettled(painted("red"), gone)).toEqual([]);
  });
});
