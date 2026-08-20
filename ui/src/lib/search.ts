// Finding a construct by typing part of its name.
//
// A five-module spec set is three hundred constructs across four views, and
// the module checkboxes only narrow it to the file. This is how a reader gets
// to `QueueOnSend` without knowing which module wrote it or which view draws
// it.
//
// Every term has to match something, and a term may match the name, the kind
// or the module — so `rule delivery queue` narrows the way a person would say
// it out loud. Ranking is by how well the *name* matched, because that is what
// the reader was typing: an exact name first, then names that start with it,
// then names that merely contain it, then everything that matched only on its
// kind or module.

import type { Node } from "./api/Node";

/** How many results the rail shows before it stops listing them. */
export const SHOWN = 12;

/** A node that matched, and how well. */
export interface Match {
  node: Node;
  /** Lower is better. Only meaningful within one result set. */
  rank: number;
}

/** What a single term can match. */
function fields(node: Node): string[] {
  return [node.name.toLowerCase(), node.kind, node.module.toLowerCase()];
}

/** How well `term` matched `node`'s name: 0 exact, 1 prefix, 2 inside, 3 not. */
function nameRank(node: Node, term: string): number {
  const name = node.name.toLowerCase();
  if (name === term) {
    return 0;
  }
  if (name.startsWith(term)) {
    return 1;
  }
  return name.includes(term) ? 2 : 3;
}

/**
 * The constructs matching `query`, best first.
 *
 * An empty query matches nothing rather than everything: a list of every
 * construct is what the canvas is for.
 */
export function search(nodes: Node[], query: string): Match[] {
  const terms = query.toLowerCase().split(/\s+/).filter((term) => term.length > 0);
  if (terms.length === 0) {
    return [];
  }

  const matches: Match[] = [];
  for (const node of nodes) {
    const searchable = fields(node);
    if (!terms.every((term) => searchable.some((field) => field.includes(term)))) {
      continue;
    }
    // Ranked on the best any one term managed against the name. Typing
    // `delivery queue` should put `QueueOnSend` above the module's entities,
    // and which of the two words did the work is not the reader's problem.
    const rank = Math.min(...terms.map((term) => nameRank(node, term)));
    matches.push({ node, rank });
  }

  // Ties break on the qualified name, which is unique — so the same query over
  // the same spec always lists the same order, and a link to a result keeps
  // meaning what it meant.
  matches.sort((a, b) => a.rank - b.rank || a.node.id.localeCompare(b.node.id));
  return matches;
}
