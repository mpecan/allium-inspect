<script lang="ts">
  // What one step did, read top to bottom the way the rule is written.
  //
  // The panel's job is to answer "why?" — why did this fire, why did it not,
  // what changed, and what could the simulator not decide. Every clause gets a
  // line and a verdict mark, and the undecided ones are the only entries that
  // carry a fill, because they are the only ones that need a person.

  import type { RuleOutcome } from "../api/RuleOutcome";
  import type { StepOutcome } from "../api/StepOutcome";
  import Verdict from "../panels/Verdict.svelte";
  import { render } from "./values";

  interface Props {
    outcome: StepOutcome | null;
    onfire: (trigger: string) => void;
  }

  const { outcome, onfire }: Props = $props();

  const DISPOSITION: Record<RuleOutcome["disposition"], string> = {
    fired: "fired",
    refused: "did not fire",
    undecided: "could not be decided",
    unsimulatable: "cannot be simulated",
  };

  const broken = $derived(
    outcome?.invariants.filter((i) => i.truth === "false" && !i.already_broken) ?? [],
  );
  const uncheckable = $derived(
    outcome?.invariants.filter((i) => i.truth === "unknown") ?? [],
  );
</script>

<section class="trace" aria-label="What the step did">
  {#if !outcome}
    <p class="prose empty">
      Fire a trigger to see which rules wait for it, whether their preconditions
      hold, and what changes.
    </p>
  {:else if outcome.rules.length === 0}
    <div class="nothing">
      <p class="eyebrow">Nothing waits for {outcome.event.trigger}</p>
      <p class="prose">
        No rule in this specification has <code>{outcome.event.trigger}</code> as
        its trigger. A trigger that is emitted and never consumed is a real thing
        for a spec to contain — and worth knowing about.
      </p>
    </div>
  {:else}
    {#each outcome.rules as rule, index (index)}
      <article class="rule {rule.disposition}">
        <header>
          <span class="eyebrow">{rule.module}</span>
          <h3>{rule.name}</h3>
          <span class="disposition">{DISPOSITION[rule.disposition]}</span>
        </header>

        {#if rule.requires.length > 0}
          <ul class="clauses">
            {#each rule.requires as clause, position (position)}
              <li>
                <Verdict kind={clause.truth} />
                <code>{clause.text}</code>
                {#if clause.unresolved.length > 0}
                  <ul class="why">
                    {#each clause.unresolved as note, at (at)}
                      <li class="prose">
                        <!-- The separator carries its own spaces. Written as
                             `{note.reason}{#if …}` with the dash inside the
                             block, Svelte trims the block's leading whitespace
                             and the line reads "not simulated— Membership{…}". -->
                        {note.reason}{#if note.expression}{" — "}<code
                            >{note.expression}</code
                          >{/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="prose none">
            No preconditions: this rule runs whenever its trigger happens.
          </p>
        {/if}

        {#if rule.effects.length > 0}
          <ul class="effects">
            {#each rule.effects as effect, position (position)}
              <li class={effect.kind}>
                {#if effect.kind === "created"}
                  <span class="verb">created</span>
                  <code>{effect.id}</code>
                {:else if effect.kind === "assigned"}
                  <span class="verb">set</span>
                  <code>{effect.id}.{effect.field}</code>
                  <span class="from">{render(effect.from).text}</span>
                  <span class="arrow" aria-hidden="true">→</span>
                  <span class="to">{render(effect.to).text}</span>
                {:else if effect.kind === "emitted"}
                  <span class="verb">emitted</span>
                  <button type="button" onclick={() => onfire(effect.trigger)}>
                    {effect.trigger}
                  </button>
                {:else if effect.kind === "refused"}
                  <span class="verb refused">refused</span>
                  <code>{effect.id}.{effect.field}</code>
                  <span class="from">{effect.from}</span>
                  <span class="arrow" aria-hidden="true">→</span>
                  <span class="to">{effect.to}</span>
                  <span class="prose reason">{effect.reason}</span>
                {:else}
                  <span class="verb">noted</span>
                  <span class="prose">{effect.description}</span>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </article>
    {/each}

    {#if broken.length > 0}
      <article class="invariants broken">
        <h3>Broke {broken.length} invariant{broken.length === 1 ? "" : "s"}</h3>
        <ul>
          {#each broken as invariant, index (index)}
            <li><Verdict kind="false" /> {invariant.name}</li>
          {/each}
        </ul>
      </article>
    {/if}

    {#if uncheckable.length > 0}
      <article class="invariants">
        <h3>
          {uncheckable.length} invariant{uncheckable.length === 1 ? "" : "s"} could
          not be checked
        </h3>
        <ul>
          {#each uncheckable as invariant, index (index)}
            <li>
              <Verdict kind="unknown" />
              {invariant.name}
              {#each invariant.unresolved as note, at (note.reason + at)}
                <!-- Every reason, not the first. An invariant can be undecided
                     for two independent reasons, and fixing the one shown only
                     to be told about the next is the same round trip twice. -->
                <span class="reason">{note.reason}</span>
              {/each}
            </li>
          {/each}
        </ul>
      </article>
    {/if}

    {#if outcome.newly_enabled.length > 0}
      <article class="enabled">
        <h3>Now possible</h3>
        <p class="prose">
          These rules watch the world rather than waiting for a person. Each one
          holds now and did not before.
        </p>
        <ul>
          {#each outcome.newly_enabled as rule, index (index)}
            <li>
              <span class="eyebrow">{rule.source}</span>
              <strong>{rule.name}</strong>
              {#if rule.over.length > 0}
                <span class="over">
                  over {rule.over.map((value) => render(value).text).join(", ")}
                </span>
              {/if}
              {#each rule.undecided as note, at (note.reason + at)}
                <!-- Instances the condition could not settle. Without these a
                     rule that holds for nobody and a rule nothing could decide
                     look identical here — and this list is what a person picks
                     their next step from, so the difference is the whole
                     question. -->
                <span class="undecided">{note.reason}</span>
              {/each}
            </li>
          {/each}
        </ul>
      </article>
    {/if}
  {/if}
</section>

<style>
  .trace {
    display: flex;
    flex-direction: column;
    gap: var(--gap-3);
    padding: var(--gap-3);
    overflow-y: auto;
    min-height: 0;
  }

  .empty,
  .nothing {
    margin: var(--gap-6) auto;
    max-width: 42ch;
    text-align: center;
  }

  .rule,
  .invariants,
  .enabled {
    border: 1px solid var(--line);
    border-left-width: 2px;
    border-radius: var(--radius);
    padding: var(--gap-2) var(--gap-3) var(--gap-3);
    background: var(--ground-panel);
  }

  .rule.fired {
    border-left-color: var(--verdict-true);
  }
  .rule.refused {
    border-left-color: var(--verdict-false);
  }
  /* The one that gets a fill. A rule the simulator could not decide is the
   * entry a person has to look at, so it is the loudest thing on the panel. */
  .rule.undecided,
  .rule.unsimulatable {
    border-left-color: var(--verdict-unknown);
    background: color-mix(in srgb, var(--verdict-unknown-fill) 60%, var(--ground-panel));
  }

  header {
    display: flex;
    align-items: baseline;
    gap: var(--gap-2);
    flex-wrap: wrap;
  }

  h3 {
    margin: 0;
    font-size: var(--t-title);
    font-weight: 500;
    letter-spacing: var(--track-tight);
  }

  .disposition {
    margin-left: auto;
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--ink-faint);
  }

  ul {
    list-style: none;
    margin: var(--gap-2) 0 0;
    padding: 0;
    font-size: var(--t-small);
  }

  .clauses > li,
  .effects > li,
  .invariants li,
  .enabled li {
    display: flex;
    align-items: baseline;
    gap: var(--gap-2);
    flex-wrap: wrap;
    padding: 2px 0;
  }

  /* A reason breaks to its own line rather than running on after the thing it
     is about. These rows are flex, so `display: block` is not enough — without
     a full basis two reasons sit side by side and read as one sentence:
     "`Member#1` has no `open_loan_count` set" followed by "an unknown cannot be
     ordered against an integer" rendered as "setan unknown".

     Scoped to `.enabled`, because `.rule.undecided` is a different thing with
     the same word in it — and `.trace` is a *column* flex, where `flex-basis`
     is the height, so a bare `.undecided` stretched the whole rule panel to
     the height of the scroll area. */
  .enabled .undecided,
  .invariants .reason {
    flex-basis: 100%;
    margin-left: 1.6rem;
    color: var(--verdict-unknown);
    font-size: var(--t-small);
  }

  .why {
    flex-basis: 100%;
    margin: 0 0 0 1.6rem;
    border-left: 2px solid var(--verdict-unknown);
    padding-left: var(--gap-2);
  }
  .why li {
    color: var(--verdict-unknown);
    font-size: var(--t-micro);
  }

  code {
    font-size: var(--t-small);
    color: var(--ink-dim);
    overflow-wrap: anywhere;
  }

  .verb {
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--ink-faint);
    min-width: 5.5em;
  }
  .verb.refused {
    color: var(--verdict-false);
  }

  .effects .from {
    color: var(--ink-faint);
  }
  .effects .to {
    color: var(--thing);
  }
  .arrow {
    color: var(--ink-faint);
  }
  .reason {
    flex-basis: 100%;
    margin-left: 5.5em;
    color: var(--verdict-false);
  }

  .effects button {
    color: var(--behaviour);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .none {
    margin: var(--gap-2) 0 0;
  }

  .invariants.broken {
    border-left-color: var(--verdict-false);
  }
  .enabled {
    border-left-color: var(--behaviour);
  }
  .enabled .over {
    color: var(--ink-faint);
    font-size: var(--t-micro);
  }
  .enabled .prose {
    margin: 0;
  }
</style>
