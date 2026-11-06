// How much the canvas will draw before it asks first.
//
// Laying out is off the page's thread (see `engine.ts`); drawing cannot be.
// Svelte Flow puts every node and every edge into the DOM, and it does it in
// one go, so the cost of a view is paid as a single stretch in which the page
// does not respond. Measured on generated spec sets, it is close to linear —
// about 0.2 ms per element on a fast machine, so a 200-file set's flow view,
// 4,000 constructs and 8,000 relationships, froze the tab for three seconds
// after layout had already finished.
//
// So past a size the canvas does not start drawing on its own. It says how
// big the view is and offers the ways to make it smaller, and drawing it
// anyway is one click — the reader is told what it will cost rather than
// finding out. What it never does is draw *part* of a view without saying so:
// a canvas that silently dropped constructs to stay fast would be stating that
// a spec has less in it than it does.

/**
 * The most elements a view may hold before the canvas asks.
 *
 * Nodes and edges counted alike, because each is a DOM subtree of comparable
 * cost. At the measured rate this is somewhere under a second of a stalled tab
 * on the machine it was measured on; slower machines pay more, which is why it
 * is not higher.
 */
export const DRAW_BUDGET = 4000;

/**
 * Whether a view of this weight is drawn without asking.
 *
 * `allowed` is the heaviest view of this kind the reader already said to draw
 * anyway. A consent covers anything up to the size it was given for, so
 * switching a module back on after drawing a larger set does not ask again;
 * a view heavier than any the reader agreed to does.
 */
export function affordable(heft: number, allowed = 0): boolean {
  return heft <= Math.max(DRAW_BUDGET, allowed);
}

/** The words the canvas uses for a view it has not drawn. */
export function overBudget(
  nodes: number,
  edges: number,
  modules: number,
): { headline: string; detail: string } {
  const files = modules === 1 ? "one file" : `${count(modules)} files`;
  return {
    headline: `${count(nodes)} constructs and ${count(edges)} relationships from ${files}.`,
    detail:
      "Drawing that many at once stalls the browser for a few seconds, during which nothing on the page responds. " +
      "Turn some modules off in the rail, or start from the modules view and pick the files worth reading.",
  };
}

/** A number as it reads in a sentence: grouped by thousands, deterministically. */
function count(n: number): string {
  return String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
