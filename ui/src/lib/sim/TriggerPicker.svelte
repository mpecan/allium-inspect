<script lang="ts">
  // Choosing what happens next.
  //
  // Grouped by the surface that offers it, because that is what a surface *is*:
  // the list of things a particular actor can do. Starting from one means
  // starting where a person would. Anything with no surface is grouped under a
  // heading that says so — you can still fire it, and doing so means starting
  // in the middle of something the spec describes.

  import type { Value } from "../api/Value";
  import type { Fireable } from "./setup";
  import { grouped } from "./setup";
  import { parse } from "./values";

  interface Props {
    triggers: Fireable[];
    /** Instance ids in the world, offered as arguments. */
    instances: string[];
    /** Triggers emitted but not yet fired, which are the loose ends. */
    pending: string[];
    onfire: (trigger: string, module: string, args: Record<string, Value>) => void;
  }

  const { triggers, instances, pending, onfire }: Props = $props();

  let chosen = $state<Fireable | null>(null);
  let bindings = $state<Record<string, string>>({});

  const groups = $derived(grouped(triggers));

  function choose(trigger: Fireable) {
    chosen = trigger;
    bindings = Object.fromEntries(trigger.parameters.map((name) => [name, ""]));
  }

  function fire() {
    if (!chosen) {
      return;
    }
    const args: Record<string, Value> = {};
    for (const [name, text] of Object.entries(bindings)) {
      args[name] = parse(text);
    }
    onfire(chosen.trigger, chosen.module, args);
  }

  /** Whether every parameter has something in it. */
  const ready = $derived(
    chosen !== null && Object.values(bindings).every((text) => text.trim() !== ""),
  );
</script>

<section class="picker" aria-label="Fire a trigger">
  {#if pending.length > 0}
    <div class="pending">
      <p class="eyebrow">Emitted, not yet followed</p>
      <ul>
        {#each pending as trigger (trigger)}
          <li>
            <button
              type="button"
              onclick={() => {
                const match = triggers.find((candidate) => candidate.trigger === trigger);
                if (match) {
                  choose(match);
                }
              }}
            >
              {trigger}
            </button>
          </li>
        {/each}
      </ul>
      <p class="prose hint">
        A rule emitted these and nothing has consumed them yet. Following one is
        how the chain continues.
      </p>
    </div>
  {/if}

  {#each groups as group (group.label)}
    <div class="group">
      <p class="eyebrow">{group.label}</p>
      <ul>
        {#each group.triggers as trigger (trigger.trigger + trigger.module)}
          <li>
            <button
              type="button"
              class:current={chosen?.trigger === trigger.trigger}
              onclick={() => choose(trigger)}
            >
              {trigger.trigger}
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {/each}

  {#if chosen}
    <form
      class="arguments"
      onsubmit={(event) => {
        event.preventDefault();
        fire();
      }}
    >
      <p class="eyebrow">{chosen.trigger}</p>
      {#if chosen.parameters.length === 0}
        <p class="prose hint">This trigger carries nothing.</p>
      {:else}
        {#each chosen.parameters as parameter (parameter)}
          <label>
            <span>{parameter}</span>
            <input
              type="text"
              list="sim-instances"
              placeholder="unknown"
              bind:value={bindings[parameter]}
            />
          </label>
        {/each}
        <datalist id="sim-instances">
          {#each instances as id (id)}<option value={id}></option>{/each}
        </datalist>
      {/if}
      <button type="submit" class="fire" disabled={chosen.parameters.length > 0 && !ready}>
        Fire
      </button>
      {#if chosen.parameters.length > 0 && !ready}
        <p class="prose hint">
          An empty argument is <em>unknown</em>, and a precondition reading it
          comes back undecided. Fill them in, or fire anyway to see that happen.
        </p>
        <button type="button" class="anyway" onclick={fire}>Fire anyway</button>
      {/if}
    </form>
  {/if}
</section>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--gap-3);
    padding: var(--gap-3);
    height: 100%;
    overflow-y: auto;
    background: var(--ground-panel);
    border-right: 1px solid var(--line);
  }

  .group + .group,
  .arguments {
    border-top: 1px solid var(--line);
    padding-top: var(--gap-3);
  }

  ul {
    list-style: none;
    margin: var(--gap-1) 0 0;
    padding: 0;
  }

  li button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 2px var(--gap-2);
    font-size: var(--t-small);
    border-radius: var(--radius);
    border-left: 2px solid transparent;
    color: var(--ink-dim);
  }
  li button:hover {
    background: var(--ground-raised);
    color: var(--ink);
  }
  li button.current {
    border-left-color: var(--behaviour);
    background: var(--ground-raised);
    color: var(--behaviour);
  }

  /* The loose ends. A rule emitting something says another should react, and a
   * run that never follows it stopped halfway through what the spec says. */
  .pending {
    border: 1px solid var(--behaviour-edge);
    border-radius: var(--radius);
    padding: var(--gap-2);
    background: var(--behaviour-fill);
  }
  .pending li button {
    color: var(--behaviour);
  }

  .arguments {
    display: flex;
    flex-direction: column;
    gap: var(--gap-2);
  }

  label {
    display: grid;
    grid-template-columns: minmax(0, 7em) minmax(0, 1fr);
    align-items: center;
    gap: var(--gap-2);
    font-size: var(--t-small);
  }

  input {
    font: inherit;
    font-size: var(--t-small);
    padding: 2px var(--gap-2);
    color: var(--ink);
    background: var(--ground-input);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    min-width: 0;
  }
  input::placeholder {
    color: var(--verdict-unknown);
    opacity: 0.75;
  }

  .fire {
    align-self: flex-start;
    padding: 3px var(--gap-4);
    border: 1px solid var(--behaviour);
    border-radius: var(--radius);
    color: var(--behaviour);
    font-size: var(--t-small);
  }
  .fire:disabled {
    opacity: 0.4;
    border-color: var(--line);
    color: var(--ink-faint);
    cursor: not-allowed;
  }

  .anyway {
    align-self: flex-start;
    font-size: var(--t-micro);
    color: var(--verdict-unknown);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .hint {
    margin: 0;
    font-size: var(--t-micro);
    line-height: 1.5;
    color: var(--ink-faint);
  }
</style>
