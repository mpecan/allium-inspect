<script lang="ts">
  // Who was in the journey, and what they held.
  //
  // A journey names instances rather than roles — two members with different
  // preconditions is the ordinary case — so this is a list of people and things
  // rather than a list of types, and each one opens to show what the world
  // actually had in it.
  //
  // Bound to a step rather than to the end. "What is Ada's loan now" is a
  // question a final state answers while hiding the step that made it so, and
  // when a value changed is most of what a journey is written to show.

  import type { CastMember } from "../api/CastMember";
  import type { Walk } from "../api/Walk";
  import { render } from "../sim/values";
  import {
    ORIGIN,
    ORIGIN_MEANING,
    appearsAt,
    configOf,
    fieldsOf,
    instanceOf,
    stateAt,
  } from "./cast";

  interface Props {
    walk: Walk;
    /** Which step's world to read. */
    at: number;
  }

  const { walk, at }: Props = $props();

  const world = $derived(stateAt(walk, at));
  const config = $derived(configOf(world));

  let opened = $state<string[]>([]);

  function toggle(name: string) {
    opened = opened.includes(name)
      ? opened.filter((each) => each !== name)
      : [...opened, name];
  }

  /** Whether this member exists yet, at the step being shown. */
  function present(member: CastMember): boolean {
    return instanceOf(world, member) !== null;
  }

  function notYet(member: CastMember): string {
    const step = appearsAt(walk, member);
    return step === -1
      ? "never created"
      : `not until step ${walk.steps[step]?.number ?? step + 1}`;
  }
</script>

<aside class="cast" aria-label="Who was in this journey">
  <h2>Cast</h2>
  {#if walk.cast.length === 0}
    <p class="prose empty">This journey names nobody.</p>
  {:else}
    <ul class="members">
      {#each walk.cast as member (member.name)}
        {@const instance = instanceOf(world, member)}
        {@const isOpen = opened.includes(member.name)}
        <li>
          <button
            type="button"
            aria-expanded={isOpen}
            onclick={() => toggle(member.name)}
            class:absent={!present(member)}
          >
            <span class="who">
              <span class="name">{member.name}</span>
              <span class="type">{member.type_expr}</span>
            </span>
            <span class="origin" title={ORIGIN_MEANING[member.origin]}>
              {ORIGIN[member.origin]}
            </span>
          </button>

          {#if isOpen}
            {#if instance}
              <dl class="fields">
                {#each fieldsOf(instance) as [field, value] (field)}
                  {@const shown = render(value)}
                  <dt>{field}</dt>
                  <dd class:unknown={shown.unknown}>{shown.text}</dd>
                {/each}
                {#if fieldsOf(instance).length === 0}
                  <!-- Not the same as "no such thing". The instance exists and
                       nothing has written to it yet, which is a fact about the
                       journey rather than about the spec. -->
                  <p class="prose note">Exists, with nothing set on it.</p>
                {/if}
              </dl>
              <p class="address">{instance.id} · {instance.module}</p>
            {:else}
              <p class="prose note">
                {#if member.entity === null}
                  <!-- The step that was meant to create it did not, and every
                       later line that names it is reading against nothing. -->
                  Nothing was created for this name. Every line below that
                  mentions it is reading against nothing.
                {:else}
                  Not in the world at this step — {notYet(member)}.
                {/if}
              </p>
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if config.length > 0}
    <h2 class="second">Configuration</h2>
    <p class="prose note">
      Seeded from the spec's own defaults, and in force for every step.
    </p>
    <dl class="config">
      {#each config as parameter (parameter.module + parameter.name)}
        {@const shown = render(parameter.value)}
        <dt>
          {#if parameter.module}<span class="module">{parameter.module}.</span>{/if}{parameter.name}
        </dt>
        <dd class:unknown={shown.unknown}>{shown.text}</dd>
      {/each}
    </dl>
  {/if}
</aside>

<style>
  .cast {
    border-left: 1px solid var(--line);
    background: var(--ground-panel);
    padding: var(--gap-4) var(--gap-3);
    overflow-y: auto;
  }

  h2 {
    margin: 0 0 var(--gap-2);
    font-size: var(--t-micro);
    font-weight: 600;
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--ink-faint);
  }

  .second {
    margin-top: var(--gap-6);
    padding-top: var(--gap-3);
    border-top: 1px solid var(--line);
  }

  .members {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .members button {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--gap-2);
    width: 100%;
    padding: var(--gap-2) var(--gap-1);
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .members button:hover {
    background: var(--ground-raised);
  }

  /* Something the journey names that is not there at this step. Dimmed rather
   * than hidden: a name that vanishes from the list is a name the reader
   * cannot look up when a line below mentions it. */
  .members button.absent .name,
  .members button.absent .type {
    color: var(--ink-faint);
  }

  .who {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .name {
    font-family: var(--font-mono);
    font-size: var(--t-small);
  }

  .type {
    font-size: var(--t-micro);
    color: var(--ink-faint);
  }

  .origin {
    flex: none;
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--ink-faint);
  }

  dl {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1px var(--gap-2);
    margin: var(--gap-1) 0 var(--gap-2);
    padding: var(--gap-2);
    border-radius: var(--radius);
    background: var(--ground-input);
    font-size: var(--t-small);
  }

  dt {
    font-family: var(--font-mono);
    color: var(--ink-dim);
    overflow-wrap: anywhere;
  }

  dd {
    margin: 0;
    font-family: var(--font-mono);
    color: var(--ink);
    text-align: right;
    overflow-wrap: anywhere;
  }

  /* The one value that is not a fact. Everywhere else in this tool an undecided
   * value is the only thing with a fill, and a panel of fields is exactly where
   * a blank would be read as a zero. */
  dd.unknown {
    color: var(--verdict-unknown);
  }

  .config dt .module {
    color: var(--ink-faint);
  }

  .note,
  .empty {
    margin: 0 0 var(--gap-2);
    font-size: var(--t-small);
    color: var(--ink-faint);
  }

  dl .note {
    grid-column: 1 / -1;
  }

  .address {
    margin: 0 0 var(--gap-3);
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    color: var(--ink-faint);
    text-align: right;
  }
</style>
