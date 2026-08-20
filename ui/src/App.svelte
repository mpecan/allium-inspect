<script lang="ts">
  import type { Severity } from "./lib/api/Severity";
  import Canvas from "./lib/graph/Canvas.svelte";
  import {
    isMeaningful,
    journey,
    narrow,
    neighbourhood,
    origins,
    type Trace,
  } from "./lib/graph/trace";
  import { inView, ownerOf, project } from "./lib/graph/views";
  import Inspector from "./lib/panels/Inspector.svelte";
  import SourceStrip from "./lib/panels/SourceStrip.svelte";
  import Rail from "./lib/Rail.svelte";
  import Simulator from "./lib/sim/Simulator.svelte";
  import {
    ApiError,
    InspectClient,
    type ModuleSource,
    type Mode,
    type SpecGraph,
    type ViewKind,
  } from "./lib/client";
  import { positionOf, worstByNode } from "./lib/spec";

  const client = new InspectClient();

  let graph = $state.raw<SpecGraph | null>(null);
  let failure = $state<string | null>(null);
  let mode = $state<Mode>("domain");
  // The last graph view chosen, so leaving the simulator returns you to it.
  let view = $state<ViewKind>("domain");
  let selectedId = $state<string | null>(null);
  let hidden = $state.raw<Set<string>>(new Set());
  let traceMode = $state<"off" | "forward" | "backward" | "near">("off");
  let sourceOpen = $state(false);
  // Off by default. Re-laying out on every click would move the picture out
  // from under a reader who was only looking something up.
  let reflow = $state(false);
  let reveal = $state.raw<{ id: string; nth: number } | null>(null);
  let revealed = 0;
  let sources = $state.raw<Map<string, ModuleSource>>(new Map());

  $effect(() => {
    const controller = new AbortController();
    client
      .spec(controller.signal)
      .then((loaded) => {
        graph = loaded;
        failure = null;
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") {
          return;
        }
        failure =
          error instanceof ApiError ? error.message : "Could not load the specification.";
      });
    return () => controller.abort();
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
   * What the canvas draws: the whole view, or — with reflow on — only what the
   * trace reached, laid out again over that.
   */
  const drawn = $derived(narrow(visibleNodes, visibleEdges, reflow ? trace : null));

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

  /** Resolve a bare construct name from a finding to a node id. */
  function selectByName(name: string) {
    const match = nodes.find((node) => node.name === name);
    if (match) {
      select(match.id);
    }
  }
</script>

<div class="shell">
  <Rail
    {mode}
    {modules}
    {hidden}
    {traceMode}
    hasSelection={selectedId !== null}
    findings={graph?.findings.length ?? 0}
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
    {reflow}
    onreflow={() => (reflow = !reflow)}
  />

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
        nodes={drawn.nodes}
        edges={drawn.edges}
        {severities}
        selected={selectedId}
        {reveal}
        {trace}
        onselect={select}
      />
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
    diagnostics={graph?.diagnostics.filter(
      (diagnostic) =>
        selected !== null &&
        diagnostic.module === selected.module &&
        selectedPosition !== null &&
        diagnostic.location?.line === selectedPosition.line,
    ) ?? []}
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
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar) minmax(0, 1fr) var(--inspector);
    height: 100%;
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
