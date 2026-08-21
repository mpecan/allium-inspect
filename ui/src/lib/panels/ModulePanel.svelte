<script lang="ts">
  // A file, read from the outside.
  //
  // The inspector answers "what is this construct"; this answers "what is this
  // file to the rest of the set". They are different questions with different
  // shapes, which is why this is not a branch inside `Inspector`: a module has
  // no fields, no clauses and no source span, and every part of that panel
  // would be switched off.
  //
  // The list that matters is `exported`. Allium has no `pub`, so a file's
  // interface is whatever the other files reached for — and unlike everything
  // else in the inspector, that is not readable from the declaration. It is
  // only knowable by looking at every edge in the set, which is precisely the
  // work a person should not be doing by hand.

  import type { ModuleReport } from "../graph/modules";

  interface Props {
    report: ModuleReport;
    path: string;
    onselect: (id: string) => void;
  }

  const { report, path, onselect }: Props = $props();

  const reach = $derived(report.exported.length);
</script>

<aside class="panel" aria-label="About this module">
  <p class="kind">module</p>
  <h2>{report.module}</h2>
  {#if path}
    <p class="address">{path}</p>
  {/if}

  <section>
    <h3>What it holds</h3>
    <ul class="census">
      <li><span>constructs</span><span class="n">{report.held}</span></li>
      <li><span>reached for from outside</span><span class="n">{reach}</span></li>
    </ul>
    <p class="prose">
      {#if reach === 0}
        Nothing outside this file refers to anything in it. It is a leaf: change
        it freely, and nothing else in the set has to know.
      {:else}
        {reach} of its {report.held} constructs are named by another file. Those
        are the ones that cannot change quietly.
      {/if}
    </p>
  </section>

  {#if report.neighbours.length > 0}
    <section>
      <h3>Who it touches</h3>
      <ul class="neighbours">
        {#each report.neighbours as near (near.module)}
          <li>
            <span class="name">{near.module}</span>
            <span class="flows">
              {#if near.out > 0}<span class="out">{near.out} out</span>{/if}
              {#if near.into > 0}<span class="into">{near.into} in</span>{/if}
            </span>
          </li>
        {/each}
      </ul>
      {#if report.neighbours.some((near) => near.out > 0 && near.into > 0)}
        <p class="prose">
          A file listed with both directions is one this module and that one
          each reach into. Two files that need each other are one file that has
          not been split, or a dependency going the wrong way.
        </p>
      {/if}
    </section>
  {/if}

  {#if reach > 0}
    <section>
      <h3>Its surface</h3>
      <p class="prose">
        What the rest of the set reaches for. The spec never declares this —
        it is what the other files happen to name.
      </p>
      <ul class="exported">
        {#each report.exported as item (item.node.id)}
          <li>
            <button type="button" onclick={() => onselect(item.node.id)}>
              <span class="eyebrow">{item.node.kind}</span>
              <span class="name">{item.node.name}</span>
            </button>
            <span class="n">{item.count}</span>
            <span class="by">{item.from.join(", ")}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</aside>

<style>
  .panel {
    border-left: 1px solid var(--line);
    background: var(--ground-panel);
    padding: var(--gap-4) var(--gap-3);
    overflow-y: auto;
    min-height: 0;
  }

  .kind {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    letter-spacing: var(--track-wide);
    text-transform: uppercase;
    color: var(--kind-config);
  }

  h2 {
    margin: 0 0 var(--gap-1);
    font-family: var(--font-mono);
    font-size: var(--t-large);
    font-weight: 500;
    color: var(--ink);
  }

  .address {
    margin: 0 0 var(--gap-4);
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    color: var(--ink-faint);
    overflow-wrap: anywhere;
  }

  section {
    margin-bottom: var(--gap-4);
  }

  h3 {
    margin: 0 0 var(--gap-2);
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    letter-spacing: var(--track-wide);
    text-transform: uppercase;
    color: var(--ink-faint);
    font-weight: 500;
  }

  .prose {
    margin: var(--gap-2) 0 0;
    font-size: var(--t-small);
    color: var(--ink-dim);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 1px;
  }

  .census li,
  .neighbours li {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--gap-2);
    font-family: var(--font-mono);
    font-size: var(--t-small);
    color: var(--ink-dim);
    padding: 2px 0;
  }

  .n {
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }

  .neighbours .name {
    color: var(--ink);
  }

  .flows {
    display: flex;
    gap: var(--gap-2);
  }

  /* Direction is the question this panel is for, so the two are not the same
     colour. Out is what this file depends on; in is what depends on it, and
     only one of those is inside the author's control. */
  .out {
    color: var(--kind-trigger);
  }

  .into {
    color: var(--kind-surface);
  }

  /* Two rows, not three columns. The referrers are a list of module names and
     grow with the set — as one column beside the name they took the width and
     squeezed `Identity` into a single character per line. */
  .exported li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: baseline;
    gap: 0 var(--gap-2);
    padding: var(--gap-1) 0;
  }

  .exported button {
    display: flex;
    align-items: baseline;
    gap: var(--gap-2);
    background: none;
    border: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: var(--t-small);
    color: var(--ink);
  }

  .exported button:hover .name,
  .exported button:focus-visible .name {
    text-decoration: underline;
  }

  .exported .eyebrow {
    font-size: var(--t-micro);
    text-transform: uppercase;
    letter-spacing: var(--track-wide);
    color: var(--ink-faint);
    flex-shrink: 0;
  }

  .exported .name {
    overflow-wrap: anywhere;
  }

  .by {
    /* Its own row, spanning both columns. */
    grid-column: 1 / -1;
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    color: var(--ink-faint);
    overflow-wrap: anywhere;
  }
</style>
