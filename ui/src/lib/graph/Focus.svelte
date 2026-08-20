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
  import { FORMS, walkForm, type Form } from "./forms";
  import { narrow } from "./trace";

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

  // All three, so a form that has nothing to show says so before it is clicked
  // rather than after. Three walks over a few hundred edges is nothing.
  const answers = $derived(
    new Map(FORMS.map((option) => [option.form, walkForm(option.form, edges, node.id)])),
  );
  const trace = $derived(answers.get(form) ?? walkForm(form, edges, node.id));
  const graph = $derived(narrow(nodes, edges, trace));
  const alone = $derived(graph.nodes.length <= 1);

  /** How many constructs a form would show, not counting this one. */
  const connected = (option: Form) => (answers.get(option)?.nodes.size ?? 1) - 1;

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
              ? `Nothing in the spec ${option.empty} ${node.name}`
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
        Nothing in the spec {FORMS.find((option) => option.form === form)?.empty}
        <strong>{node.name}</strong> — or nothing in the modules that are still
        switched on.
      </p>
    {:else}
      {#key form}
        <Canvas
          view="flow"
          nodes={graph.nodes}
          edges={graph.edges}
          {severities}
          selected={node.id}
          reveal={null}
          trace={null}
          onselect={(id) => id !== null && onselect(id)}
          {onopen}
          bare
        />
      {/key}
    {/if}
  </div>

  <p class="prose caveat">
    {graph.nodes.length - 1} connected · double-click a construct to look at that
    one instead
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
