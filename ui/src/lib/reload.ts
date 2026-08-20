// Noticing that the spec changed under the tool.
//
// The browser fetches the graph once. A watcher re-reads the spec set whenever
// a file is saved and swaps the server's copy, and until this existed it told
// nobody: the reader went on studying a picture of a file that no longer said
// that, with nothing on screen to distinguish the two. It was worse when the
// edit broke the spec — the server kept serving the last good graph, correctly,
// and said so only to a health endpoint nothing asked.
//
// So the client asks. `/api/health` carries a revision that moves whenever the
// answer changes, and the state of the specification in the same response, so
// one request settles both questions.
//
// A poll rather than a stream, deliberately. One request a second to a loopback
// socket costs nothing; there is no connection to lose, reconnect, or leave
// half-open behind a laptop lid; and a stream would still have needed the health
// payload alongside it.

import type { Health } from "./api/Health";

/** How the spec set stands, as the interface needs to say it. */
export interface Standing {
  /** The revision this describes. */
  revision: number;
  /** Whether the graph on screen should be replaced. */
  stale: boolean;
  /** What to tell the reader, or null when there is nothing to tell. */
  trouble: Trouble | null;
}

export interface Trouble {
  /** One line, in the reader's terms. */
  headline: string;
  /** The detail beneath it, when there is any. */
  detail: string | null;
  /** Whether the graph on screen is from before the trouble started. */
  showingOlder: boolean;
}

/**
 * What changed between the revision we are showing and the one just reported.
 *
 * `known` is the revision the current graph came from, or null before the first
 * poll — the first answer is never stale, because there is nothing older on
 * screen to replace.
 */
export function standing(known: number | null, health: Health): Standing {
  return {
    revision: health.revision,
    stale: known !== null && health.revision !== known,
    trouble: troubleWith(health),
  };
}

/**
 * What is wrong, phrased for someone reading rather than debugging.
 *
 * The two failures are different and are told apart, because the reader's next
 * move differs. A reload that failed means the graph on screen is from before
 * the edit and is not to be trusted. A spec that carries errors means the graph
 * *is* current — Allium still describes a file it could not fully parse — and
 * what is on screen is an honest picture of a spec with mistakes in it.
 */
function troubleWith(health: Health): Trouble | null {
  if (health.error !== null) {
    return {
      headline: "The last read of the spec failed.",
      detail: health.error,
      showingOlder: true,
    };
  }
  if (health.errors > 0) {
    const errors = `${health.errors} error${health.errors === 1 ? "" : "s"}`;
    return {
      headline: `The spec has ${errors}.`,
      detail: "This is what allium could still read of it.",
      showingOlder: false,
    };
  }
  return null;
}
