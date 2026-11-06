// Where ELK runs, and what happens to a layout nobody is waiting for any more.
//
// ELK is a Java layout engine compiled to JavaScript, and on a spec set of a
// couple of hundred files it computes for five or six seconds at a stretch.
// Run on the page's own thread, that is five or six seconds in which nothing
// else happens: the rail does not respond, the view button just clicked does
// not even depress, and the whole tab reads as hung — because, for that long,
// it is. So it runs in a worker, and the page stays a page while it thinks.
//
// The second half of the job is the layout that went stale. Clicking Flow and
// then Lifecycle before Flow has settled used to queue the two: the reader
// waited out a picture they had already walked away from before getting the
// one they asked for. A run inside ELK cannot be interrupted, but the worker
// it runs in can be thrown away — so a new request terminates the old worker,
// the superseded promise rejects with `Superseded`, and the new run starts on
// a fresh one immediately.
//
// The main-thread engine survives as the fallback, and only as that. A worker
// that cannot start — blocked by a policy, or a file the server did not send —
// would otherwise leave the canvas placing forever, which is how a lock-up
// looks from the outside even though nothing is running at all.

import type { ElkGraph, ElkLike } from "./layout";

/** One ELK instance: a worker, or the bundled engine on this thread. */
export interface Instance {
  layout(graph: ElkGraph): Promise<ElkGraph>;
  /** Stop it for good. Absent on an engine that has nothing to stop. */
  terminateWorker?(): void;
}

/** How to get hold of an engine of each kind. */
export interface Spawners {
  /**
   * A fresh worker-backed engine. `failed` is called if the worker itself
   * dies — not if a layout is rejected, which is an answer, but if there is
   * nothing left to give one.
   */
  worker: ((failed: (reason: unknown) => void) => Instance) | null;
  /** The engine on this thread, loaded only once it is needed. */
  local: () => Promise<Instance>;
}

/** The rejection a layout gets when a newer one replaced it. */
export class Superseded extends Error {
  constructor() {
    super("superseded by a newer layout");
    this.name = "Superseded";
  }
}

/** An ELK that is also something to shut down. */
export interface Engine extends ElkLike {
  dispose(): void;
}

/**
 * An ELK that runs one layout at a time, and drops the old one for a new one.
 *
 * Ordered: a result is only ever delivered for the latest request. Every
 * earlier request that had not finished rejects with `Superseded`, so a caller
 * can tell "someone asked for something else" apart from "ELK could not do it"
 * — the second gets a grid, the first gets nothing at all.
 */
export function engine(spawn: Spawners): Engine {
  let current: Instance | null = null;
  /** Rejects the request in flight, if there is one. */
  let abandon: ((reason: unknown) => void) | null = null;
  /** Set once the worker has failed, after which this thread does the work. */
  let local: Promise<Instance> | null = spawn.worker === null ? spawn.local() : null;
  let disposed = false;

  function stop() {
    current?.terminateWorker?.();
    current = null;
  }

  function instance(): Promise<Instance> {
    if (local) {
      return local;
    }
    if (current) {
      return Promise.resolve(current);
    }
    const spawned = spawn.worker?.((reason) => {
      // Whatever was in flight on it is not coming back, and nor is anything
      // sent to a worker that cannot start. Hand both to this thread.
      stop();
      local = spawn.local();
      abandon?.(reason);
    });
    current = spawned ?? null;
    return spawned ? Promise.resolve(spawned) : (local = spawn.local());
  }

  return {
    async layout(graph) {
      if (disposed) {
        throw new Superseded();
      }
      if (abandon) {
        // A worker is mid-layout on a picture nobody wants any more. Killing it
        // is the only way to make it stop.
        abandon(new Superseded());
        stop();
      }
      return await new Promise<ElkGraph>((resolve, reject) => {
        let settled = false;
        const finish = (outcome: () => void) => {
          if (settled) {
            return;
          }
          settled = true;
          if (abandon === fail) {
            abandon = null;
          }
          outcome();
        };
        const fail = (reason: unknown) => {
          finish(() => {
            // A worker that died rather than a layout that was replaced. The
            // request is still wanted, so it goes again on the local engine.
            if (reason instanceof Superseded) {
              reject(reason);
              return;
            }
            void instance()
              .then((engine) => engine.layout(graph))
              .then(resolve, reject);
          });
        };
        abandon = fail;
        void instance()
          .then((engine) => engine.layout(graph))
          .then(
            (result) => finish(() => resolve(result)),
            (error: unknown) => finish(() => reject(error)),
          );
      });
    },
    dispose() {
      disposed = true;
      abandon?.(new Superseded());
      stop();
    },
  };
}
