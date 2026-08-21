<script lang="ts">
  // How a construct looks on the canvas.
  //
  // Eleven node kinds do not need eleven components. What differs between them
  // is which family colours them, what shape they take, and which two or three
  // rows are worth showing at this size — all of which are data. A component
  // per kind would be eleven places to change when the header gains a badge.
  //
  // Shape carries the kinds within a family, because colour is already carrying
  // the family: an entity is square, a value type is rounded, an enum is a
  // pill, and an unresolved reference is a dashed outline with no fill so it
  // reads as an absence rather than as another kind of thing.
  //
  // Separate from ConstructNode so it holds no dependency on the graph library.
  // Svelte Flow's `Handle` refuses to render outside a node context, which made
  // the whole appearance of a construct untestable without mounting a canvas —
  // and the bug that motivated splitting this out was a duplicate list key that
  // took down every node on screen.

  import type { Node as SpecNode } from "../api/Node";
  import type { Severity } from "../api/Severity";
  import { PILL_ROWS, familyOf } from "./layout";
  import { summaryRows } from "./rows";

  interface Props {
    node: SpecNode;
    severity?: Severity | null;
    dimmed?: boolean;
    selected?: boolean;
  }

  const { node, severity = null, dimmed = false, selected = false }: Props = $props();

  const family = $derived(familyOf(node.kind));
  const rows = $derived(summaryRows(node));
  // See `PILL_ROWS`: past a few values a stadium is a lens rather than a pill,
  // and the rows nearest its top and bottom are clipped by its own outline.
  const lens = $derived(node.kind === "enum" && rows.length > PILL_ROWS);
</script>

<div
  class="construct {family} kind-{node.kind}"
  class:selected
  class:lens
  class:dimmed={dimmed}
>
  <header>
    <span class="kind">{node.kind}</span>
    {#if severity}
      <span
        class="severity {severity}"
        title="{severity} reported against this construct"
        aria-label="{severity} reported against this construct"
      ></span>
    {/if}
  </header>

  <p class="name">{node.name}</p>

  {#if rows.length > 0}
    <ul class="rows">
      <!-- Keyed by position, not by label. A rule that creates two entities
           produces two rows labelled `creates`, and a duplicate key is a
           runtime exception in Svelte 5 that takes the whole canvas down. These
           lists are re-derived wholesale on every change, so position is both
           correct and the only key guaranteed unique. -->
      {#each rows as row, index (index)}
        <li class:muted={row.muted}>
          <span class="row-label">{row.label}</span>
          {#if row.value}<span class="row-value">{row.value}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .construct {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 96px;
    padding: 5px 9px 7px;
    border: 1px solid var(--node-edge);
    background: var(--node-fill);
    border-radius: var(--radius);
    color: var(--ink);
    transition:
      box-shadow 120ms ease,
      opacity 120ms ease;
  }

  /* One accent per kind, and the fill and edge mixed from it against the
   * canvas. Writing all three out per kind would be thirty-three values kept
   * in step by hand, and the first one to drift would be a node that reads as
   * a different family than its border says. */
  .thing,
  .behaviour,
  .boundary,
  .constraint {
    --node-fill: color-mix(in srgb, var(--node-accent) 17%, var(--ground-canvas));
    --node-edge: color-mix(in srgb, var(--node-accent) 62%, var(--ground-canvas));
  }

  /* The band each kind belongs to, as a fallback. A kind the language gains
   * before this file hears about it still lands in the right band rather than
   * rendering with no accent at all. */
  .thing {
    --node-accent: var(--thing);
  }
  .behaviour {
    --node-accent: var(--behaviour);
  }
  .boundary {
    --node-accent: var(--boundary);
  }
  .constraint {
    --node-accent: var(--constraint);
  }

  /* And the kind itself, which is what a reader actually tells apart. A
   * trigger beside a rule and an enum beside an entity are the two pairs that
   * carried one colour between them, and they are the two that co-occur most:
   * triggers and rules fill the Flow view, entities and enums the Domain. */
  .kind-entity {
    --node-accent: var(--kind-entity);
  }
  .kind-value {
    --node-accent: var(--kind-value);
  }
  .kind-variant {
    --node-accent: var(--kind-variant);
  }
  .kind-enum {
    --node-accent: var(--kind-enum);
  }
  .kind-rule {
    --node-accent: var(--kind-rule);
  }
  .kind-trigger {
    --node-accent: var(--kind-trigger);
  }
  .kind-surface {
    --node-accent: var(--kind-surface);
  }
  .kind-actor {
    --node-accent: var(--kind-actor);
  }
  .kind-invariant {
    --node-accent: var(--kind-invariant);
  }
  .kind-config {
    --node-accent: var(--kind-config);
  }

  .unresolved {
    --node-edge: var(--unresolved);
    --node-fill: transparent;
    --node-accent: var(--unresolved);
    border-style: dashed;
  }

  /* Form distinguishes the kinds inside a family. */
  .kind-value,
  .kind-variant {
    border-radius: var(--radius-round);
  }
  /* A stadium, not a circle. The ends are semicircles, so the corners a
   * rectangle would have are inside the curve — text measured as if the box
   * were square sits on the outline. The extra room is horizontal because that
   * is where the curve takes it, and `PILL_WIDTH`/`PILL_HEIGHT` in layout.ts
   * are these numbers: ELK trusts the size it is given. */
  .kind-enum {
    border-radius: 999px;
    padding: 9px 24px 11px;
  }
  /* Enough values that the stadium stopped being a pill. Still the roundest
   * thing on the canvas — the shape vocabulary survives — but the corners come
   * back, so the last row is not sitting on the outline and the box is not
   * mostly empty. */
  .kind-enum.lens {
    border-radius: 20px;
    padding: 9px 15px 11px;
  }
  .kind-trigger {
    /* A trigger is a moment, not a thing: the clipped corner marks it as the
     * one construct on the canvas that has no extent in the spec — it is named
     * by the rules that emit and await it, and declared nowhere. */
    border-radius: var(--radius);
    clip-path: polygon(8px 0, 100% 0, 100% 100%, 0 100%, 0 8px);
  }
  .kind-invariant {
    border-left-width: 3px;
  }

  .selected {
    box-shadow:
      0 0 0 1px var(--node-accent),
      0 0 0 4px color-mix(in srgb, var(--node-accent) 22%, transparent);
  }

  .dimmed {
    opacity: 0.24;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--gap-2);
  }

  .kind {
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--node-accent);
  }

  .severity {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    flex: none;
  }
  .severity.error {
    background: var(--severity-error);
  }
  .severity.warning {
    background: var(--severity-warning);
  }
  .severity.info {
    background: var(--severity-info);
  }

  /* The name carries the accent too, not only the kind label above it.
   * The label is six microtype letters; the name is the largest thing on the
   * box, and it is what a reader is scanning for when they are looking for
   * "the triggers" rather than for one construct by name. Lightened toward the
   * ink so it stays a name being read rather than a colour being shown. */
  .name {
    margin: 0;
    font-size: var(--t-title);
    letter-spacing: var(--track-tight);
    line-height: 1.2;
    color: color-mix(in srgb, var(--node-accent) 38%, var(--ink));
  }

  .rows {
    margin: 3px 0 0;
    padding: 3px 0 0;
    list-style: none;
    border-top: 1px solid color-mix(in srgb, var(--node-edge) 55%, transparent);
    font-size: var(--t-micro);
    /* `MAX_ROW_WIDTH` in layout.ts, which measures a row as `min(width, 240)`.
     * The two are one decision: without the cap here a long row widens the box
     * past the size ELK was told, and ELK spaces nodes by the size it was told.
     * On the rows rather than on the node, because the *name* is deliberately
     * uncapped — a construct whose name is clipped cannot be identified, which
     * is the one thing a box on a graph is for. */
    max-width: 240px;
    /* And `min-width: 0`, because a flex item's automatic minimum size is its
     * *content* — which silently outranks the cap above and was why the box
     * still grew. */
    min-width: 0;
  }

  .rows li {
    display: flex;
    justify-content: space-between;
    gap: var(--gap-3);
    color: var(--ink-dim);
    white-space: nowrap;
    overflow: hidden;
  }

  /* The label is ellipsised too, not only the value. An enum's rows *are* its
   * values, and a value is a label with nothing beside it — so without this one
   * long state name runs straight out of the box, past the size ELK was given.
   * `min-width: 0` is what lets a flex item shrink below its content at all. */
  .row-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rows li.muted {
    color: var(--ink-faint);
    font-style: italic;
  }

  .row-value {
    color: var(--ink-faint);
    flex: none;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 11ch;
  }
</style>
