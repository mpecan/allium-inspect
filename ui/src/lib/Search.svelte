<script lang="ts">
  // Finding a construct without knowing where it lives.
  //
  // The list is keyboard-first: the arrows move, Enter opens, Escape gets out.
  // A reader who has just typed six characters has their hands on the keys, and
  // making them reach for the mouse to take the first result is the sort of
  // thing that makes a tool feel like a form.

  import type { Node } from "./api/Node";
  import { search, SHOWN } from "./search";

  interface Props {
    nodes: Node[];
    onpick: (id: string) => void;
  }

  const { nodes, onpick }: Props = $props();

  let query = $state("");
  let active = $state(0);
  let field = $state<HTMLInputElement | null>(null);

  const matches = $derived(search(nodes, query));
  const shown = $derived(matches.slice(0, SHOWN));

  // A new query is a new list, so the highlight goes back to the top rather
  // than staying on whichever row happens to be in that position now.
  $effect(() => {
    void query;
    active = 0;
  });

  function pick(index: number) {
    const match = shown[index];
    if (match) {
      onpick(match.node.id);
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      query = "";
      field?.blur();
      return;
    }
    if (shown.length === 0) {
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      active = (active + step + shown.length) % shown.length;
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      pick(active);
    }
  }
</script>

<svelte:window
  onkeydown={(event) => {
    // `/` is the search key everywhere a page is mostly reading, and this page
    // is mostly reading. Not while something else already has the keys.
    const target = event.target as HTMLElement | null;
    const typing = target?.tagName === "INPUT" || target?.tagName === "SELECT";
    if (event.key === "/" && !typing && !event.metaKey && !event.ctrlKey) {
      event.preventDefault();
      field?.focus();
    }
  }}
/>

<section class="find">
  <h2><label for="find-construct">Find</label></h2>
  <input
    id="find-construct"
    type="search"
    bind:this={field}
    bind:value={query}
    placeholder="name, kind or module"
    autocomplete="off"
    spellcheck="false"
    {onkeydown}
  />

  {#if query.trim().length > 0}
    {#if shown.length === 0}
      <p class="prose caveat">
        Nothing in this spec set is called that. Search matches a construct's
        name, its kind, or the module it is in.
      </p>
    {:else}
      <ul class="results">
        {#each shown as match, index (match.node.id)}
          <li>
            <button
              type="button"
              class:active={index === active}
              onmouseenter={() => (active = index)}
              onclick={() => pick(index)}
            >
              <span class="found">{match.node.name}</span>
              <span class="where">{match.node.kind} · {match.node.module}</span>
            </button>
          </li>
        {/each}
      </ul>
      {#if matches.length > shown.length}
        <p class="address more">
          {shown.length} of {matches.length} — keep typing to narrow it
        </p>
      {/if}
    {/if}
  {/if}
</section>

<style>
  input {
    width: 100%;
    margin-top: var(--gap-1);
    padding: 3px var(--gap-2);
    font: inherit;
    font-size: var(--t-small);
    color: var(--ink);
    background: var(--ground-input);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  input:focus-visible {
    outline: 1px solid var(--behaviour);
    outline-offset: 1px;
  }
  input::placeholder {
    color: var(--ink-faint);
  }

  .results {
    list-style: none;
    margin: var(--gap-2) 0 0;
    padding: 0;
  }

  .results button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 2px var(--gap-2);
    border-radius: var(--radius);
    border-left: 2px solid transparent;
  }
  .results button.active {
    background: var(--ground-raised);
    border-left-color: var(--behaviour);
  }

  .found {
    display: block;
    font-size: var(--t-small);
    color: var(--ink);
    overflow-wrap: anywhere;
  }
  /* Two constructs can share a name across modules, so where it lives is part
   * of which one this is rather than decoration. */
  .where {
    display: block;
    font-size: var(--t-micro);
    color: var(--ink-faint);
  }

  .more {
    margin: var(--gap-1) 0 0;
  }
</style>
