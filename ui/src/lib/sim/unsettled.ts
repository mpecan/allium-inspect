// What a step left undecided that a fired rule's card and the world panel
// would otherwise not say.
//
// Two siblings woken by one trigger that write one field differently both
// fire, and the field is left unknown, naming both (issue #24). Without this
// the trace showed two clean `set` lines and the world a bare `unknown`: an
// unknown with no reason anywhere on the page, which stipulation 1 calls
// indistinguishable from a bug. The same for a field written through a call
// nobody answered (issue #27).

import type { Effect } from "../api/Effect";
import type { Instance } from "../api/Instance";
import type { Unresolved } from "../api/Unresolved";
import type { World } from "../api/World";

/**
 * Why `field` of `instance` holds something undecided, as the write that left
 * it so said — or nothing, when no write said.
 */
export function whyUndecided(instance: Instance | undefined, field: string): Unresolved[] {
  return instance?.why?.[field] ?? [];
}

/**
 * Why an assignment a rule made does not stand in the world the step left:
 * it wrote a settled value, and the field holds an undecided one with a
 * reason. Empty when the write stood — or when it wrote the unknown itself,
 * which the effect already shows.
 */
export function notSettled(effect: Effect, world: World): Unresolved[] {
  if (effect.kind !== "assigned" || effect.to.kind === "unknown") {
    return [];
  }
  const instance = world.entities[effect.id];
  if (instance?.fields[effect.field]?.kind !== "unknown") {
    return [];
  }
  return whyUndecided(instance, effect.field);
}
