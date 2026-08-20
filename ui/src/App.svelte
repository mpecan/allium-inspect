<script lang="ts">
  import type { Severity } from "./lib/api/Severity";
  import Canvas from "./lib/graph/Canvas.svelte";
  import Focus from "./lib/graph/Focus.svelte";
  import {
    isMeaningful,
    journey,
    neighbourhood,
    origins,
    type Trace,
  } from "./lib/graph/trace";
  import { inView, ownerOf, project } from "./lib/graph/views";
  import Inspector from "./lib/panels/Inspector.svelte";
  import SourceStrip from "./lib/panels/SourceStrip.svelte";
  import Rail from "./lib/Rail.svelte";
  import Reports from "./lib/Reports.svelte";
  import Simulator from "./lib/sim/Simulator.svelte";
  import {
    ApiError,
    InspectClient,
    type ModuleSource,
    type Mode,
    type SpecGraph,
    type ViewKind,
  } from "./lib/client";
  import { standing, type Trouble } from "./lib/reload";
  import { positionOf, reportedAgainst, unattributed, worstByNode } from "./lib/spec";

  const client = new InspectClient();

  /** How often to ask the server whether anything changed, in milliseconds. */
  const EVERY = 1000;

  let graph = $state.raw<SpecGraph | null>(null);
  let failure = $state<string | null>(null);
  let mode = $state<Mode>("domain");
  // The last graph view chosen, so leaving the simulator returns you to it.
  let view = $state<ViewKind>("domain");
  let selectedId = $state<string | null>(null);
  let hidden = $state.raw<Set<string>>(new Set());
  let traceMode = $state<"off" | "forward" | "backward" | "near">("off");
  let sourceOpen = $state(false);
  // The construct the reader double-clicked, shown on its own. Null is the
  // ordinary state: the canvas behind it never moves.
  let focused = $state<string | null>(null);
  let reportsOpen = $state(false);
  let reveal = $state.raw<{ id: string; nth: number } | null>(null);
  let revealed = 0;
  let sources = $state.raw<Map<string, ModuleSource>>(new Map());

  /** What the last poll said about the spec, for the banner. */
  let trouble = $state.raw<Trouble | null>(null);
  /** The revision the graph on screen came from. */
  let shown: number | null = null;

  /** Fetch the graph, and remember which revision it is. */
  async function load(revision: number, signal: AbortSignal) {
    try {
      graph = await client.spec(signal);
      shown = revision;
      failure = null;
      // Source is fetched per module on demand and cached; a new revision makes
      // every cached file a file that may no longer say that.
      sources = new Map();
    } catch (error: unknown) {
      if (error instanceof DOMException && error.name === "AbortError") {
        return;
      }
      failure =
        error instanceof ApiError ? error.message : "Could not load the specification.";
    }
  }

  /**
   * Ask once a second whether the answer has changed.
   *
   * See `lib/reload.ts` for why this is a poll. The first answer loads the
   * graph; every one after it only does so when the revision has moved.
   */
  $effect(() => {
    const controller = new AbortController();
    let timer = 0;

    const ask = async () => {
      try {
        const state = standing(shown, await client.health(controller.signal));
        trouble = state.trouble;
        if (state.stale || graph === null) {
          await load(state.revision, controller.signal);
        }
      } catch (error: unknown) {
        if (error instanceof DOMException && error.name === "AbortError") {
          return;
        }
        // The server has gone away — stopped, or the machine slept. The graph
        // on screen is still worth reading, so this says so and keeps asking.
        trouble = {
          headline: "Lost the server.",
          detail: "It may have stopped. This is the last graph it sent.",
          showingOlder: true,
        };
      }
      timer = window.setTimeout(() => void ask(), EVERY);
    };
    void ask();

    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  });

  const nodes = $derived(graph?.nodes ?? []);
  const edges = $derived(graph?.edges ?? []);
  const modules = $derived(graph?.modules ?? []);

  const visible = $derived(project(view, nodes, edges, hidden));
  const visibleNodes = $derived(visible.nodes);
  const visibleEdges = $derived(visible.edges);

  const severities = $derived<Map<string, Severity>>(
    graph ? worstByNode(graph) : new Map(),
  );

  const selected = $derived(nodes.find((node) => node.id === selectedId) ?? null);

  const trace = $derived.by<Trace | null>(() => {
    if (!selectedId || traceMode === "off") {
      return null;
    }
    const walked =
      traceMode === "forward"
        ? journey(visibleEdges, selectedId)
        : traceMode === "backward"
          ? origins(visibleEdges, selectedId)
          : neighbourhood(visibleEdges, selectedId);

    // A trace of one node is the selection itself. Dimming three hundred boxes
    // to highlight the one already highlighted tells the reader nothing and
    // costs them the context — so the canvas stays as it was and the rail says
    // there was nothing to follow.
    return isMeaningful(walked) ? walked : null;
  });

  const traceIsEmpty = $derived(
    selectedId !== null && traceMode !== "off" && trace === null,
  );

  /**
   * Everything the reader has not switched off.
   *
   * The pop-up asks about a construct rather than about the current view, so it
   * gets the whole spec set less the hidden modules — a module checkbox is a
   * deliberate "not this", and a view is only a way of looking.
   */
  const present = $derived(nodes.filter((node) => !hidden.has(node.module)));
  const between = $derived.by(() => {
    const shown = new Set(present.map((node) => node.id));
    return edges.filter((edge) => shown.has(edge.from) && shown.has(edge.to));
  });

  /** What no construct carries, and therefore has nowhere else to be read. */
  const loose = $derived(unattributed(graph?.diagnostics ?? []));
  const findings = $derived(graph?.findings ?? []);

  const focusedNode = $derived(
    focused === null ? null : (nodes.find((node) => node.id === focused) ?? null),
  );

  const selectedModule = $derived(
    selected ? modules.find((module) => module.name === selected.module) : undefined,
  );
  const selectedSource = $derived(
    selected ? (sources.get(selected.module)?.text ?? "") : "",
  );
  const selectedPosition = $derived(
    selected && selectedSource ? positionOf(selectedSource, selected.span) : null,
  );

  // Source is fetched per module, on demand and once. Sending every spec file
  // with the graph would multiply the payload for text the reader may never
  // open, and re-fetching on every selection would flicker the strip.
  $effect(() => {
    const module = selected?.module;
    if (!module || sources.has(module)) {
      return;
    }
    void client
      .source(module)
      .then((loaded) => {
        sources = new Map(sources).set(module, loaded);
      })
      .catch(() => {
        /* The strip falls back to showing nothing; the graph is still usable. */
      });
  });

  function toggleModule(name: string) {
    const next = new Set(hidden);
    if (next.has(name)) {
      next.delete(name);
    } else {
      next.add(name);
    }
    hidden = next;
  }

  function select(id: string | null) {
    // A state pill on the lifecycle canvas stands for its entity: the state is
    // a value inside a transition list, not a construct with source of its own.
    selectedId = id === null ? null : ownerOf(id);
    if (id === null) {
      traceMode = "off";
    }
  }

  /** The views a graph node could be drawn in, in the order the rail lists them. */
  const VIEWS: ViewKind[] = ["domain", "flow", "lifecycle", "journey"];

  /**
   * Show `id`, wherever it is.
   *
   * A search result can name a construct in a hidden module, or one this view
   * does not draw, or one laid out well off the screen. Selecting it and
   * leaving all three in the way would look like nothing happened, so each is
   * cleared in turn before the canvas is asked to move to it.
   */
  function find(id: string) {
    const node = nodes.find((candidate) => candidate.id === id);
    if (!node) {
      return;
    }
    if (hidden.has(node.module)) {
      const next = new Set(hidden);
      next.delete(node.module);
      hidden = next;
    }
    if (mode === "simulate" || !inView(node, view)) {
      const home = VIEWS.find((candidate) => inView(node, candidate));
      if (home) {
        view = home;
        mode = home;
      }
    }
    select(id);
    revealed += 1;
    reveal = { id, nth: revealed };
  }

  /**
   * Show the construct a finding names.
   *
   * The analyser reports names, not ids, and two modules can both declare a
   * `Device` — so the finding's own module breaks the tie before falling back
   * to whichever one there is. It goes through `find` rather than selecting
   * directly, because a construct in a switched-off module or in a view that
   * does not draw its kind would otherwise be selected invisibly.
   */
  function selectByName(name: string, module?: string) {
    const candidates = nodes.filter((node) => node.name === name);
    const match = candidates.find((node) => node.module === module) ?? candidates[0];
    if (match) {
      find(match.id);
    }
  }
</script>

<div class="shell" class:troubled={trouble !== null}>
  {#if trouble}
    <!-- Across the top, above everything, because it is a statement about
         whether the rest of the screen can be believed. -->
    <p class="trouble" class:older={trouble.showingOlder} role="status">
      <strong>{trouble.headline}</strong>
      {#if trouble.detail}<span class="detail">{trouble.detail}</span>{/if}
    </p>
  {/if}

  <Rail
    {mode}
    {modules}
    {hidden}
    {traceMode}
    hasSelection={selectedId !== null}
    reports={findings.length + loose.length}
    version={graph?.allium_version ?? ""}
    onmode={(next) => {
      mode = next;
      if (next !== "simulate") {
        view = next;
      }
    }}
    onmodule={toggleModule}
    ontrace={(mode) => (traceMode = mode)}
    {traceIsEmpty}
    {nodes}
    onfind={find}
    onreports={() => (reportsOpen = true)}
  />

  {#if reportsOpen}
    <Reports
      {findings}
      {loose}
      onselect={(name, module) => {
        reportsOpen = false;
        selectByName(name, module);
      }}
      onclose={() => (reportsOpen = false)}
    />
  {/if}

  {#if mode === "simulate"}
    <div class="stage">
      <Simulator {client} />
    </div>
  {:else}
  <main>
    {#if failure}
      <div class="failure">
        <p class="eyebrow">Not connected</p>
        <p class="prose">{failure}</p>
      </div>
    {:else if !graph}
      <div class="failure">
        <p class="eyebrow">Reading the specification</p>
        <p class="prose">Running allium over the spec set.</p>
      </div>
    {:else}
      <Canvas
        {view}
        nodes={visibleNodes}
        edges={visibleEdges}
        {severities}
        selected={selectedId}
        {reveal}
        {trace}
        onselect={select}
        onopen={(id) => {
          select(id);
          focused = id;
        }}
      />

      {#if focusedNode}
        <Focus
          node={focusedNode}
          nodes={present}
          edges={between}
          {severities}
          onselect={select}
          onopen={(id) => {
            select(id);
            focused = id;
          }}
          onclose={() => (focused = null)}
        />
      {/if}
    {/if}

    <SourceStrip
      path={selectedModule?.path ?? "no selection"}
      text={selectedSource}
      span={selected?.span ?? null}
      open={sourceOpen}
      ontoggle={() => (sourceOpen = !sourceOpen)}
    />
  </main>
  {/if}

  {#if mode !== "simulate"}
  <Inspector
    node={selected}
    position={selectedPosition}
    modulePath={selectedModule?.path ?? ""}
    diagnostics={reportedAgainst(graph?.diagnostics ?? [], selected)}
    findings={graph?.findings.filter(
      (finding) =>
        selected !== null &&
        (finding.entities.includes(selected.name) ||
          finding.rules.includes(selected.name)),
    ) ?? []}
    obligations={graph?.obligations.filter(
      (obligation) =>
        selected !== null &&
        obligation.module === selected.module &&
        (obligation.construct === selected.name ||
          obligation.construct.startsWith(`${selected.name}.`)),
    ) ?? []}
    onselect={selectByName}
  />
  {/if}
</div>

<style>
  /* A statement about whether the rest of the screen can be believed, so it is
   * above the rest of the screen rather than beside it. Two colours, because
   * the two failures ask different things of the reader: amber for a spec that
   * has errors and is still being drawn honestly, red for a graph that is from
   * before the edit and describes a file that no longer exists. */
  .trouble {
    grid-column: 1 / -1;
    display: flex;
    align-items: baseline;
    gap: var(--gap-2);
    margin: 0;
    padding: var(--gap-2) var(--gap-3);
    font-size: var(--t-small);
    color: var(--ink);
    background: var(--verdict-unknown-fill);
    border-bottom: 1px solid var(--verdict-unknown);
  }
  .trouble.older {
    background: color-mix(in srgb, var(--verdict-false) 22%, var(--ground-panel));
    border-bottom-color: var(--verdict-false);
  }
  .trouble .detail {
    color: var(--ink-dim);
  }

  .shell {
    display: grid;
    grid-template-columns: var(--sidebar) minmax(0, 1fr) var(--inspector);
    height: 100%;
  }
  /* The banner takes a row of its own; without it the three columns are the
   * only row and the grid has nowhere to put it. */
  .shell.troubled {
    grid-template-rows: auto minmax(0, 1fr);
  }

  /* The simulator replaces the canvas and the inspector both, so the shell
   * gives it everything but the rail. */
  .shell:has(.stage) {
    grid-template-columns: var(--sidebar) minmax(0, 1fr);
  }

  .stage {
    min-width: 0;
    min-height: 0;
  }

  main {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    min-width: 0;
  }

  .failure {
    display: grid;
    place-content: center;
    text-align: center;
    gap: var(--gap-2);
    padding: var(--gap-6);
  }

  .failure .prose {
    max-width: 44ch;
  }

  @media (max-width: 1100px) {
    .shell {
      grid-template-columns: var(--sidebar) minmax(0, 1fr);
    }
    .shell :global(.inspector) {
      display: none;
    }
  }
</style>
