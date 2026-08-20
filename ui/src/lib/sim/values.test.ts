import { describe, expect, it } from "vitest";

import type { Value } from "../api/Value";
import { duration, parse, render } from "./values";

describe("render", () => {
  it("names an undecided value rather than leaving a blank", () => {
    // An empty cell reads as "nothing here". "The simulator does not know
    // what is here" is a different statement, and it is the one this whole
    // tool exists to make clearly.
    expect(render({ kind: "unknown" })).toEqual({ text: "unknown", unknown: true });
  });

  it("shows null as null, because a spec asserts about it", () => {
    // `attachment_size = null` is a real precondition with a real answer.
    expect(render({ kind: "null" })).toEqual({ text: "null", unknown: false });
  });

  it("shows the scalars as they were written", () => {
    expect(render({ kind: "bool", value: true }).text).toBe("true");
    expect(render({ kind: "int", value: -3 }).text).toBe("-3");
    expect(render({ kind: "float", value: 1.5 }).text).toBe("1.5");
    expect(render({ kind: "enum", value: "available" }).text).toBe("available");
    expect(render({ kind: "ref", value: "Loan#1" }).text).toBe("Loan#1");
  });

  it("quotes a string so it is distinguishable from a state", () => {
    // `"held"` and `held` are different values in Allium, and the panel has to
    // show which one it is looking at.
    expect(render({ kind: "str", value: "held" }).text).toBe('"held"');
    expect(render({ kind: "enum", value: "held" }).text).toBe("held");
  });

  it("shows a duration in units a person reads", () => {
    // `1814400000` is not a number anyone reads as three weeks.
    expect(render({ kind: "duration", value: 21 * 86_400_000 }).text).toBe("3 weeks");
  });

  it("marks a timestamp as an offset, since the clock has no calendar", () => {
    expect(render({ kind: "timestamp", value: 3_600_000 }).text).toBe("t+1 hour");
  });

  it("shows an empty collection as empty rather than as nothing", () => {
    expect(render({ kind: "set", value: [] }).text).toBe("(empty)");
  });

  it("shows a collection's contents", () => {
    const set: Value = {
      kind: "set",
      value: [
        { kind: "ref", value: "Copy#1" },
        { kind: "ref", value: "Copy#2" },
      ],
    };
    expect(render(set).text).toBe("{Copy#1, Copy#2}");
    expect(render(set).unknown).toBe(false);
  });

  it("marks a collection holding an undecided element as only partly known", () => {
    // Otherwise it reads as a settled list that happens to have a gap.
    const set: Value = {
      kind: "set",
      value: [{ kind: "ref", value: "Copy#1" }, { kind: "unknown" }],
    };
    expect(render(set).unknown).toBe(true);
  });
});

describe("duration", () => {
  it("uses the largest unit that divides exactly", () => {
    expect(duration(604_800_000)).toBe("1 week");
    expect(duration(86_400_000)).toBe("1 day");
    expect(duration(3_600_000)).toBe("1 hour");
    expect(duration(60_000)).toBe("1 minute");
    expect(duration(1_000)).toBe("1 second");
  });

  it("pluralises only when it should", () => {
    expect(duration(2 * 86_400_000)).toBe("2 days");
    expect(duration(86_400_000)).toBe("1 day");
  });

  it("falls back to milliseconds when nothing divides exactly", () => {
    expect(duration(1_500)).toBe("1500ms");
    expect(duration(7)).toBe("7ms");
  });

  it("shows zero as zero rather than as no time at all", () => {
    expect(duration(0)).toBe("0");
  });

  it("handles a negative duration", () => {
    expect(duration(-86_400_000)).toBe("-1 day");
    expect(duration(-2 * 3_600_000)).toBe("-2 hours");
  });
});

describe("parse", () => {
  it("reads an empty box as undecided, not as null", () => {
    // The distinction the simulator is built on: `unknown` is "nobody said",
    // and `null` is "there is nothing there".
    expect(parse("")).toEqual({ kind: "unknown" });
    expect(parse("   ")).toEqual({ kind: "unknown" });
    expect(parse("null")).toEqual({ kind: "null" });
  });

  it("prefers a declared state over any other reading", () => {
    // A field whose type is `open | returned` holding `open` is a state, even
    // though `open` is also a perfectly good string.
    expect(parse("open", ["open", "returned"])).toEqual({ kind: "enum", value: "open" });
    expect(parse("open")).toEqual({ kind: "str", value: "open" });
  });

  it("reads booleans, numbers and references", () => {
    expect(parse("true")).toEqual({ kind: "bool", value: true });
    expect(parse("false")).toEqual({ kind: "bool", value: false });
    expect(parse("42")).toEqual({ kind: "int", value: 42 });
    expect(parse("-7")).toEqual({ kind: "int", value: -7 });
    expect(parse("1.5")).toEqual({ kind: "float", value: 1.5 });
    expect(parse("Loan#1")).toEqual({ kind: "ref", value: "Loan#1" });
  });

  it("reads a duration written the way the spec writes one", () => {
    expect(parse("21.days")).toEqual({ kind: "duration", value: 21 * 86_400_000 });
    expect(parse("1.hour")).toEqual({ kind: "duration", value: 3_600_000 });
  });

  it("keeps anything else as a string", () => {
    // A field typed `String` holding `5` is a string; guessing otherwise would
    // silently change what the simulator is told.
    expect(parse("Ada Lovelace")).toEqual({ kind: "str", value: "Ada Lovelace" });
    expect(parse('"quoted"')).toEqual({ kind: "str", value: "quoted" });
    expect(parse("QA76.6")).toEqual({ kind: "str", value: "QA76.6" });
  });

  it("does not mistake a shelfmark for a duration", () => {
    expect(parse("3.fortnights")).toEqual({ kind: "str", value: "3.fortnights" });
  });

  it("round-trips what render produced, for every kind that can be typed", () => {
    const cases: [Value, string[]][] = [
      [{ kind: "null" }, []],
      [{ kind: "bool", value: true }, []],
      [{ kind: "int", value: 42 }, []],
      [{ kind: "float", value: 1.5 }, []],
      [{ kind: "enum", value: "open" }, ["open"]],
      [{ kind: "ref", value: "Loan#1" }, []],
      [{ kind: "str", value: "Ada" }, []],
      [{ kind: "duration", value: 21 * 86_400_000 }, []],
    ];
    for (const [value, states] of cases) {
      const shown = render(value).text;
      const typed = value.kind === "duration" ? "21.days" : shown;
      expect(parse(typed, states)).toEqual(value);
    }
  });
});
