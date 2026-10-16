import { describe, expect, it } from "vitest";

import type { Enabled } from "../api/Enabled";
import { offered } from "./enabled";

function entry(trigger: string, over: string[]): Enabled {
  return {
    rule: `orders::rule::X`,
    name: "X",
    module: "orders",
    trigger,
    source: "state",
    over: over.map((value) => ({ kind: "ref", value })),
    binding: "o",
    undecided: [],
    unsettled_over: [],
  };
}

describe("offered", () => {
  it("says a creation's reaction reacts to the creation, not that it is a state", () => {
    // Issue #9: the creation rule arrives with `source: state`, because the
    // model files it so, and the panel used to show it under that label.
    expect(offered(entry("Order.created", ["Order#1"]))).toEqual({
      eyebrow: "created",
      about: "reacts to Order#1 being created",
    });
  });

  it("leaves a state rule as it was", () => {
    expect(offered(entry("Copy", ["Copy#2", "Copy#3"]))).toEqual({
      eyebrow: "state",
      about: "over Copy#2, Copy#3",
    });
  });

  it("says nothing about whom when it is about nobody", () => {
    expect(offered(entry("Copy", [])).about).toBeNull();
    expect(offered(entry("Order.created", [])).about).toBeNull();
  });
});
