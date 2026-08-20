<script lang="ts">
  // Settling the canvas onto a new set of nodes.
  //
  // Two things have to happen whenever the drawn set changes, and neither
  // happens on its own once the canvas has mounted.
  //
  // Measure. Svelte Flow measures a node when its element is first observed and
  // routes every edge from those measurements — a node it has not measured has
  // no handle bounds, and an edge with no handle to attach to is not drawn at
  // all. Nodes here arrive well after the canvas mounts, because ELK lays them
  // out asynchronously, and that first observation does not reliably arrive.
  // The symptom is a graph that draws every construct and not one relationship
  // between them.
  //
  // Fit. `fitView` on <SvelteFlow> frames the nodes it mounted with and nothing
  // after. Switching from a 272-node view to a 12-node one, or turning a module
  // off, would otherwise leave the reader looking at the empty space where the
  // old graph used to be.
  //
  // Framing one node is the same job, which is why it is here rather than in a
  // component of its own: a search result can be anywhere in a three-hundred
  // node layout, and two components each calling `fitView` would take it in
  // turns to undo each other.
  //
  // Must be rendered inside <SvelteFlow>, which is where the store lives.

  import { untrack } from "svelte";

  import { useStore, useUpdateNodeInternals } from "@xyflow/svelte";

  interface Props {
    /** The ids currently drawn. */
    ids: string[];
    /**
     * One node to frame instead of all of them, and a count so that asking for
     * the same node twice frames it twice.
     */
    focus: { id: string; nth: number } | null;
  }

  const { ids, focus }: Props = $props();

  /**
   * Closest the canvas will zoom on its own.
   *
   * A reflowed trace can be two constructs, and fitting two boxes to a
   * 1500-pixel canvas draws them at four times life size — which reads as a
   * mistake rather than as a small answer. Zooming in past this stays the
   * reader's decision.
   */
  const CLOSE = 1.1;

  /** The last focus honoured, so a later reshape frames everything again. */
  let framed = 0;

  const store = useStore();
  const update = useUpdateNodeInternals();

  // The trigger is the drawn *set*, not the array. Svelte Flow hands back a new
  // array every time it records a measurement, and settling on that would be
  // settling on our own last settle.
  const drawn = $derived(ids.join("\u0000"));

  /** Frames to keep asking for. Half a second; a canvas that has not settled
   *  by then is not going to, and a loop with no end is worse than a gap. */
  const PATIENCE = 30;

  $effect(() => {
    void drawn;
    // Tracked, so asking to frame a node that is already drawn is enough on its
    // own to move the viewport.
    const wanted = focus;
    const targets = untrack(() => ids);
    if (targets.length === 0) {
      return;
    }

    let frame = 0;
    // A frame late, and again until it takes. Measuring looks each node up in
    // the DOM by id, and on the pass that hands Svelte Flow a new set they are
    // not painted yet — a lookup that finds nothing settles nothing, and there
    // is no second trigger to fall back on.
    //
    // Everything inside is untracked: all of it reads and writes the canvas's
    // own node array, and tracked, settling would be its own trigger.
    const settle = (left: number) => {
      frame = requestAnimationFrame(() => {
        untrack(() => {
          // Measured *and* showing what was asked for. `nodesInitialized` alone
          // still reads true for the set the canvas is about to replace, and
          // framing against that set frames the wrong thing once and never
          // retries.
          const showing =
            store.nodesInitialized &&
            store.nodeLookup.size === targets.length &&
            (wanted === null || store.nodeLookup.has(wanted.id));
          if (showing) {
            if (wanted !== null && wanted.nth > framed) {
              framed = wanted.nth;
              void store.fitView({
                nodes: [{ id: wanted.id }],
                maxZoom: CLOSE,
                duration: 220,
              });
            } else {
              void store.fitView({ maxZoom: CLOSE });
            }
            return;
          }
          update(targets);
          if (left > 0) {
            settle(left - 1);
          }
        });
      });
    };
    settle(PATIENCE);

    return () => cancelAnimationFrame(frame);
  });
</script>
