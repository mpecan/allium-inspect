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
  // Framing *part* of what is drawn is the same job, which is why it is here
  // rather than in a component of its own: two components each calling
  // `fitView` would take it in turns to undo each other. A search result is one
  // node; a highlight is a set of them; a view switch is all of them.
  //
  // Must be rendered inside <SvelteFlow>, which is where the store lives.

  import { untrack } from "svelte";

  import { useStore, useUpdateNodeInternals } from "@xyflow/svelte";

  interface Props {
    /** The ids currently drawn. */
    ids: string[];
    /**
     * What to frame, and a count so that asking for the same thing twice frames
     * it twice.
     *
     * `ids: null` means everything that is drawn. A subset is what a highlight
     * asks for: showing which twelve of three hundred a chain reached, and then
     * leaving them at the scale of the three hundred, tells the reader where
     * they are but not what they found.
     */
    frame: { ids: string[] | null; nth: number } | null;
  }

  const { ids, frame }: Props = $props();

  const store = useStore();
  const update = useUpdateNodeInternals();

  // The trigger is the drawn *set*, not the array. Svelte Flow hands back a new
  // array every time it records a measurement, and settling on that would be
  // settling on our own last settle.
  const drawn = $derived(ids.join("\u0000"));

  /**
   * Closest the canvas will zoom on its own.
   *
   * A reflowed chain can be two constructs, and fitting two boxes to a
   * 1500-pixel canvas draws them at four times life size — which reads as a
   * mistake rather than as a small answer. Zooming in past this stays the
   * reader's decision.
   */
  const CLOSE = 1.1;

  /** How long to wait between attempts, and how many to make. */
  const STEP = 16;
  const PATIENCE = 30;

  /** The last framing honoured, so a reshape does not repeat it. */
  let framed = 0;

  $effect(() => {
    void drawn;
    // Tracked, so asking to frame something already drawn is enough on its own
    // to move the viewport.
    const wanted = frame;
    const targets = untrack(() => ids);
    if (targets.length === 0) {
      return;
    }

    let timer = 0;

    // A tick late, and again until it takes. Measuring looks each node up in
    // the DOM by id, and on the pass that hands Svelte Flow a new set they are
    // not painted yet — a lookup that finds nothing settles nothing, and there
    // is no second trigger to fall back on.
    //
    // A timer rather than an animation frame, because animation frames do not
    // run in a hidden tab. Opening the tool and looking at something else while
    // it loads used to leave every node unmeasured and therefore every edge
    // undrawn, permanently: nothing else was ever going to ask again.
    //
    // Everything inside is untracked: all of it reads and writes the canvas's
    // own node array, and tracked, settling would be its own trigger.
    const settle = (left: number) => {
      timer = window.setTimeout(() => {
        untrack(() => {
          // Measured *and* showing what was asked for. `nodesInitialized` alone
          // still reads true for the set the canvas is about to replace, and
          // framing against that set frames the wrong thing once and never
          // retries. Framing a subset the canvas has not drawn yet would frame
          // the gaps where it will be.
          const onto = wanted?.ids?.filter((id) => store.nodeLookup.has(id)) ?? null;
          const showing =
            store.nodesInitialized &&
            store.nodeLookup.size === targets.length &&
            (wanted?.ids == null || onto?.length === wanted.ids.length);
          if (showing) {
            if (wanted !== null && wanted.nth > framed) {
              framed = wanted.nth;
              // No `duration`: Svelte Flow resolves an animated fit straight out
              // of a derived, which its own source calls a no-go.
              void store.fitView({ nodes: onto?.map((id) => ({ id })), maxZoom: CLOSE });
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
      }, STEP);
    };

    // A tab that was hidden the whole time never laid anything out to measure,
    // so coming back to it is a reason to start over rather than a moment to
    // discover the canvas gave up while nobody was looking.
    const restart = () => {
      clearTimeout(timer);
      settle(PATIENCE);
    };
    restart();
    document.addEventListener("visibilitychange", restart);

    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", restart);
    };
  });
</script>
