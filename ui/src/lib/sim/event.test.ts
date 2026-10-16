import { describe, expect, it } from "vitest";

import type { Value } from "../api/Value";
import { chosenEvent, followedEvent, type Emission } from "./event";

const task: Value = { kind: "ref", value: "Task#1" };
const user: Value = { kind: "ref", value: "User#1" };

describe("chosenEvent", () => {
  it("passes the form's values by position, in order, and names none", () => {
    // `Assign(task, user)` against a second listener `when: Assign(user,
    // task)`: named as well, that listener bound `user` to the user by the
    // coincidence of the name, while the walk — by position — bound it to the
    // task.
    expect(chosenEvent("Assign", "desk", [task, user])).toEqual({
      trigger: "Assign",
      module: "desk",
      emitted: false,
      arguments: {},
      positional: [task, user],
    });
  });

  it("does not share the array it was handed", () => {
    const values = [task];
    const event = chosenEvent("Assign", "desk", values);
    values.push(user);
    expect(event.positional).toEqual([task]);
  });
});

describe("followedEvent", () => {
  // Marked as an emission, so the step hears it only where the walk would: a
  // module that does not import `chains` and has its own `Published` is not
  // woken by this one.
  it("fires an emission with what it carried, both ways, as an emission", () => {
    const emission: Emission = {
      kind: "emitted",
      trigger: "Published",
      module: "chains",
      arguments: { item: task },
      positional: [user],
    };
    expect(followedEvent(emission)).toEqual({
      trigger: "Published",
      module: "chains",
      emitted: true,
      arguments: { item: task },
      positional: [user],
    });
  });
});
