<script lang="ts">
  // The graph, laid out and drawn.
  //
  // Svelte Flow owns pan, zoom, selection and edge routing; this owns the
  // translation from a spec graph into its node and edge shapes, and the
  // dimming that a trace applies. The layout itself is ELK's, run whenever the
  // view or the visible set changes — positions are computed rather than
  // interactive, because a spec graph has a correct reading order and letting a
  // reader drag nodes around would only let them lose it.

  import {
    Background,
    BackgroundVariant,
    Controls,
    MiniMap,
    SvelteFlow,
    type Edge as FlowEdge,
    type Node as FlowNode,
  } from "@xyflow/svelte";
  import ELK from "elkjs/lib/elk.bundled.js";

  import type { Edge } from "../api/Edge";
  import type { Node } from "../api/Node";
  import type { Severity } from "../api/Severity";
  import type { ViewKind } from "../client";
  import ConstructNode from "./ConstructNode.svelte";
  import { familyOf, layout } from "./layout";
  import type { Trace } from "./trace";

  interface Props {
    view: ViewKind;
    nodes: Node[];
    edges: Edge[];
    severities: Map<string, Severity>;
    selected: string | null;
    trace: Trace | null;
    onselect: (id: string | null) => void;
  }

  const { view, nodes, edges, severities, selected, trace, onselect }: Props =
    $props();

  const elk = new ELK();
  const nodeTypes = { construct: ConstructNode };

  let flowNodes = $state.raw<FlowNode[]>([]);
  let flowEdges = $state.raw<FlowEdge[]>([]);
  let placing = $state(true);

  // Re-laying out on every render would fight the user's pan and zoom, so the
  // key is what actually changes the picture: which view, and which nodes.
  const shape = $derived(`${view}:${nodes.map((node) => node.id).join(",")}`);

  $effect(() => {
    // Read the key so the effect re-runs when the shape changes, not when a
    // selection does — highlighting is a class change, not a new layout.
    void shape;
    let cancelled = false;
    placing = true;

    void layout(elk, view, nodes, edges).then((placed) => {
      if (cancelled) {
        return;
      }
      const byId = new Map(placed.nodes.map((node) => [node.id, node]));
      flowNodes = nodes.map((node) => {
        const place = byId.get(node.id);
        return {
          id: node.id,
          type: "construct",
          position: { x: place?.x ?? 0, y: place?.y ?? 0 },
          data: { node, severity: severities.get(node.id) ?? null, dimmed: false },
          selectable: true,
          draggable: false,
        } satisfies FlowNode;
      });
      placing = false;
    });

    return () => {
      cancelled = true;
    };
  });

  // Dimming and edge emphasis are derived, so a trace repaints without a
  // relayout — the picture must not move when you ask what follows from
  // something, or you lose the place you were reading.
  const painted = $derived.by(() => {
    const dim = trace !== null;
    return flowNodes.map((node) => ({
      ...node,
      selected: node.id === selected,
      data: {
        ...node.data,
        dimmed: dim && !trace.nodes.has(node.id),
      },
    }));
  });

  const paintedEdges = $derived.by(() => {
    const onPath = new Set(
      trace ? [...trace.edges].map((edge) => `${edge.from}->${edge.to}`) : [],
    );
    return edges.map((edge, index) => {
      const key = `${edge.from}->${edge.to}`;
      const lit = trace === null || onPath.has(key);
      return {
        id: `e${index}:${key}`,
        source: edge.from,
        target: edge.to,
        label: edge.label,
        animated: trace !== null && onPath.has(key),
        style: `stroke: ${lit ? "var(--edge-active)" : "var(--edge)"}; stroke-width: ${
          trace !== null && onPath.has(key) ? 1.8 : 1
        }; opacity: ${lit ? 1 : 0.18};`,
        labelStyle: `fill: var(--ink-faint); font-size: 9px; opacity: ${lit ? 1 : 0.15};`,
      } satisfies FlowEdge;
    });
  });

  $effect(() => {
    flowEdges = paintedEdges;
  });

  function minimapColour(node: FlowNode): string {
    const construct = (node.data as { node: Node }).node;
    return `var(--${familyOf(construct.kind)})`;
  }
</script>

<div class="canvas" class:placing>
  {#if nodes.length === 0}
    <p class="empty prose">
      Nothing to draw in this view. Try another view, or turn a module back on.
    </p>
  {:else}
    <SvelteFlow
      nodes={painted}
      edges={flowEdges}
      {nodeTypes}
      fitView
      minZoom={0.15}
      maxZoom={2.5}
      proOptions={{ hideAttribution: false }}
      onnodeclick={({ node }) => onselect(node.id)}
      onpaneclick={() => onselect(null)}
    >
      <Background
        variant={BackgroundVariant.Dots}
        gap={22}
        size={1}
        bgColor="var(--ground-canvas)"
        patternColor="var(--ground-canvas-grid)"
      />
      <Controls showLock={false} />
      <MiniMap nodeColor={minimapColour} pannable zoomable />
    </SvelteFlow>
  {/if}
</div>

<style>
  .canvas {
    position: relative;
    height: 100%;
    min-height: 0;
    background: var(--ground-canvas);
  }

  /* A brief, quiet fade while ELK places the graph. Without it the canvas
   * flashes through an unlaid-out pile on every view change. */
  .canvas.placing :global(.svelte-flow__renderer) {
    opacity: 0.35;
  }
  .canvas :global(.svelte-flow__renderer) {
    transition: opacity 160ms ease;
  }

  .canvas :global(.svelte-flow__node) {
    cursor: pointer;
  }

  .canvas :global(.svelte-flow__handle) {
    opacity: 0;
    pointer-events: none;
  }

  .canvas :global(.svelte-flow__controls-button) {
    background: var(--ground-raised);
    border-bottom: 1px solid var(--line);
    fill: var(--ink-dim);
  }

  .canvas :global(.svelte-flow__minimap) {
    background: var(--ground-panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  .canvas :global(.svelte-flow__attribution) {
    background: transparent;
    font-size: 9px;
  }
  .canvas :global(.svelte-flow__attribution a) {
    color: var(--ink-faint);
  }

  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-content: center;
    text-align: center;
    max-width: 30ch;
    margin: auto;
  }
</style>
