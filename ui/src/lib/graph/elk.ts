// The browser's ELK: a worker when there can be one, this thread when not.
//
// Kept apart from `engine.ts` so that what is tested there — ordering,
// supersession, the fall back on a dead worker — is tested without a browser,
// and this file is only the wiring: which script the worker runs, and where the
// bundled engine is loaded from when it has to be.

import ELK from "elkjs/lib/elk-api.js";
// The worker script as a file of its own rather than inlined, so the page does
// not parse a megabyte and a half of layout engine it is about to hand away.
import workerUrl from "elkjs/lib/elk-worker.min.js?url";

import { engine, type Engine, type Instance } from "./engine";

/** An ELK for the canvas: one per canvas, disposed with it. */
export function browserEngine(): Engine {
  return engine({
    worker:
      typeof Worker === "undefined"
        ? null
        : (failed) =>
            // elkjs's types are looser than `Instance` asks: `layout` there is
            // generic over the node shape. The shape it returns is ours.
            new ELK({
              workerFactory: () => {
                const worker = new Worker(workerUrl);
                worker.addEventListener("error", (event) => {
                  // Reported here, once, rather than by the browser as an
                  // uncaught error, because it is handled: the local engine
                  // takes over and the canvas still draws.
                  event.preventDefault();
                  console.warn("layout worker failed; laying out on the page instead", event.message);
                  failed(event);
                });
                return worker;
              },
            }) as unknown as Instance,
    local: async () => {
      const { default: Bundled } = await import("elkjs/lib/elk.bundled.js");
      return new Bundled() as unknown as Instance;
    },
  });
}
