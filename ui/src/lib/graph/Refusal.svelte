<script lang="ts">
  // What stands in for a view too large to draw without asking.
  //
  // The app's to show rather than the canvas's: the ways out it offers — the
  // rail, the modules view — are the app's controls, and a canvas reused inside
  // the focus pop-up has neither. See `budget.ts` for why it exists at all.

  import { overBudget } from "./budget";

  interface Props {
    nodes: number;
    edges: number;
    modules: number;
    /** Switch to the modules view; absent when that is the view refused. */
    onoverview?: () => void;
    ondraw: () => void;
  }

  const { nodes, edges, modules, onoverview, ondraw }: Props = $props();

  const words = $derived(overBudget(nodes, edges, modules));
</script>

<div class="refusal" role="status">
  <p class="eyebrow">Not drawn yet</p>
  <p class="prose headline">{words.headline}</p>
  <p class="prose">{words.detail}</p>
  <div class="choices">
    {#if onoverview}
      <button type="button" class="primary" onclick={onoverview}>Open the modules view</button>
    {/if}
    <button type="button" onclick={ondraw}>Draw it anyway</button>
  </div>
</div>

<style>
  /* The same notice shape as the app's "not connected" panel. */
  .refusal {
    display: grid;
    place-content: center;
    justify-items: center;
    text-align: center;
    gap: var(--gap-2);
    padding: var(--gap-6);
    background: var(--ground-canvas);
  }
  .refusal p {
    margin: 0;
    max-width: 44ch;
  }
  .headline {
    color: var(--ink);
  }
  .choices {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--gap-2);
    margin-top: var(--gap-2);
  }
  .choices button {
    padding: 3px var(--gap-3);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    font-size: var(--t-small);
  }
  .choices button:hover {
    color: var(--ink);
    border-color: var(--ink-dim);
  }
  .choices button.primary {
    border-color: var(--behaviour);
    color: var(--behaviour);
  }
</style>
