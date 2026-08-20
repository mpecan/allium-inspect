<script lang="ts">
  // One construct, and what it is connected to.
  //
  // Double-clicking a box opens this. It answers the question the main canvas
  // cannot answer without being rearranged — "what is this joined to?" — and it
  // answers it somewhere else, so the view behind it does not move. That was
  // the objection to doing it in place: a reader who was reading the graph does
  // not want the graph to reflow under them.
  //
  // The three forms are the three directions the question runs in, and each is
  // laid out and framed on its own. Double-clicking inside moves the pop-up to
  // that construct, which is how you walk a chain a step at a time without ever
  // losing the canvas you started from.

  import type { Edge } from "../api/Edge";
  import type { Node } from "../api/Node";
  import type { Severity } from "../api/Severity";
  import Canvas from "./Canvas.svelte";
  import { applies, FORMS, walkForm, type Form } from "./forms";
  import { narrow } from "./trace";
  import { project } from "./views";

  interface Props {
    /** The construct this is a view of. */
    node: Node;
    /**
     * Every construct the reader has not switched off — not the current view.
     *
     * A view is a filter, and two of the three forms ask about causation, which
     * the domain view draws none of. Answering "what leads to this?" with
     * "nothing, in this view" would be true and useless.
     */
    nodes: Node[];
    edges: Edge[];
    severities: Map<string, Severity>;
    onselect: (id: string) => void;
    onopen: (id: string) => void;
    onclose: () => void;
  }

  const { node, nodes, edges, severities, onselect, onopen, onclose }: Props = $props();

  let form = $state<Form>("near");
  let panel = $state<HTMLDialogElement | null>(null);

  // Every form up front, so one with nothing to show says so before it is
  // clicked rather than after. Four small walks over a few hundred edges is
  // nothing, and the lifecycle is not a walk at all.
  const answers = $derived(
    new Map(FORMS.map((option) => [option.form, drawFor(option.form)])),
  );
  const graph = $derived(
    answers.get(form) ?? { nodes: [] as Node[], edges: [] as Edge[] },
  );
  const alone = $derived(graph.nodes.length <= 1);

  /**
   * What one form draws.
   *
   * The lifecycle is projected from the entity's own transition list rather
   * than walked — the same projection the main canvas uses, given one entity
   * instead of all of them.
   */
  function drawFor(option: Form): { nodes: Node[]; edges: Edge[] } {
    if (!applies(option, node)) {
      return { nodes: [], edges: [] };
    }
    if (option === "lifecycle") {
      return project("lifecycle", [node], []);
    }
    return narrow(nodes, edges, walkForm(option, edges, node.id));
  }

  /** How much a form would show, not counting the construct itself. */
  const connected = (option: Form) =>
    Math.max(0, (answers.get(option)?.nodes.length ?? 1) - 1);

  /** Which layout a form wants: a state machine reads differently from a chain. */
  const laidOutAs = $derived(form === "lifecycle" ? "lifecycle" : "flow");

  // Modal rather than an overlay of our own: it takes the focus, it returns it
  // on close, and Escape works without anyone implementing Escape.
  $effect(() => {
    panel?.showModal();
  });
</script>

<dialog
  bind:this={panel}
  aria-label="{node.kind} {node.name}"
  onclose={onclose}
  onclick={(event) => {
    // The backdrop is the dialog itself; anything inside stops here.
    if (event.target === panel) {
      panel?.close();
    }
  }}
>
  <header>
    <div class="what">
      <p class="eyebrow">{node.kind}</p>
      <h2>{node.name}</h2>
      <p class="address">{node.qualified}</p>
    </div>

    <ul class="forms">
      {#each FORMS as option (option.form)}
        <li>
          <button
            type="button"
            class:current={form === option.form}
            aria-current={form === option.form ? "true" : undefined}
            disabled={connected(option.form) === 0}
            title={connected(option.form) === 0
              ? option.form === "lifecycle"
                ? `${node.name} declares no transitions`
                : `Nothing in the spec ${option.empty} ${node.name}`
              : `${connected(option.form)} — ${option.hint}`}
            onclick={() => (form = option.form)}
          >
            {option.label}
          </button>
        </li>
      {/each}
    </ul>

    <button type="button" class="close" title="Close" onclick={() => panel?.close()}>
      ×
    </button>
  </header>

  <div class="stage">
    {#if alone}
      <p class="prose empty">
        {#if form === "lifecycle"}
          <strong>{node.name}</strong> declares no transitions, so it has no
          states to move between.
        {:else}
          Nothing in the spec {FORMS.find((option) => option.form === form)?.empty}
          <strong>{node.name}</strong> — or nothing in the modules that are still
          switched on.
        {/if}
      </p>
    {:else}
      {#key form}
        <Canvas
          view={laidOutAs}
          nodes={graph.nodes}
          edges={graph.edges}
          {severities}
          selected={node.id}
          frame={null}
          trace={null}
          onselect={(id) => id !== null && onselect(id)}
          {onopen}
          bare
        />
      {/key}
    {/if}
  </div>

  <p class="prose caveat">
    {#if form === "lifecycle"}
      {connected("lifecycle")} states
    {:else}
      {connected(form)} connected · double-click a construct to look at that one
      instead
    {/if}
  </p>
</dialog>

<style>
  dialog {
    width: min(90vw, 1200px);
    height: min(82vh, 820px);
    padding: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    color: var(--ink);
    background: var(--ground-panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
  }
  dialog::backdrop {
    background: color-mix(in srgb, var(--ground-canvas) 72%, transparent);
  }

  header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: start;
    gap: var(--gap-3);
    padding: var(--gap-3);
    border-bottom: 1px solid var(--line);
  }

  h2 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 500;
    color: var(--behaviour);
  }

  .forms {
    display: flex;
    gap: var(--gap-1);
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .forms button {
    padding: 2px var(--gap-2);
    font-size: var(--t-small);
    color: var(--ink-dim);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  .forms button:disabled {
    color: var(--ink-faint);
    border-color: transparent;
    cursor: default;
  }
  .forms button.current {
    color: var(--behaviour);
    border-color: var(--behaviour);
    background: var(--ground-raised);
  }

  .close {
    font-size: 1.1rem;
    line-height: 1;
    padding: 0 var(--gap-2);
    color: var(--ink-faint);
  }
  .close:hover {
    color: var(--ink);
  }

  .stage {
    min-height: 0;
    position: relative;
  }

  .empty {
    padding: var(--gap-4);
    color: var(--ink-dim);
  }

  .caveat {
    margin: 0;
    padding: var(--gap-2) var(--gap-3);
    font-size: var(--t-micro);
    color: var(--ink-faint);
    border-top: 1px solid var(--line);
  }
</style>
