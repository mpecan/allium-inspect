<script lang="ts">
  // The left rail: which view, which modules, and how far to follow a chain.
  //
  // Views are listed with what each one answers rather than by name alone. A
  // reader opening this tool for the first time does not know what "flow" means
  // here, and a five-word subtitle costs less than the click it saves.

  import type { Module } from "./api/Module";
  import type { ViewKind } from "./client";

  type TraceMode = "off" | "forward" | "backward" | "near";

  interface Props {
    view: ViewKind;
    modules: Module[];
    hidden: Set<string>;
    traceMode: TraceMode;
    hasSelection: boolean;
    findings: number;
    version: string;
    onview: (view: ViewKind) => void;
    onmodule: (name: string) => void;
    ontrace: (mode: TraceMode) => void;
  }

  const {
    view,
    modules,
    hidden,
    traceMode,
    hasSelection,
    findings,
    version,
    onview,
    onmodule,
    ontrace,
  }: Props = $props();

  const VIEWS: { kind: ViewKind; name: string; answers: string }[] = [
    { kind: "domain", name: "Domain", answers: "what the spec holds" },
    { kind: "flow", name: "Flow", answers: "what happens, and in what order" },
    { kind: "lifecycle", name: "Lifecycle", answers: "how each entity changes state" },
    { kind: "journey", name: "Journey", answers: "what follows from an action" },
  ];

  const TRACES: { mode: TraceMode; label: string; hint: string }[] = [
    { mode: "off", label: "All", hint: "Show the whole view" },
    { mode: "forward", label: "Follows", hint: "What this leads to" },
    { mode: "backward", label: "Leads to", hint: "What leads to this" },
    { mode: "near", label: "Adjacent", hint: "One step in either direction" },
  ];
</script>

<nav aria-label="Views and filters">
  <header>
    <h1>allium<span>inspect</span></h1>
    {#if version}<p class="address">{version}</p>{/if}
  </header>

  <section>
    <h2>View</h2>
    <ul class="views">
      {#each VIEWS as option (option.kind)}
        <li>
          <button
            type="button"
            class:current={view === option.kind}
            aria-current={view === option.kind ? "page" : undefined}
            onclick={() => onview(option.kind)}
          >
            <span class="view-name">{option.name}</span>
            <span class="view-answers">{option.answers}</span>
          </button>
        </li>
      {/each}
    </ul>
  </section>

  <section>
    <h2>Trace</h2>
    <ul class="traces">
      {#each TRACES as option (option.mode)}
        <li>
          <button
            type="button"
            class:current={traceMode === option.mode}
            disabled={!hasSelection && option.mode !== "off"}
            title={hasSelection
              ? option.hint
              : "Select a construct to trace from it"}
            onclick={() => ontrace(option.mode)}
          >
            {option.label}
          </button>
        </li>
      {/each}
    </ul>
    <p class="prose caveat">
      A trace is derived from which triggers a surface offers and which each rule
      emits. Allium has no journey construct, so this is what follows — not what
      a person does.
    </p>
  </section>

  <section>
    <h2>Modules</h2>
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

  {#if findings > 0}
    <footer>
      <p class="address">
        {findings} analysis finding{findings === 1 ? "" : "s"}
      </p>
    </footer>
  {/if}
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
    padding: 3px var(--gap-2);
    font-size: var(--t-micro);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    color: var(--ink-dim);
    width: 100%;
  }
  .traces button:hover:not(:disabled) {
    border-color: var(--line-strong);
    color: var(--ink);
  }
  .traces button.current {
    border-color: var(--behaviour);
    color: var(--behaviour);
  }
  .traces button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
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
