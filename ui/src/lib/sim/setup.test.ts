import { describe, expect, it } from "vitest";

import { grouped, offered, type Fireable } from "./setup";

function fireable(partial: Partial<Fireable> & Pick<Fireable, "trigger">): Fireable {
  return {
    module: "delivery",
    parameters: [],
    surface: null,
    actor: null,
    ...partial,
  };
}

const triggers: Fireable[] = [
  fireable({ trigger: "MemberAdoptsHub", surface: "HubVisibility", actor: "membership/Member" }),
  fireable({ trigger: "MemberRemovesHub", surface: "HubVisibility", actor: "membership/Member" }),
  fireable({ trigger: "PersonExportsHistory", module: "archive", surface: "ArchiveControls" }),
  fireable({ trigger: "MessageSent", module: "messaging" }),
];

describe("offered", () => {
  it("offers everything when nothing is switched off", () => {
    expect(offered(triggers, new Set())).toHaveLength(4);
  });

  it("stops offering a switched-off module's triggers", () => {
    // The checkboxes filtered the canvas and did nothing here, while sitting in
    // the same rail four inches above it.
    expect(offered(triggers, new Set(["archive"])).map((t) => t.trigger)).toEqual([
      "MemberAdoptsHub",
      "MemberRemovesHub",
      "MessageSent",
    ]);
  });

  it("offers nothing when every module is off", () => {
    expect(offered(triggers, new Set(["delivery", "archive", "messaging"]))).toEqual([]);
  });

  it("keeps the order it was given", () => {
    // The server put the surfaces first and `grouped` relies on that order, so
    // filtering must not reshuffle what it hands on.
    expect(offered(triggers, new Set(["messaging"])).map((t) => t.trigger)).toEqual([
      "MemberAdoptsHub",
      "MemberRemovesHub",
      "PersonExportsHistory",
    ]);
  });
});

describe("grouped", () => {
  it("names a group by the surface and the actor it faces", () => {
    // A boundary without a party is meaningless, and the actor is the half a
    // reader recognises: they know what a member is before they know what
    // `HubVisibility` is.
    expect(grouped(triggers)[0]?.label).toBe("HubVisibility · membership/Member");
  });

  it("puts every trigger a surface offers under that one heading", () => {
    const first = grouped(triggers)[0];
    expect(first?.triggers.map((t) => t.trigger)).toEqual([
      "MemberAdoptsHub",
      "MemberRemovesHub",
    ]);
  });

  it("names a surface with no actor by itself", () => {
    expect(grouped(triggers).map((group) => group.label)).toContain("ArchiveControls");
  });

  it("collects the rest under one honest heading", () => {
    // A trigger nothing provides is not something a person does — it is emitted
    // by a rule — and grouping it with the surfaces would say otherwise.
    const last = grouped(triggers).at(-1);
    expect(last?.label).toBe("Emitted elsewhere");
    expect(last?.triggers.map((t) => t.trigger)).toEqual(["MessageSent"]);
  });

  it("keeps the order the server chose", () => {
    // Surfaces first, because a trigger a surface provides is the honest place
    // to start. Re-sorting here would second-guess a decision already made.
    expect(grouped(triggers).map((group) => group.label)).toEqual([
      "HubVisibility · membership/Member",
      "ArchiveControls",
      "Emitted elsewhere",
    ]);
  });

  it("has no groups at all when nothing is offered", () => {
    expect(grouped([])).toEqual([]);
  });
});
