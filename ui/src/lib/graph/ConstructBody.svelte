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
  import { familyOf } from "./layout";
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
</script>

<div
  class="construct {family} kind-{node.kind}"
  class:selected
  class:dimmed={dimmed}
>
  <header>
    <span class="kind">{node.kind}</span>
    {#if severity}
      <span
        class="severity {severity}"
        title="{severity} reported in this module"
        aria-label="{severity} reported in this module"
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

  .thing {
    --node-edge: var(--thing-edge);
    --node-fill: var(--thing-fill);
    --node-accent: var(--thing);
  }
  .behaviour {
    --node-edge: var(--behaviour-edge);
    --node-fill: var(--behaviour-fill);
    --node-accent: var(--behaviour);
  }
  .boundary {
    --node-edge: var(--boundary-edge);
    --node-fill: var(--boundary-fill);
    --node-accent: var(--boundary);
  }
  .constraint {
    --node-edge: var(--constraint-edge);
    --node-fill: var(--constraint-fill);
    --node-accent: var(--constraint);
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
  .kind-enum {
    border-radius: 999px;
    padding-inline: 14px;
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

  .name {
    margin: 0;
    font-size: var(--t-title);
    letter-spacing: var(--track-tight);
    line-height: 1.2;
  }

  .rows {
    margin: 3px 0 0;
    padding: 3px 0 0;
    list-style: none;
    border-top: 1px solid color-mix(in srgb, var(--node-edge) 55%, transparent);
    font-size: var(--t-micro);
  }

  .rows li {
    display: flex;
    justify-content: space-between;
    gap: var(--gap-3);
    color: var(--ink-dim);
    white-space: nowrap;
  }

  .rows li.muted {
    color: var(--ink-faint);
    font-style: italic;
  }

  .row-value {
    color: var(--ink-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 11ch;
  }
</style>
