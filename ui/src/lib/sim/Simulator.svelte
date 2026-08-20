<script lang="ts">
  // The simulator, assembled.
  //
  // Three columns and a strip: what you can do, what happened, what exists, and
  // the run so far. The middle column is the one that answers "why", so it gets
  // the space.
  //
  // The whole run lives here as a value. The server holds no session — it is
  // handed a world and an event and answers with the world that resulted — so
  // going back a step is free and nothing has to be undone anywhere else.

  import type { Value } from "../api/Value";
  import type { World } from "../api/World";
  import { ApiError, InspectClient } from "../client";
  import {
    advance,
    back,
    canGoBack,
    canGoForward,
    current,
    forward,
    goTo,
    pendingTriggers,
    replaceWorld,
    start,
    stepCount,
    type History,
  } from "./history";
  import { offered, type Setup } from "./setup";
  import Timeline from "./Timeline.svelte";
  import Trace from "./Trace.svelte";
  import TriggerPicker from "./TriggerPicker.svelte";
  import WorldPanel from "./WorldPanel.svelte";

  interface Props {
    client: InspectClient;
    /** Modules the reader has switched off, which are not offered here either. */
    hidden: ReadonlySet<string>;
  }

  const { client, hidden }: Props = $props();

  let setup = $state.raw<Setup | null>(null);
  let history = $state.raw<History | null>(null);
  let failure = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    client
      .setup()
      .then((loaded) => {
        setup = loaded;
        history = start(loaded.world, "seeded");
        failure = null;
      })
      .catch((error: unknown) => {
        failure =
          error instanceof ApiError ? error.message : "Could not set up a simulation.";
      });
  });

  const frame = $derived(history ? current(history) : null);
  const instances = $derived(Object.keys(frame?.world.entities ?? {}));
  const pending = $derived(history ? pendingTriggers(history) : []);

  async function fire(trigger: string, module: string, args: Record<string, Value>) {
    if (!history) {
      return;
    }
    busy = true;
    try {
      const outcome = await client.step(current(history).world, {
        trigger,
        module,
        arguments: args,
      });
      history = advance(history, outcome);
      failure = null;
    } catch (error: unknown) {
      failure = error instanceof ApiError ? error.message : "The step failed.";
    } finally {
      busy = false;
    }
  }

  /** Fire a trigger with no arguments — used by the "follow this" links. */
  function follow(trigger: string) {
    const known = setup?.triggers.find((candidate) => candidate.trigger === trigger);
    void fire(trigger, known?.module ?? "", {});
  }

  function editWorld(world: World) {
    if (history) {
      history = replaceWorld(history, world);
    }
  }
</script>

<div class="simulator">
  {#if failure}
    <div class="failure">
      <p class="eyebrow">Not connected</p>
      <p class="prose">{failure}</p>
    </div>
  {:else if !setup || !history || !frame}
    <div class="failure">
      <p class="eyebrow">Setting up</p>
      <p class="prose">Reading the specification's configuration defaults.</p>
    </div>
  {:else}
    <TriggerPicker
      triggers={offered(setup.triggers, hidden)}
      {instances}
      {pending}
      onfire={(trigger, module, args) => void fire(trigger, module, args)}
    />

    <main class:busy>
      <Timeline
        frames={history.frames}
        at={history.at}
        ongo={(index) => (history = goTo(history!, index))}
      />

      <div class="controls">
        <button
          type="button"
          disabled={!canGoBack(history)}
          onclick={() => (history = back(history!))}>← back</button
        >
        <button
          type="button"
          disabled={!canGoForward(history)}
          onclick={() => (history = forward(history!))}>forward →</button
        >
        <span class="address">
          {stepCount(history)} step{stepCount(history) === 1 ? "" : "s"}
        </span>
        <button
          type="button"
          class="restart"
          onclick={() => (history = start(setup!.world, "seeded"))}>restart</button
        >
      </div>

      <Trace outcome={frame.outcome} onfire={follow} />
    </main>

    <WorldPanel world={frame.world} entities={setup.entities} onchange={editWorld} />
  {/if}
</div>

<style>
  .simulator {
    display: grid;
    grid-template-columns: 14rem minmax(0, 1fr) 20rem;
    height: 100%;
    min-height: 0;
  }

  main {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    transition: opacity 120ms ease;
  }
  main.busy {
    opacity: 0.6;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: var(--gap-2);
    padding: var(--gap-2) var(--gap-3);
    border-bottom: 1px solid var(--line);
    flex: none;
  }

  .controls button {
    font-size: var(--t-micro);
    padding: 2px var(--gap-2);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    color: var(--ink-dim);
  }
  .controls button:hover:not(:disabled) {
    border-color: var(--line-strong);
    color: var(--ink);
  }
  .controls button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .controls .restart {
    margin-left: auto;
  }

  .failure {
    grid-column: 1 / -1;
    display: grid;
    place-content: center;
    text-align: center;
    gap: var(--gap-2);
    padding: var(--gap-6);
  }
  .failure .prose {
    max-width: 44ch;
  }

  @media (max-width: 1200px) {
    .simulator {
      grid-template-columns: 13rem minmax(0, 1fr);
    }
    .simulator :global(.world) {
      display: none;
    }
  }
</style>
