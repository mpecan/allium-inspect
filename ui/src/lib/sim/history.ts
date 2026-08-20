// The run: every step taken, and the ability to go back through them.
//
// A simulation is worth having only if you can back out of it. Reading a
// specification is an exploratory activity — you fire something to find out what
// happens, and half the time the answer is "not what I wanted". So the history
// keeps every world it has passed through rather than only the current one, and
// stepping back is free.
//
// Going back and then forward again *discards* what was ahead, the way an
// editor's undo does. The alternative — keeping a tree of branches — is a
// different and much larger tool, and a linear run is what a person walking a
// journey actually wants.

import type { StepOutcome } from "../api/StepOutcome";
import type { World } from "../api/World";

/** One entry in the run: the world, and how it was reached. */
export interface Frame {
  world: World;
  /** The step that produced this world; absent for the starting position. */
  outcome: StepOutcome | null;
  /** A short label for the timeline. */
  label: string;
}

/** Every world the run has passed through, and where in it we are. */
export interface History {
  frames: Frame[];
  /** Index into `frames`; always a valid position. */
  at: number;
}

/** A run that has only its starting world. */
export function start(world: World, label = "start"): History {
  return { frames: [{ world, outcome: null, label }], at: 0 };
}

/** The frame currently in view. */
export function current(history: History): Frame {
  // Clamped rather than trusted: `at` is carried in component state, and a
  // frame index that has drifted out of range should show the end of the run
  // rather than throw in the middle of a render.
  const index = Math.min(Math.max(history.at, 0), history.frames.length - 1);
  return history.frames[index] as Frame;
}

/** The world currently in view. */
export function world(history: History): World {
  return current(history).world;
}

/**
 * Record `outcome` as the next step.
 *
 * Anything ahead of the current position is dropped: stepping after going back
 * replaces the future rather than branching, which is what an editor's undo
 * does and what a person walking one path expects.
 */
export function advance(history: History, outcome: StepOutcome): History {
  const kept = history.frames.slice(0, history.at + 1);
  const frames = [
    ...kept,
    { world: outcome.world, outcome, label: outcome.event.trigger },
  ];
  return { frames, at: frames.length - 1 };
}

/** Move to `index`, clamped into the run. */
export function goTo(history: History, index: number): History {
  return { ...history, at: Math.min(Math.max(index, 0), history.frames.length - 1) };
}

/** One step back. */
export function back(history: History): History {
  return goTo(history, history.at - 1);
}

/** One step forward, if there is one. */
export function forward(history: History): History {
  return goTo(history, history.at + 1);
}

/** Whether there is anything behind or ahead. */
export function canGoBack(history: History): boolean {
  return history.at > 0;
}

export function canGoForward(history: History): boolean {
  return history.at < history.frames.length - 1;
}

/** Replace the current world, keeping the run's shape.
 *
 * The world editor needs this: adding an entity is not a step — no rule ran and
 * no trigger fired — so it edits the position you are standing on rather than
 * appending to the run.
 */
export function replaceWorld(history: History, world: World): History {
  const frames = history.frames.map((frame, index) =>
    index === history.at ? { ...frame, world } : frame,
  );
  return { ...history, frames };
}

/** How many steps have actually been taken. */
export function stepCount(history: History): number {
  return history.frames.filter((frame) => frame.outcome !== null).length;
}

/** Every trigger emitted so far and not yet fired, oldest first.
 *
 * The list of loose ends. A rule emitting `MessageSent` says something else
 * should react to it, and a run that never fires it has stopped halfway through
 * what the spec describes.
 */
export function pendingTriggers(history: History): string[] {
  const emitted: string[] = [];
  const fired = new Set<string>();

  for (const frame of history.frames.slice(0, history.at + 1)) {
    if (!frame.outcome) {
      continue;
    }
    fired.add(frame.outcome.event.trigger);
    for (const trigger of frame.outcome.emitted) {
      if (!emitted.includes(trigger)) {
        emitted.push(trigger);
      }
    }
  }

  return emitted.filter((trigger) => !fired.has(trigger));
}
