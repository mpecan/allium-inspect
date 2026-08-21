<script lang="ts">
  // The left rail: which view, which modules, and how far to follow a chain.
  //
  // Views are listed with what each one answers rather than by name alone. A
  // reader opening this tool for the first time does not know what "flow" means
  // here, and a five-word subtitle costs less than the click it saves.

  import type { Module } from "./api/Module";
  import type { Node } from "./api/Node";
  import type { Mode, ViewKind } from "./client";
  import { ANSWERS } from "./graph/views";
  import Search from "./Search.svelte";

  type TraceMode = "off" | "forward" | "backward" | "near";

  interface Props {
    /// Which mode is showing. The rail switches between them; `view` in the app
    /// remembers the last *graph* one so leaving the simulator returns to it.
    mode: Mode;
    modules: Module[];
    hidden: Set<string>;
    traceMode: TraceMode;
    hasSelection: boolean;
    traceIsEmpty: boolean;
    /** Analysis findings plus anything reported against no construct. */
    reports: number;
    version: string;
    /** Every construct in the spec set, for search — not only the drawn ones. */
    nodes: Node[];
    onmode: (mode: Mode) => void;
    onmodule: (name: string) => void;
    ontrace: (mode: TraceMode) => void;
    onfind: (id: string) => void;
    onreports: () => void;
  }

  const {
    mode,
    modules,
    hidden,
    traceMode,
    hasSelection,
    traceIsEmpty,
    reports,
    version,
    nodes,
    onmode,
    onmodule,
    ontrace,
    onfind,
    onreports,
  }: Props = $props();

  const VIEWS: { kind: ViewKind; name: string }[] = [
    { kind: "domain", name: "Domain" },
    { kind: "flow", name: "Flow" },
    { kind: "lifecycle", name: "Lifecycle" },
    { kind: "chain", name: "Chain" },
    { kind: "modules", name: "Modules" },
  ];

  // Each carries what it does on screen rather than in a `title`. The view
  // buttons four lines above always did, and the difference showed: a reader
  // watching over someone's shoulder never sees a tooltip, and someone who has
  // not met the word "trace" has no way to find out what these four are for.
  const TRACES: { mode: TraceMode; label: string; hint: string }[] = [
    { mode: "off", label: "All", hint: "the whole view" },
    // Not "Leads to", which reads as "this leads to …" — the other direction.
    { mode: "forward", label: "Follows", hint: "what happens after this" },
    { mode: "backward", label: "Leads here", hint: "what has to happen first" },
    { mode: "near", label: "Adjacent", hint: "one step either way" },
  ];
</script>

<nav aria-label="Views and filters">
  <header>
    <h1>allium<span>inspect</span></h1>
    {#if version}<p class="address">{version}</p>{/if}
  </header>

  <Search {nodes} onpick={onfind} />

  <section>
    <h2>View</h2>
    <ul class="views">
      {#each VIEWS as option (option.kind)}
        <li>
          <button
            type="button"
            class:current={mode === option.kind}
            aria-current={mode === option.kind ? "page" : undefined}
            onclick={() => onmode(option.kind)}
          >
            <span class="view-name">{option.name}</span>
            <span class="view-answers">{ANSWERS[option.kind]}</span>
          </button>
        </li>
      {/each}
      <li>
        <button
          type="button"
          class:current={mode === "journeys"}
          aria-current={mode === "journeys" ? "page" : undefined}
          onclick={() => onmode("journeys")}
        >
          <span class="view-name">Journeys</span>
          <span class="view-answers">what someone set out to do</span>
        </button>
      </li>
      <li>
        <button
          type="button"
          class:current={mode === "simulate"}
          aria-current={mode === "simulate" ? "page" : undefined}
          onclick={() => onmode("simulate")}
        >
          <span class="view-name">Simulate</span>
          <span class="view-answers">fire a trigger and watch</span>
        </button>
      </li>
    </ul>
  </section>

  {#if mode !== "simulate" && mode !== "journeys"}
  <section>
    <h2>Trace</h2>
    <ul class="traces">
      {#each TRACES as option (option.mode)}
        <li>
          <button
            type="button"
            class:current={traceMode === option.mode}
            disabled={!hasSelection && option.mode !== "off"}
            title={hasSelection ? undefined : "Select a construct to trace from it"}
            onclick={() => ontrace(option.mode)}
          >
            <span class="trace-name">{option.label}</span>
            <span class="trace-hint">{option.hint}</span>
          </button>
        </li>
      {/each}
    </ul>
    {#if traceIsEmpty}
      <p class="prose caveat empty-trace">
        Nothing follows from this one in this view. Try another direction, or
        the Flow view, which carries more of the chain.
      </p>
    {:else}
      <p class="prose caveat">
        A chain is derived from which triggers a surface offers and which each
        rule emits — so it is what <em>follows</em>, not what anyone set out to
        do. For that, somebody has to write it down: see Journeys.
      </p>
    {/if}
  </section>

  {/if}

  <section>
    <h2>Modules</h2>
    <p class="prose caveat modules-hint">
      {mode === "simulate"
        ? "Which triggers to offer. Switching one off hides it here; it does not stop its rules firing."
        : "What to draw."}
    </p>
    <ul class="modules">
      {#each modules as module (module.name)}
        <li>
          <label>
            <input
              type="checkbox"
              checked={!hidden.has(module.name)}
              onchange={() => onmodule(module.name)}
            />
            <span>{module.name}</span>
            {#if module.imports.some((i) => !i.target)}
              <span class="unresolved-mark" title="an import did not resolve">↯</span>
            {/if}
          </label>
        </li>
      {:else}
        <li class="address">none loaded</li>
      {/each}
    </ul>
  </section>

  <footer>
    <button type="button" class="reports" onclick={onreports} disabled={reports === 0}>
      {#if reports === 0}
        nothing reported
      {:else}
        {reports} thing{reports === 1 ? "" : "s"} reported
      {/if}
    </button>
  </footer>

</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    gap: var(--gap-4);
    padding: var(--gap-3);
    height: 100%;
    overflow-y: auto;
    background: var(--ground-panel);
    border-right: 1px solid var(--line);
  }

  h1 {
    margin: 0;
    font-size: var(--t-body);
    font-weight: 500;
    letter-spacing: var(--track-tight);
  }
  /* The two halves of the name set apart rather than hyphenated: the tool
   * inspects allium, and the type says so without punctuation. */
  h1 span {
    color: var(--behaviour);
  }
  h1 span::before {
    content: "·";
    margin: 0 0.35ch;
    color: var(--ink-faint);
  }

  header .address {
    margin: 1px 0 0;
  }

  h2 {
    margin: 0 0 var(--gap-2);
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    font-weight: 500;
    color: var(--ink-faint);
  }

  section {
    border-top: 1px solid var(--line);
    padding-top: var(--gap-3);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .views button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 3px var(--gap-2);
    border-radius: var(--radius);
    border-left: 2px solid transparent;
  }
  .views button:hover {
    background: var(--ground-raised);
  }
  .views button.current {
    border-left-color: var(--behaviour);
    background: var(--ground-raised);
  }

  .view-name {
    display: block;
    font-size: var(--t-small);
  }
  .view-answers {
    display: block;
    font-size: var(--t-micro);
    color: var(--ink-faint);
  }
  .views button.current .view-name {
    color: var(--behaviour);
  }

  .traces {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px;
  }
  .traces button {
    display: block;
    padding: 3px var(--gap-2);
    text-align: left;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    width: 100%;
  }
  .trace-name {
    display: block;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  .trace-hint {
    display: block;
    font-size: var(--t-micro);
    line-height: 1.3;
    color: var(--ink-faint);
  }
  .traces button:hover:not(:disabled) {
    border-color: var(--line-strong);
    color: var(--ink);
  }
  .traces button.current .trace-name {
    color: var(--behaviour);
  }
  .traces button.current {
    border-color: var(--behaviour);
    color: var(--behaviour);
  }
  .traces button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .empty-trace {
    color: var(--boundary);
  }

  /* The count used to be a paragraph, which meant the five conflicts the
   * analyser found were reachable only by guessing which construct to select. */
  .reports {
    width: 100%;
    text-align: left;
    padding: 2px 0;
    font-size: var(--t-micro);
    letter-spacing: var(--track-tight);
    color: var(--ink-dim);
  }
  .reports:not(:disabled):hover {
    color: var(--ink);
  }
  .reports:disabled {
    color: var(--ink-faint);
    cursor: default;
  }

  .modules-hint {
    margin: 0 0 var(--gap-2);
  }

  .caveat {
    margin: var(--gap-2) 0 0;
    font-size: var(--t-micro);
    line-height: 1.5;
    color: var(--ink-faint);
  }

  .modules label {
    display: flex;
    align-items: center;
    gap: var(--gap-2);
    padding: 2px 0;
    font-size: var(--t-small);
    cursor: pointer;
  }
  .modules input {
    accent-color: var(--behaviour);
    margin: 0;
  }

  .unresolved-mark {
    margin-left: auto;
    color: var(--severity-warning);
  }

  footer {
    margin-top: auto;
    border-top: 1px solid var(--line);
    padding-top: var(--gap-2);
  }
</style>
