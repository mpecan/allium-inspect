import { describe, expect, it, vi } from "vitest";

import { engine, Superseded, type Instance } from "./engine";
import type { ElkGraph } from "./layout";

/** A layout the test settles by hand, so ordering can be driven exactly. */
interface Pending {
  graph: ElkGraph;
  resolve: (graph: ElkGraph) => void;
  reject: (reason: unknown) => void;
}

/** A fake worker engine that records every request and every termination. */
function fakeWorker() {
  const pending: Pending[] = [];
  const terminated: number[] = [];
  const failures: ((reason: unknown) => void)[] = [];
  let spawned = 0;
  const spawn = (failed: (reason: unknown) => void): Instance => {
    const which = spawned++;
    failures.push(failed);
    return {
      layout: (graph) =>
        new Promise<ElkGraph>((resolve, reject) => {
          pending.push({ graph, resolve, reject });
        }),
      terminateWorker: () => terminated.push(which),
    };
  };
  return {
    spawn,
    pending,
    terminated,
    failures,
    spawned: () => spawned,
  };
}

/** A main-thread engine that answers at once, marking what it laid out. */
function localEngine() {
  const calls: string[] = [];
  const local = vi.fn(async (): Promise<Instance> => ({
    layout: async (graph) => {
      calls.push(graph.id);
      return { ...graph, id: `local:${graph.id}` };
    },
  }));
  return { local, calls };
}

const graph = (id: string): ElkGraph => ({ id, children: [], edges: [] });

/** Let queued promise callbacks run. */
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("the layout engine", () => {
  it("delivers a layout and reuses the worker for the next one", async () => {
    const worker = fakeWorker();
    const { local } = localEngine();
    const elk = engine({ worker: worker.spawn, local });

    const first = elk.layout(graph("a"));
    await tick();
    worker.pending[0]?.resolve({ ...graph("a"), id: "done:a" });
    await expect(first).resolves.toMatchObject({ id: "done:a" });

    const second = elk.layout(graph("b"));
    await tick();
    worker.pending[1]?.resolve(graph("b"));
    await expect(second).resolves.toMatchObject({ id: "b" });

    // One worker for both: nothing was in flight when the second arrived.
    expect(worker.spawned()).toBe(1);
    expect(worker.terminated).toEqual([]);
    expect(local).not.toHaveBeenCalled();
  });

  // The reason the engine exists beyond being off-thread: a layout nobody is
  // waiting for must not hold up the one they are.
  it("abandons a layout still running when a newer one is asked for", async () => {
    const worker = fakeWorker();
    const elk = engine({ worker: worker.spawn, local: localEngine().local });

    const stale = elk.layout(graph("flow"));
    await tick();
    const fresh = elk.layout(graph("lifecycle"));

    await expect(stale).rejects.toBeInstanceOf(Superseded);
    // The old worker is killed, and the new request goes to a new one.
    expect(worker.terminated).toEqual([0]);
    await tick();
    expect(worker.spawned()).toBe(2);
    expect(worker.pending.map((p) => p.graph.id)).toEqual(["flow", "lifecycle"]);

    // Even if the dead worker's answer were to arrive, it is not delivered.
    worker.pending[0]?.resolve(graph("flow"));
    worker.pending[1]?.resolve(graph("lifecycle"));
    await expect(fresh).resolves.toMatchObject({ id: "lifecycle" });
  });

  // A dead worker's answer can still arrive. It must not be mistaken for the
  // current request finishing, or that request could no longer be replaced.
  it("ignores a late answer from a replaced layout", async () => {
    const worker = fakeWorker();
    const elk = engine({ worker: worker.spawn, local: localEngine().local });

    const first = elk.layout(graph("a"));
    await tick();
    const second = elk.layout(graph("b"));
    await expect(first).rejects.toBeInstanceOf(Superseded);
    await tick();
    worker.pending[0]?.resolve(graph("a"));
    await tick();

    const third = elk.layout(graph("c"));
    await expect(second).rejects.toBeInstanceOf(Superseded);
    expect(worker.terminated).toEqual([0, 1]);
    await tick();
    worker.pending[2]?.resolve(graph("c"));
    await expect(third).resolves.toMatchObject({ id: "c" });
  });

  it("passes ELK's own rejection through, without killing the worker", async () => {
    const worker = fakeWorker();
    const elk = engine({ worker: worker.spawn, local: localEngine().local });

    const run = elk.layout(graph("a"));
    await tick();
    worker.pending[0]?.reject(new Error("ELK said no"));
    await expect(run).rejects.toThrow("ELK said no");
    expect(worker.terminated).toEqual([]);

    // And the next one still runs on the same worker.
    const next = elk.layout(graph("b"));
    await tick();
    worker.pending[1]?.resolve(graph("b"));
    await expect(next).resolves.toMatchObject({ id: "b" });
    expect(worker.spawned()).toBe(1);
  });

  // A worker that cannot start would otherwise leave the canvas placing for
  // good — a lock-up from the outside, with nothing running at all.
  it("finishes on this thread when the worker dies mid-layout", async () => {
    const worker = fakeWorker();
    const { local, calls } = localEngine();
    const elk = engine({ worker: worker.spawn, local });

    const run = elk.layout(graph("a"));
    await tick();
    worker.failures[0]?.(new Error("worker script not found"));

    await expect(run).resolves.toMatchObject({ id: "local:a" });
    expect(calls).toEqual(["a"]);
    expect(worker.terminated).toEqual([0]);

    // From then on, this thread does the work; no second worker is tried.
    await expect(elk.layout(graph("b"))).resolves.toMatchObject({ id: "local:b" });
    expect(worker.spawned()).toBe(1);
    expect(local).toHaveBeenCalledTimes(1);
  });

  it("uses this thread from the start where there are no workers", async () => {
    const { local, calls } = localEngine();
    const elk = engine({ worker: null, local });
    await expect(elk.layout(graph("a"))).resolves.toMatchObject({ id: "local:a" });
    expect(calls).toEqual(["a"]);
  });

  it("stops the worker and refuses further layouts once disposed", async () => {
    const worker = fakeWorker();
    const elk = engine({ worker: worker.spawn, local: localEngine().local });

    const run = elk.layout(graph("a"));
    await tick();
    elk.dispose();

    await expect(run).rejects.toBeInstanceOf(Superseded);
    expect(worker.terminated).toEqual([0]);
    await expect(elk.layout(graph("b"))).rejects.toBeInstanceOf(Superseded);
    expect(worker.spawned()).toBe(1);
  });
});
