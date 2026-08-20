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
  import { untrack } from "svelte";

  import type { Edge } from "../api/Edge";
  import type { Node } from "../api/Node";
  import type { Severity } from "../api/Severity";
  import type { ViewKind } from "../client";
  import ConstructNode from "./ConstructNode.svelte";
  import Settle from "./Settle.svelte";
  import { familyOf, layout } from "./layout";
  import { paint } from "./paint";
  import type { Trace } from "./trace";

  interface Props {
    view: ViewKind;
    nodes: Node[];
    edges: Edge[];
    severities: Map<string, Severity>;
    selected: string | null;
    /** A node to move the viewport to, and a count so asking twice moves twice. */
    reveal: { id: string; nth: number } | null;
    trace: Trace | null;
    onselect: (id: string | null) => void;
  }

  const { view, nodes, edges, severities, selected, reveal, trace, onselect }: Props =
    $props();

  const elk = new ELK();
  const nodeTypes = { construct: ConstructNode };

  // Two arrays, and the split matters. `placed` is ours: what ELK decided, in
  // the order the graph gave. `flowNodes` and `flowEdges` are Svelte Flow's —
  // it is bound to them and writes each node's measured box back into them, and
  // that measurement is what its edge routing depends on.
  let placed = $state.raw<FlowNode[]>([]);
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

    void layout(elk, view, nodes, edges).then((result) => {
      if (cancelled) {
        return;
      }
      const byId = new Map(result.nodes.map((node) => [node.id, node]));
      placed = nodes.map((node) => {
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

  // Dimming and edge emphasis repaint without a relayout — the picture must not
  // move when you ask what follows from something, or you lose the place you
  // were reading. This writes into the array Svelte Flow owns rather than
  // deriving a new one, because a derived array is read-only and Svelte Flow's
  // own writes to it — the measurements it takes of each node — would be lost.
  $effect(() => {
    flowNodes = paint(placed, untrack(() => flowNodes), selected, trace);
  });

  const paintedEdges = $derived.by(() => {
    const onPath = new Set(
      trace ? [...trace.edges].map((edge) => `${edge.from}->${edge.to}`) : [],
    );
    return edges.map((edge, index) => {
      const key = `${edge.from}->${edge.to}`;
      const traced = trace !== null && onPath.has(key);
      const lit = trace === null || traced;
      return {
        id: `e${index}:${key}`,
        source: edge.from,
        target: edge.to,
        // Labelled only on a traced path. A real spec set has hundreds of
        // edges, and a label on every one covers the graph it is describing —
        // the reader is following one chain, and that is the one worth naming.
        label: traced ? edge.label : undefined,
        animated: traced,
        style: `stroke: ${traced ? "var(--edge-active)" : "var(--edge)"}; stroke-width: ${
          traced ? 1.8 : 1
        }; opacity: ${lit ? 0.55 : 0.12};`,
        // The label is an HTML element, so the emphasis is `color`.
        labelStyle: traced ? "color: var(--ink);" : undefined,
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
      bind:nodes={flowNodes}
      bind:edges={flowEdges}
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
      <Settle ids={flowNodes.map((node) => node.id)} focus={reveal} />
      <Controls showLock={false} />
      <MiniMap
        nodeColor={minimapColour}
        bgColor="var(--ground-panel)"
        maskColor="color-mix(in srgb, var(--ground-canvas) 78%, transparent)"
        nodeStrokeWidth={0}
        pannable
        zoomable
      />
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
  /* The minimap ships an opaque light background and a light mask; both read
   * as a hole punched in the canvas until they are told otherwise. */
  .canvas :global(.svelte-flow__minimap-svg) {
    background: var(--ground-panel);
  }
  .canvas :global(.svelte-flow__minimap-mask) {
    fill: color-mix(in srgb, var(--ground-canvas) 72%, transparent);
    stroke: var(--line-strong);
    stroke-width: 1;
  }

  /* Edge labels are drawn only on a traced path, so they can afford to be
   * legible rather than apologetic. The plate behind them is styled here
   * because the edge type has no prop for it: left alone it ships as an opaque
   * white block, which on a dark canvas is a hole rather than a label — and
   * with the label's own colour on top of it, an unreadable one.
   *
   * These are HTML rather than SVG, so it is `background` and `color`, not
   * `fill`. */
  .canvas :global(.svelte-flow__edge-label) {
    padding: 0 3px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: 9px;
    line-height: 1.5;
    color: var(--ink-dim);
    background: var(--ground-canvas);
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
