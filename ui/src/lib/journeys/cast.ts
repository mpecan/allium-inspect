// Reading a journey's cast out of the world each step left behind.
//
// Kept out of the component because these are decisions, not markup: which
// step's world a reader is looking at, and what "no value" means for a field
// nothing ever set. Both have a wrong answer that looks fine on screen.

import type { CastMember } from "../api/CastMember";
import type { Instance } from "../api/Instance";
import type { Origin } from "../api/Origin";
import type { Value } from "../api/Value";
import type { Walk } from "../api/Walk";
import type { World } from "../api/World";

/** How each origin reads in the panel. */
export const ORIGIN: Record<Origin, string> = {
  cast: "cast",
  given: "given",
  caught: "caught",
};

/** What each origin means, for the reader who has not written a journey yet. */
export const ORIGIN_MEANING: Record<Origin, string> = {
  cast: "named in the cast block",
  given: "described in the given block",
  caught: "created by a step, and caught there",
};

/**
 * The world as it stood after step `at`, or before anything ran.
 *
 * `-1` is the world the `given` block laid out — which is a real and useful
 * answer, because half of reading a journey is checking what it started from.
 * An index past the end clamps to the last step rather than returning nothing:
 * a walk that lost a step should show the state it reached, not a blank panel.
 */
export function stateAt(walk: Walk, at: number): World | null {
  if (walk.steps.length === 0) {
    return null;
  }
  if (at < 0) {
    return null;
  }
  return walk.steps[Math.min(at, walk.steps.length - 1)]?.world ?? null;
}

/** The instance `member` resolved to in `world`, if it resolved to one. */
export function instanceOf(world: World | null, member: CastMember): Instance | null {
  if (!world || member.entity === null) {
    return null;
  }
  return world.entities[member.entity] ?? null;
}

/**
 * One instance's fields, in the order the world holds them.
 *
 * Not sorted. The world is a `BTreeMap` on the Rust side and the order is the
 * spec's own; re-sorting here would put `status` above `book` for no reason a
 * reader could name.
 */
export function fieldsOf(instance: Instance | null): [string, Value][] {
  return instance ? Object.entries(instance.fields) : [];
}

/** One configuration parameter, split into the module that declares it. */
export interface Parameter {
  module: string;
  name: string;
  value: Value;
}

/**
 * The configuration in force, grouped by module.
 *
 * Shown beside the cast because it is the other half of why a step came out the
 * way it did: `loan_limit` decides whether Ada may borrow at all, and a panel
 * that showed her fields but not the limit would leave the deciding value
 * invisible.
 */
export function configOf(world: World | null): Parameter[] {
  if (!world) {
    return [];
  }
  return Object.entries(world.config).map(([key, value]) => {
    const at = key.indexOf(".");
    return at === -1
      ? { module: "", name: key, value }
      : { module: key.slice(0, at), name: key.slice(at + 1), value };
  });
}

/**
 * Which step, if any, first gave `member` an instance.
 *
 * A caught name does not exist until the step that created it, and showing its
 * fields as empty in step one reads as "it has no values" rather than "it does
 * not exist yet".
 */
export function appearsAt(walk: Walk, member: CastMember): number {
  if (member.entity === null) {
    return -1;
  }
  const entity = member.entity;
  const found = walk.steps.findIndex((step) => entity in step.world.entities);
  return found;
}
