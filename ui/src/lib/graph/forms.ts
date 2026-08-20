// The ways of asking "what is this construct connected to?".
//
// Three questions rather than one, because a rule and an entity sit in the
// middle of chains that run both ways and a reader is asking about one
// direction at a time. Adjacent is the default: it is the only one that never
// comes back empty for a construct that is connected to anything at all, so it
// is the answer that always says something.

import { journey, neighbourhood, origins, type Trace } from "./trace";
import type { Edge } from "../api/Edge";

/** Which question is being asked of a construct. */
export type Form = "near" | "forward" | "backward";

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
];

/** Walk `edges` from `id` the way `form` asks. */
export function walkForm(form: Form, edges: Edge[], id: string): Trace {
  switch (form) {
    case "forward":
      return journey(edges, id);
    case "backward":
      return origins(edges, id);
    case "near":
      return neighbourhood(edges, id);
  }
}
