// The event a step is asked to fire, built from what the reader did.
//
// By position, never by name. A trigger is declared by nothing but the rules
// listening for it, and each binds a firing's arguments under the names its own
// `when` clause gives them, in order — the same way `allium-journey walk`
// passes an act. The form's labels are one listener's names, so sending them
// as named arguments as well would bind a second listener that uses the same
// names in another order by the coincidence of the name, and the browser and
// the walk would give two different answers for one act.

import type { Effect } from "../api/Effect";
import type { Event } from "../api/Event";
import type { Value } from "../api/Value";

/** An emission, as a rule's effects carry it. */
export type Emission = Extract<Effect, { kind: "emitted" }>;

/** A trigger fired from the form, its values in the trigger's parameter order. */
export function chosenEvent(trigger: string, module: string, values: Value[]): Event {
  return { trigger, module, arguments: {}, positional: [...values] };
}

/**
 * An emission fired as it was emitted: what it named by name and what it
 * passed by position, exactly. Firing it with nothing — which "follow this"
 * used to do — left every listener with a parameter undecided and told the
 * reader it "carried no arguments" about an emission that plainly did.
 */
export function followedEvent(emission: Emission): Event {
  return {
    trigger: emission.trigger,
    module: emission.module,
    arguments: { ...emission.arguments },
    positional: [...emission.positional],
  };
}
