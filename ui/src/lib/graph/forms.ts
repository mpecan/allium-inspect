// The ways of looking at one construct.
//
// Three of them ask what it is connected to, because a rule and an entity sit
// in the middle of chains that run both ways and a reader is asking about one
// direction at a time. Adjacent is the default: it is the only one that never
// comes back empty for a construct connected to anything at all, so it is the
// answer that always says something.
//
// The fourth looks *inside* rather than out. The lifecycle view on the main
// canvas draws every entity's state machine at once, and the one a reader is
// holding is somewhere in that field of eighty — so it is here too, for the one
// they are holding.
//
// The other three views are not here, and the reason is that they would be the
// same buttons twice. Flow and Journey are chains through a construct, which is
// what Follows and Leads here already are; Domain is what a construct relates
// to, which is Adjacent. Lifecycle was the only one with no per-construct
// equivalent, because a state machine belongs to exactly one entity.

import { journey, neighbourhood, origins, type Trace } from "./trace";
import type { Edge } from "../api/Edge";
import type { Node } from "../api/Node";

/** Which question is being asked of a construct. */
export type Form = "near" | "forward" | "backward" | "lifecycle";

export interface FormOption {
  form: Form;
  label: string;
  /** What the reader gets, phrased as the answer rather than the mechanism. */
  hint: string;
  /**
   * Completes "Nothing in the spec …", for a form with no answer.
   *
   * A separate phrasing rather than reusing the hint, because "Nothing in the
   * spec what this leads to Attestation" is the sort of sentence that only ever
   * happens when one string was asked to do two jobs.
   */
  empty: string;
}

/** In the order they are offered, most general first. */
export const FORMS: readonly FormOption[] = [
  {
    form: "near",
    label: "Adjacent",
    hint: "one step in either direction",
    empty: "is next to",
  },
  // "Leads to" was the label here and it read backwards: a button saying
  // "leads to" next to a construct is naturally read as "this leads to …",
  // which is the other direction entirely. Both labels now name the end of the
  // chain the reader will be shown.
  {
    form: "forward",
    label: "Follows",
    hint: "what happens after this",
    empty: "follows from",
  },
  {
    form: "backward",
    label: "Leads here",
    hint: "what has to happen first",
    empty: "leads to",
  },
  {
    form: "lifecycle",
    label: "Lifecycle",
    hint: "the states it moves between",
    empty: "is a state of",
  },
];

/**
 * Whether `form` has anything to say about `node`.
 *
 * Only the lifecycle is choosy: a state machine belongs to an entity that
 * declares transitions, and offering it for a rule would be offering an empty
 * answer. The three directions apply to anything, and whether they come back
 * empty is a fact about the spec rather than about the kind.
 */
export function applies(form: Form, node: Node): boolean {
  if (form !== "lifecycle") {
    return true;
  }
  return node.detail.type === "entity" && node.detail.transitions.length > 0;
}

/**
 * Walk `edges` from `id` the way `form` asks.
 *
 * The lifecycle is not a walk — it is drawn from the entity's own transition
 * list — so it reaches nothing here and the caller projects it instead.
 */
export function walkForm(form: Form, edges: Edge[], id: string): Trace {
  switch (form) {
    case "forward":
      return journey(edges, id);
    case "backward":
      return origins(edges, id);
    case "lifecycle":
      return { nodes: new Set([id]), edges: new Set(), depth: 0 };
    case "near":
      return neighbourhood(edges, id);
  }
}
