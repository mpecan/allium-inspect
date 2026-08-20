import { describe, expect, it } from "vitest";

import type { StepOutcome } from "../api/StepOutcome";
import type { World } from "../api/World";
import {
  advance,
  back,
  canGoBack,
  canGoForward,
  current,
  forward,
  goTo,
  pendingTriggers,
  replaceWorld,
  start,
  stepCount,
  world,
} from "./history";

function emptyWorld(now = 0): World {
  return { entities: {}, config: {}, now, next_ordinal: {} };
}

function outcome(trigger: string, now: number, emitted: string[] = []): StepOutcome {
  return {
    world: emptyWorld(now),
    event: { trigger, module: "m", arguments: {} },
    rules: [],
    invariants: [],
    newly_enabled: [],
    emitted,
  };
}

describe("start", () => {
  it("is one frame with no step behind it", () => {
    const history = start(emptyWorld());
    expect(history.frames).toHaveLength(1);
    expect(history.at).toBe(0);
    expect(current(history).outcome).toBeNull();
    expect(stepCount(history)).toBe(0);
  });

  it("has nowhere to go in either direction", () => {
    const history = start(emptyWorld());
    expect(canGoBack(history)).toBe(false);
    expect(canGoForward(history)).toBe(false);
  });
});

describe("advance", () => {
  it("appends the step and moves to it", () => {
    const history = advance(start(emptyWorld()), outcome("MemberBorrows", 10));
    expect(history.frames).toHaveLength(2);
    expect(history.at).toBe(1);
    expect(world(history).now).toBe(10);
    expect(current(history).label).toBe("MemberBorrows");
    expect(stepCount(history)).toBe(1);
  });

  it("keeps every world it has passed through", () => {
    // Reading a spec is exploratory: you fire something to find out what
    // happens, and going back has to be free.
    let history = start(emptyWorld(1));
    history = advance(history, outcome("First", 2));
    history = advance(history, outcome("Second", 3));
    expect(history.frames.map((frame) => frame.world.now)).toEqual([1, 2, 3]);
  });
});

describe("going back and forward", () => {
  const run = () => {
    let history = start(emptyWorld(1));
    history = advance(history, outcome("First", 2));
    history = advance(history, outcome("Second", 3));
    return history;
  };

  it("moves one frame at a time", () => {
    let history = run();
    history = back(history);
    expect(world(history).now).toBe(2);
    history = back(history);
    expect(world(history).now).toBe(1);
    history = forward(history);
    expect(world(history).now).toBe(2);
  });

  it("reports where it can go", () => {
    const history = run();
    expect(canGoBack(history)).toBe(true);
    expect(canGoForward(history)).toBe(false);

    const rewound = back(history);
    expect(canGoBack(rewound)).toBe(true);
    expect(canGoForward(rewound)).toBe(true);
  });

  it("stops at the ends rather than running off them", () => {
    let history = run();
    history = back(back(back(back(history))));
    expect(history.at).toBe(0);
    history = forward(forward(forward(forward(history))));
    expect(history.at).toBe(2);
  });

  it("jumps to a frame by index", () => {
    expect(world(goTo(run(), 0)).now).toBe(1);
    expect(world(goTo(run(), 1)).now).toBe(2);
  });

  it("clamps a jump outside the run", () => {
    expect(goTo(run(), -5).at).toBe(0);
    expect(goTo(run(), 99).at).toBe(2);
  });

  it("shows the end of the run when the position has drifted out of range", () => {
    // `at` lives in component state; a stale index should render the end of
    // the run rather than throw in the middle of a paint.
    const drifted = { ...run(), at: 99 };
    expect(world(drifted).now).toBe(3);
  });
});

describe("stepping after going back", () => {
  it("replaces what was ahead rather than branching", () => {
    // The same thing an editor's undo does. A tree of branches is a different
    // and much larger tool, and a linear run is what walking a journey means.
    let history = start(emptyWorld(1));
    history = advance(history, outcome("First", 2));
    history = advance(history, outcome("Second", 3));
    history = back(history);
    history = advance(history, outcome("Instead", 9));

    expect(history.frames.map((frame) => frame.label)).toEqual([
      "start",
      "First",
      "Instead",
    ]);
    expect(canGoForward(history)).toBe(false);
    expect(world(history).now).toBe(9);
  });
});

describe("replaceWorld", () => {
  it("edits the frame you are standing on rather than appending", () => {
    // Adding an entity is not a step: no rule ran and no trigger fired.
    let history = start(emptyWorld(1));
    history = advance(history, outcome("First", 2));
    const edited = replaceWorld(history, emptyWorld(42));

    expect(edited.frames).toHaveLength(2);
    expect(edited.at).toBe(1);
    expect(world(edited).now).toBe(42);
    expect(stepCount(edited)).toBe(1);
  });

  it("leaves the other frames alone", () => {
    let history = start(emptyWorld(1));
    history = advance(history, outcome("First", 2));
    const edited = replaceWorld(history, emptyWorld(42));
    expect(edited.frames[0]?.world.now).toBe(1);
  });
});

describe("pendingTriggers", () => {
  it("lists what has been emitted and not yet fired", () => {
    // The loose ends. A rule emitting `CopyBorrowed` says something should
    // react to it, and a run that never fires it stopped halfway.
    let history = start(emptyWorld());
    history = advance(history, outcome("MemberBorrows", 1, ["CopyBorrowed"]));
    expect(pendingTriggers(history)).toEqual(["CopyBorrowed"]);
  });

  it("drops one once it has been fired", () => {
    let history = start(emptyWorld());
    history = advance(history, outcome("MemberBorrows", 1, ["CopyBorrowed"]));
    history = advance(history, outcome("CopyBorrowed", 2));
    expect(pendingTriggers(history)).toEqual([]);
  });

  it("counts only what is behind the current position", () => {
    let history = start(emptyWorld());
    history = advance(history, outcome("First", 1, ["Emitted"]));
    history = back(history);
    expect(pendingTriggers(history)).toEqual([]);
  });

  it("lists a trigger once however often it is emitted", () => {
    let history = start(emptyWorld());
    history = advance(history, outcome("A", 1, ["Same"]));
    history = advance(history, outcome("B", 2, ["Same"]));
    expect(pendingTriggers(history)).toEqual(["Same"]);
  });

  it("is empty for a run that has not stepped", () => {
    expect(pendingTriggers(start(emptyWorld()))).toEqual([]);
  });
});
