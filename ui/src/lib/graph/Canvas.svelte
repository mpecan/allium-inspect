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
    MarkerType,
    type Node as FlowNode,
  } from "@xyflow/svelte";
  import ELK from "elkjs/lib/elk.bundled.js";
  import { untrack } from "svelte";

  import type { Edge } from "../api/Edge";
  import type { Node } from "../api/Node";
  import type { Severity } from "../api/Severity";
  import type { ViewKind } from "../client";
  import ConstructNode from "./ConstructNode.svelte";
  import ModuleHull from "./ModuleHull.svelte";
  import { hullId } from "./hulls";
  import RoutedEdge from "./RoutedEdge.svelte";
  import Settle from "./Settle.svelte";
  import { layout } from "./layout";
  import type { Point } from "./route";
  import { minimapColour } from "./minimap";
  import { paint } from "./paint";
  import type { Trace } from "./trace";

  interface Props {
    view: ViewKind;
    /**
     * Draw a boundary around each file's constructs.
     *
     * An overlay rather than a layout constraint — see `hulls.ts` for why.
     */
    grouped?: boolean;
    nodes: Node[];
    edges: Edge[];
    severities: Map<string, Severity>;
    selected: string | null;
    /** What to frame, and a count so asking twice frames twice. */
    frame: { ids: string[] | null; nth: number } | null;
    trace: Trace | null;
    onselect: (id: string | null) => void;
    /** A construct the reader asked to look at on its own. */
    onopen?: (id: string) => void;
    /**
     * Drop the overview map. It earns its place on a view of three hundred
     * constructs and is clutter over four, where the whole graph is already on
     * screen and the map covers part of it.
     */
    bare?: boolean;
  }

  const {
    view,
    grouped = false,
    nodes,
    edges,
    severities,
    selected,
    frame,
    trace,
    onselect,
    onopen,
    bare = false,
  }: Props = $props();

  /**
   * Which construct a pointer event landed on, if any.
   *
   * Read off the DOM rather than from Svelte Flow, which has no double-click
   * event for a node — and the node component should not have to know that the
   * canvas has a second thing you can do to it.
   */
  function constructAt(event: MouseEvent): string | null {
    const element = (event.target as HTMLElement | null)?.closest(".svelte-flow__node");
    return element?.getAttribute("data-id") ?? null;
  }

  const elk = new ELK();
  const nodeTypes = { construct: ConstructNode, hull: ModuleHull };
  /**
   * Low enough that "fit" means fit.
   *
   * Routing the edges properly makes a three-hundred-node view considerably
   * taller, and a floor of 0.15 left two thirds of the journey view off the
   * screen with nothing on the canvas to say it was there.
   */
  const MIN_ZOOM = 0.05;
  const edgeTypes = { routed: RoutedEdge };

  // Two arrays, and the split matters. `placed` is ours: what ELK decided, in
  // the order the graph gave. `flowNodes` and `flowEdges` are Svelte Flow's —
  // it is bound to them and writes each node's measured box back into them, and
  // that measurement is what its edge routing depends on.
  let placed = $state.raw<FlowNode[]>([]);
  /** ELK's route for each edge, by its position in `edges`. */
  let routes = $state.raw<Map<number, Point[]>>(new Map());
  let flowNodes = $state.raw<FlowNode[]>([]);
  let flowEdges = $state.raw<FlowEdge[]>([]);
  let placing = $state(true);

  // Re-laying out on every render would fight the user's pan and zoom, so the
  // key is what actually changes the picture: which view, and which nodes.
  // `grouped` is part of the key because the boundaries are built inside the
  // layout promise, and a value only read in an async callback is not a
  // dependency Svelte can see — toggling it changed the checkbox and nothing
  // else.
  const shape = $derived(
    `${view}:${grouped}:${nodes.map((node) => node.id).join(",")}`,
  );

  $effect(() => {
    // Read the key so the effect re-runs when the shape changes, not when a
    // selection does — highlighting is a class change, not a new layout.
    void shape;
    let cancelled = false;
    placing = true;

    const settling = layout(elk, view, nodes, edges, grouped && view !== "modules");
    void settling.then((result) => {
      if (cancelled) {
        return;
      }
      routes = result.routes;
      const byId = new Map(result.nodes.map((node) => [node.id, node]));
      // Behind the constructs, and before them in the array so Svelte Flow
      // paints them first. Not selectable: a boundary is a fact about where
      // things are, and clicking one should reach the construct underneath.
      const boxes: FlowNode[] = (result.groups ?? []).map((box) => ({
        id: hullId(box.module),
        type: "hull",
        position: { x: box.x, y: box.y },
        data: {
          module: box.module,
          held: box.held,
          width: box.width,
          height: box.height,
        },
        selectable: false,
        draggable: false,
        zIndex: -1,
      }));
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
      placed = [...boxes, ...placed];
      placing = false;
    });
    settling.catch((error: unknown) => {
      // A rejected layout used to leave `placing` set for good, and `placing`
      // is what holds the canvas at 35% opacity while ELK thinks. So the
      // failure showed up not as an error but as a graph that had gone ghostly
      // and stayed that way — "the canvas locked up", with nothing on screen
      // saying why.
      //
      // Clearing it leaves the last layout that *did* work, fully visible and
      // usable, which is the most this can honestly offer: a view that could
      // not be laid out has no positions to draw.
      if (cancelled) {
        return;
      }
      placing = false;
      console.error(`could not lay out the ${view} view`, error);
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
        type: "routed",
        data: { points: routes.get(index) },
        source: edge.from,
        target: edge.to,
        // Labelled only on a traced path. A real spec set has hundreds of
        // edges, and a label on every one covers the graph it is describing —
        // the reader is following one chain, and that is the one worth naming.
        //
        // The modules view is the exception, because there the label *is* the
        // content: an edge weighted 16 and an edge weighted 1 are different
        // relationships, and hiding that leaves five boxes joined by arrows
        // that all look alike. It can afford it — one node per file means ten
        // edges where the others have hundreds.
        label: traced || view === "modules" ? edge.label : undefined,
        animated: traced,
        // Opacity mixed into the stroke rather than set as a property: the
        // property applies to the arrowhead too, and an edge that recedes
        // correctly leaves a head too faint to read a direction from. The line
        // stays background; the head does not.
        style: `stroke: ${
          traced
            ? "var(--edge-active)"
            : `color-mix(in srgb, var(--edge) ${lit ? 70 : 14}%, var(--ground-canvas))`
        }; stroke-width: ${traced ? 1.8 : 1}; opacity: ${traced ? 0.9 : 1};`,
        // The label is an HTML element, so the emphasis is `color`.
        labelStyle: traced || view === "modules" ? "color: var(--ink);" : undefined,
        // Every edge in this model is directed, and in one view the direction
        // is the whole content: a Copy goes `available -> on_loan` *and*
        // `on_loan -> available`, and without a head those are one line drawn
        // twice. Sized down from the library default, which is built for a
        // canvas of a dozen edges rather than a spec set with hundreds.
        markerEnd: {
          type: MarkerType.ArrowClosed,
          width: 14,
          height: 14,
          color: traced
            ? "var(--edge-active)"
            : `color-mix(in srgb, var(--edge) ${lit ? 100 : 16}%, var(--ground-canvas))`,
        },
      } satisfies FlowEdge;
    });
  });

  $effect(() => {
    flowEdges = paintedEdges;
  });

</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="canvas"
  class:placing
  ondblclick={(event) => {
    const id = constructAt(event);
    if (id) {
      onopen?.(id);
    }
  }}
>
  {#if nodes.length === 0}
    <p class="empty prose">
      Nothing to draw in this view. Try another view, or turn a module back on.
    </p>
  {:else}
    <!-- `zoomOnDoubleClick` is off because double-clicking opens a construct. -->
    <SvelteFlow
      bind:nodes={flowNodes}
      bind:edges={flowEdges}
      {nodeTypes}
      {edgeTypes}
      fitView
      zoomOnDoubleClick={false}
      minZoom={MIN_ZOOM}
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
      <Settle ids={flowNodes.map((node) => node.id)} {frame} />
      <Controls showLock={false} />
      {#if !bare}
        <MiniMap
          nodeColor={minimapColour}
          bgColor="var(--ground-panel)"
          maskColor="color-mix(in srgb, var(--ground-canvas) 78%, transparent)"
          nodeStrokeWidth={0}
          pannable
          zoomable
        />
      {/if}
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

  /* A plate is scenery, and scenery does not take clicks.
   *
   * Svelte Flow wraps every node in an element of its own, and the wrapper is
   * outside the component — so `pointer-events: none` on the hull itself left
   * the wrapper catching everything. Clicking the empty part of a file's plate
   * fired `onnodeclick` with `archive::hull`, an id no construct has: the
   * selection became it, `paint` marked the plate selected, and Svelte Flow
   * elevated the selected node to `z-index: 999` — so the box a reader clicked
   * to look *into* covered every construct inside it.
   *
   * Inert, the click falls through to the pane, which clears the selection.
   * That is what clicking the background of a file should mean anyway. */
  .canvas :global(.svelte-flow__node-hull) {
    pointer-events: none;
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
