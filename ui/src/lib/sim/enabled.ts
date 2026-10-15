// How a "Now possible" entry reads.
//
// Two different things arrive in that list. A state rule holds over the world:
// `Copy.status = lost` is true of every lost copy. A creation rule is woken by
// one instance having just been made, which is not a condition over anything,
// and labelling it `state` beside "these rules watch the world" said of it the
// one thing it is not.

import type { Enabled } from "../api/Enabled";
import { render } from "./values";

/** What the step's simulator spells a creation's trigger with, at the end. */
const CREATED = ".created";

export interface Offered {
  /** The small label above the rule's name. */
  eyebrow: string;
  /** What it is about, in words, or nothing when it is about nobody. */
  about: string | null;
}

/** How one entry of `newly_enabled` is described. */
export function offered(rule: Enabled): Offered {
  const over = rule.over.map((value) => render(value).text).join(", ");
  if (rule.trigger.endsWith(CREATED)) {
    return {
      eyebrow: "created",
      about: over ? `reacts to ${over} being created` : null,
    };
  }
  return { eyebrow: rule.source, about: over ? `over ${over}` : null };
}
