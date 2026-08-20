<script lang="ts">
  import type { Node } from "./lib/api/Node";
  import type { Severity } from "./lib/api/Severity";
  import Canvas from "./lib/graph/Canvas.svelte";
  import { journey, neighbourhood, origins, type Trace } from "./lib/graph/trace";
  import Inspector from "./lib/panels/Inspector.svelte";
  import SourceStrip from "./lib/panels/SourceStrip.svelte";
  import Rail from "./lib/Rail.svelte";
  import {
    ApiError,
    InspectClient,
    type ModuleSource,
    type SpecGraph,
    type ViewKind,
  } from "./lib/client";
  import { positionOf, worstByNode } from "./lib/spec";

  const client = new InspectClient();

  let graph = $state.raw<SpecGraph | null>(null);
  let failure = $state<string | null>(null);
  let view = $state<ViewKind>("domain");
  let selectedId = $state<string | null>(null);
  let hidden = $state.raw<Set<string>>(new Set());
  let traceMode = $state<"off" | "forward" | "backward" | "near">("off");
  let sourceOpen = $state(false);
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

  const visibleNodes = $derived(
    nodes.filter((node) => !hidden.has(node.module) && inView(node, view)),
  );
  const visibleIds = $derived(new Set(visibleNodes.map((node) => node.id)));
  const visibleEdges = $derived(
    edges.filter((edge) => visibleIds.has(edge.from) && visibleIds.has(edge.to)),
  );

  const severities = $derived<Map<string, Severity>>(
    graph ? worstByNode(graph) : new Map(),
  );

  const selected = $derived(nodes.find((node) => node.id === selectedId) ?? null);

  const trace = $derived.by<Trace | null>(() => {
    if (!selectedId || traceMode === "off") {
      return null;
    }
    if (traceMode === "forward") return journey(visibleEdges, selectedId);
    if (traceMode === "backward") return origins(visibleEdges, selectedId);
    return neighbourhood(visibleEdges, selectedId);
  });

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

  /** Which constructs belong in a given view. */
  function inView(node: Node, kind: ViewKind): boolean {
    switch (kind) {
      case "domain":
        return ["entity", "value", "variant", "enum", "config", "external"].includes(
          node.kind,
        );
      case "flow":
        return ["rule", "trigger", "entity", "value", "variant"].includes(node.kind);
      case "lifecycle":
        return (
          node.kind === "entity" &&
          node.detail.type === "entity" &&
          node.detail.transitions.length > 0
        );
      case "journey":
        return ["surface", "actor", "trigger", "rule", "entity"].includes(node.kind);
    }
  }

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
    selectedId = id;
    if (id === null) {
      traceMode = "off";
    }
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
    {view}
    {modules}
    {hidden}
    {traceMode}
    hasSelection={selectedId !== null}
    findings={graph?.findings.length ?? 0}
    version={graph?.allium_version ?? ""}
    onview={(next) => (view = next)}
    onmodule={toggleModule}
    ontrace={(mode) => (traceMode = mode)}
  />

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
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar) minmax(0, 1fr) var(--inspector);
    height: 100%;
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
