// The shapes the simulator's own routes return.
//
// Hand-written rather than generated, because these three are the server's
// *presentation* of the spec for one screen — which entity types the editor can
// make, which triggers a person can fire — rather than part of the model. They
// have no Rust counterpart worth exporting and would only add three files to a
// generated directory whose value is that everything in it is derived.

import type { World } from "../api/World";

/** An entity the world editor can make instances of. */
export interface EntityChoice {
  entity: string;
  module: string;
  fields: FieldChoice[];
}

/** One field of an entity, and what the editor should offer for it. */
export interface FieldChoice {
  name: string;
  type_expr: string;
  /** The states it may take, when it is a status field. */
  states: string[];
  /** Computed by the spec rather than by the simulator. */
  derived: boolean;
}

/** A trigger the user can fire. */
export interface Fireable {
  trigger: string;
  module: string;
  parameters: string[];
  /** The surface that offers it, when one does. */
  surface: string | null;
  /** The actor that surface faces. */
  actor: string | null;
}

/** Everything a fresh simulation needs. */
export interface Setup {
  world: World;
  entities: EntityChoice[];
  triggers: Fireable[];
  /** Every state the spec declares, for reading a typed trigger argument. */
  states: string[];
}

/**
 * The triggers from modules the reader has not switched off.
 *
 * The module checkboxes were inert here — present, enabled, and doing nothing —
 * while filtering the canvas four inches above. They mean the same thing in
 * both places: what to *show*. Nothing about the specification changes, so a
 * rule in a hidden module still fires when its trigger does; what changes is
 * whether this thirty-surface list offers it to you.
 */
export function offered(
  triggers: Fireable[],
  hidden: ReadonlySet<string>,
): Fireable[] {
  return triggers.filter((trigger) => !hidden.has(trigger.module));
}

/**
 * Triggers grouped by where they come from.
 *
 * Surfaces first and named, because a trigger a surface provides is something a
 * *person* does and those are the honest places to start. The rest are grouped
 * under a heading that says as much — you can fire them, and doing so means
 * starting in the middle of something.
 */
export function grouped(
  triggers: Fireable[],
): { label: string; triggers: Fireable[] }[] {
  const groups = new Map<string, Fireable[]>();

  for (const trigger of triggers) {
    const label = trigger.surface
      ? `${trigger.surface}${trigger.actor ? ` · ${trigger.actor}` : ""}`
      : "Emitted elsewhere";
    const existing = groups.get(label);
    if (existing) {
      existing.push(trigger);
    } else {
      groups.set(label, [trigger]);
    }
  }

  // Insertion order, which the server already put surfaces first in — so the
  // grouping does not have to re-derive a priority the server decided.
  return [...groups].map(([label, triggers]) => ({ label, triggers }));
}

/**
 * What tells one entity choice from another: its module and its name.
 *
 * The name alone is not enough. Two modules may each declare an `Order`, and a
 * list keyed by the bare name had two rows with one key — which Svelte answers
 * by unmounting the whole simulator, not just the list.
 */
export function choiceKey(
  choice: Pick<EntityChoice, "entity" | "module">,
): string {
  return `${choice.module}/${choice.entity}`;
}

/**
 * How a choice is labelled: bare when its name is the only one of its kind,
 * and with its module when another module declares the same name, because
 * two rows reading `Order` are a choice nobody can make.
 */
export function choiceLabel(
  choice: EntityChoice,
  entities: EntityChoice[],
): string {
  const shared =
    entities.filter((other) => other.entity === choice.entity).length > 1;
  return shared ? choiceKey(choice) : choice.entity;
}

/** The choice an instance was made from: its entity, in its module. */
export function choiceOf(
  entities: EntityChoice[],
  entity: string,
  module: string,
): EntityChoice | undefined {
  return entities.find(
    (candidate) => candidate.entity === entity && candidate.module === module,
  );
}
