// @vitest-environment happy-dom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Fireable } from "./setup";
import TriggerPicker from "./TriggerPicker.svelte";

function fireable(
  trigger: string,
  parameters: string[] = [],
  surface = "SetOfFiles",
): Fireable {
  return { trigger, module: "reading", parameters, surface, actor: "Reader" };
}

/**
 * happy-dom has no layout, so `scrollIntoView` is a stub. What is under test is
 * that the component asks for it at all and asks for the right thing — the
 * scrolling itself is the browser's.
 */
function watchScrolling(): ReturnType<typeof vi.fn> {
  const scrolled = vi.fn();
  Element.prototype.scrollIntoView = scrolled;
  return scrolled;
}

describe("TriggerPicker", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("sends a word the spec declares as a state as that state, and any other as text", async () => {
    // `MemberWithdrawsHold(hold, changed_mind)` against `reason in
    // {changed_mind, found_elsewhere}`. Sent as the string "changed_mind" it
    // matched nothing, so the rule was undecided in the browser while the
    // same act in a journey fired.
    watchScrolling();
    const onfire = vi.fn();
    render(TriggerPicker, {
      props: {
        triggers: [fireable("MemberWithdrawsHold", ["reason", "note"])],
        instances: [],
        pending: [],
        states: ["changed_mind", "found_elsewhere"],
        onfire,
      },
    });

    await fireEvent.click(
      screen.getByRole("button", { name: "MemberWithdrawsHold" }),
    );
    const boxes = screen.getAllByRole("combobox");
    expect(boxes).toHaveLength(2);
    await fireEvent.input(boxes[0]!, { target: { value: "changed_mind" } });
    await fireEvent.input(boxes[1]!, { target: { value: "moved" } });
    await fireEvent.click(screen.getByRole("button", { name: "Fire" }));

    expect(onfire).toHaveBeenCalledWith("MemberWithdrawsHold", "reading", {
      reason: { kind: "enum", value: "changed_mind" },
      note: { kind: "str", value: "moved" },
    });
  });

  it("brings the argument form into view when a trigger is chosen", async () => {
    const scrolled = watchScrolling();
    render(TriggerPicker, {
      props: {
        triggers: [fireable("SomebodyPointsAtASpecSet", ["reader", "root"])],
        instances: [],
        pending: [],
        onfire: () => {},
      },
    });

    expect(scrolled).not.toHaveBeenCalled();

    await fireEvent.click(
      screen.getByRole("button", { name: "SomebodyPointsAtASpecSet" }),
    );

    // The form exists to be filled in, so it has to be the thing scrolled to —
    // scrolling to the button that was just clicked would be a no-op that
    // looks like the fix.
    expect(scrolled).toHaveBeenCalledTimes(1);
    const target = scrolled.mock.instances[0] as HTMLElement;
    expect(target.tagName).toBe("FORM");
    expect(target.textContent).toContain("SomebodyPointsAtASpecSet");

    // `nearest` and not `start`: a form already on screen must not jump.
    expect(scrolled).toHaveBeenCalledWith({ block: "nearest" });
  });

  it("brings it into view for a trigger that carries nothing", async () => {
    const scrolled = watchScrolling();
    render(TriggerPicker, {
      props: {
        triggers: [fireable("ClockMoved")],
        instances: [],
        pending: [],
        onfire: () => {},
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: "ClockMoved" }));

    // It says "This trigger carries nothing", which is an answer worth reading
    // and is in the same place off the bottom of the column.
    expect(scrolled).toHaveBeenCalledTimes(1);
  });

  it("scrolls again when the reader changes their mind", async () => {
    const scrolled = watchScrolling();
    render(TriggerPicker, {
      props: {
        triggers: [fireable("ReadTheSet"), fireable("SetBecameReadable")],
        instances: [],
        pending: [],
        onfire: () => {},
      },
    });

    await fireEvent.click(screen.getByRole("button", { name: "ReadTheSet" }));
    await fireEvent.click(
      screen.getByRole("button", { name: "SetBecameReadable" }),
    );

    expect(scrolled).toHaveBeenCalledTimes(2);
  });
});
