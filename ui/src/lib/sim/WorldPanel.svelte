<script lang="ts">
  // The world you are simulating against: what exists, and what time it is.
  //
  // Editable, because a specification's rules are all about entities that
  // already exist and something has to bring the first one into being. The
  // configuration comes seeded from the spec's own defaults, so the only thing
  // a person has to supply is the part that is genuinely theirs to choose.
  //
  // A derived field is marked. The spec computes it and this simulator does
  // not, so leaving one unset is the single most common reason a rule comes
  // back undecided — and saying so here is cheaper than explaining it in the
  // trace afterwards.

  import type { EntityChoice } from "./setup";
  import type { Instance } from "../api/Instance";
  import type { World } from "../api/World";
  import { duration, parse, render } from "./values";

  interface Props {
    world: World;
    entities: EntityChoice[];
    onchange: (world: World) => void;
  }

  const { world, entities, onchange }: Props = $props();

  let creating = $state<string>("");

  const instances = $derived(Object.values(world.entities) as Instance[]);
  // Codepoint order, not the host's locale: the Rust side orders every map
  // this way, and a panel that sorted differently per machine would make two
  // readers of one shared world disagree about a list neither of them chose.
  const configured = $derived(
    Object.entries(world.config).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
  );

  function create(entity: string) {
    const choice = entities.find((candidate) => candidate.entity === entity);
    if (!choice) {
      return;
    }
    const ordinal = world.next_ordinal[entity] ?? 1;
    const id = `${entity}#${ordinal}`;
    onchange({
      ...world,
      entities: {
        ...world.entities,
        [id]: { id, entity, module: choice.module, fields: {} },
      },
      next_ordinal: { ...world.next_ordinal, [entity]: ordinal + 1 },
    });
    creating = "";
  }

  function remove(id: string) {
    const { [id]: _gone, ...rest } = world.entities;
    onchange({ ...world, entities: rest });
  }

  function setField(id: string, field: string, text: string, states: string[]) {
    const instance = world.entities[id];
    if (!instance) {
      return;
    }
    onchange({
      ...world,
      entities: {
        ...world.entities,
        [id]: { ...instance, fields: { ...instance.fields, [field]: parse(text, states) } },
      },
    });
  }

  function fieldsOf(instance: Instance) {
    const choice = entities.find((candidate) => candidate.entity === instance.entity);
    return choice?.fields ?? [];
  }

  function shown(instance: Instance, field: string): string {
    const value = instance.fields[field];
    if (!value || value.kind === "unknown") {
      return "";
    }
    return value.kind === "str" ? value.value : render(value).text;
  }

  function advanceClock(by: number) {
    onchange({ ...world, now: Math.max(0, world.now + by) });
  }
</script>

<aside class="world" aria-label="The simulated world">
  <section class="clock">
    <h3>Now</h3>
    <p class="value">t+{duration(world.now)}</p>
    <div class="advance">
      {#each [["1 hour", 3_600_000], ["1 day", 86_400_000], ["1 week", 604_800_000]] as [label, by] (label)}
        <button type="button" onclick={() => advanceClock(by as number)}>+{label}</button>
      {/each}
      <button
        type="button"
        disabled={world.now === 0}
        onclick={() => onchange({ ...world, now: 0 })}>reset</button
      >
    </div>
    <p class="prose hint">
      The clock only moves when you move it, so a rule that waits for a due date
      is something you step to rather than wait for.
    </p>
  </section>

  <section>
    <h3>Instances</h3>
    {#if instances.length === 0}
      <p class="prose hint">
        Nothing exists yet. Most rules act on entities that are already there, so
        create one to have something to act on.
      </p>
    {/if}

    <ul class="instances">
      {#each instances as instance (instance.id)}
        <li>
          <header>
            <code>{instance.id}</code>
            <span class="eyebrow">{instance.module}</span>
            <button
              type="button"
              class="remove"
              title="Remove {instance.id}"
              onclick={() => remove(instance.id)}>×</button
            >
          </header>
          <dl>
            {#each fieldsOf(instance) as field (field.name)}
              <dt class:derived={field.derived} title={field.derived ? "Derived: the spec computes this and the simulator does not" : field.type_expr}>
                {field.name}{#if field.derived}<span aria-hidden="true">ƒ</span>{/if}
              </dt>
              <dd>
                {#if field.states.length > 0}
                  <select
                    value={shown(instance, field.name)}
                    onchange={(event) =>
                      setField(
                        instance.id,
                        field.name,
                        event.currentTarget.value,
                        field.states,
                      )}
                  >
                    <option value="">unknown</option>
                    {#each field.states as state (state)}
                      <option value={state}>{state}</option>
                    {/each}
                  </select>
                {:else}
                  <input
                    type="text"
                    placeholder="unknown"
                    value={shown(instance, field.name)}
                    onchange={(event) =>
                      setField(
                        instance.id,
                        field.name,
                        event.currentTarget.value,
                        field.states,
                      )}
                  />
                {/if}
              </dd>
            {/each}
          </dl>
        </li>
      {/each}
    </ul>

    <div class="create">
      <select bind:value={creating} aria-label="Entity to create">
        <option value="">create…</option>
        {#each entities as choice (choice.entity)}
          <option value={choice.entity}>{choice.entity}</option>
        {/each}
      </select>
      <button type="button" disabled={creating === ""} onclick={() => create(creating)}>
        add
      </button>
    </div>
  </section>

  {#if configured.length > 0}
    <section>
      <h3>Configuration</h3>
      <p class="prose hint">Seeded from the specification's own defaults.</p>
      <dl class="config">
        {#each configured as [name, value] (name)}
          <dt>{name}</dt>
          <dd class:unknown={render(value).unknown}>{render(value).text}</dd>
        {/each}
      </dl>
    </section>
  {/if}
</aside>

<style>
  .world {
    display: flex;
    flex-direction: column;
    gap: var(--gap-4);
    padding: var(--gap-3);
    height: 100%;
    overflow-y: auto;
    background: var(--ground-panel);
    border-left: 1px solid var(--line);
  }

  h3 {
    margin: 0 0 var(--gap-2);
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    font-weight: 500;
    color: var(--ink-faint);
  }

  section + section {
    border-top: 1px solid var(--line);
    padding-top: var(--gap-3);
  }

  .clock .value {
    margin: 0;
    font-size: var(--t-title);
    color: var(--boundary);
  }

  .advance {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    margin-top: var(--gap-2);
  }
  .advance button {
    padding: 2px var(--gap-2);
    font-size: var(--t-micro);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    color: var(--ink-dim);
  }
  .advance button:hover:not(:disabled) {
    border-color: var(--boundary);
    color: var(--boundary);
  }
  .advance button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .hint {
    margin: var(--gap-2) 0 0;
    font-size: var(--t-micro);
    line-height: 1.5;
    color: var(--ink-faint);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .instances > li {
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: var(--gap-2);
    margin-bottom: var(--gap-2);
    background: var(--ground-raised);
  }

  .instances header {
    display: flex;
    align-items: baseline;
    gap: var(--gap-2);
  }
  .instances header code {
    color: var(--thing);
    font-size: var(--t-small);
  }
  .remove {
    margin-left: auto;
    color: var(--ink-faint);
    line-height: 1;
  }
  .remove:hover {
    color: var(--verdict-false);
  }

  dl {
    display: grid;
    /* Proportional rather than sized to the longest name. A qualified config
     * parameter can be forty characters, and letting it set the column width
     * squeezes the value it exists to show off the edge of the panel. */
    grid-template-columns: minmax(0, 1.7fr) minmax(0, 1fr);
    gap: 2px var(--gap-2);
    margin: var(--gap-2) 0 0;
    font-size: var(--t-micro);
    align-items: center;
  }

  dt {
    color: var(--ink-dim);
    overflow-wrap: anywhere;
  }
  /* A derived field is marked because the spec computes it and this simulator
   * does not. Leaving one unset is the commonest reason a rule comes back
   * undecided, and saying so here is cheaper than explaining it afterwards. */
  dt.derived {
    color: var(--ink-faint);
    font-style: italic;
  }
  dt.derived span {
    margin-left: 2px;
    color: var(--verdict-unknown);
    font-style: normal;
  }

  dd {
    margin: 0;
    min-width: 0;
  }

  input,
  select {
    width: 100%;
    font: inherit;
    font-size: var(--t-micro);
    padding: 1px var(--gap-1);
    color: var(--ink);
    background: var(--ground-input);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  input::placeholder {
    color: var(--verdict-unknown);
    opacity: 0.75;
  }

  .create {
    display: flex;
    gap: var(--gap-2);
    margin-top: var(--gap-2);
  }
  .create select {
    flex: 1;
    font-size: var(--t-small);
  }
  .create button {
    padding: 2px var(--gap-3);
    font-size: var(--t-small);
    border: 1px solid var(--behaviour);
    border-radius: var(--radius);
    color: var(--behaviour);
  }
  .create button:disabled {
    opacity: 0.4;
    border-color: var(--line);
    color: var(--ink-faint);
    cursor: not-allowed;
  }

  .config dd {
    color: var(--ink-dim);
  }
  .config dd.unknown {
    color: var(--verdict-unknown);
  }
</style>
