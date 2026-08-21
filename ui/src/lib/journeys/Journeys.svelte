<script lang="ts">
  // What somebody set out to do, and how much of it this spec already does.
  //
  // The one view that is not derived. Every other view is a projection of what
  // the CLI reported; a journey is written by a person, against the spec, and
  // the panel's whole job is to put the two side by side — the intent on the
  // left, the specification's answer on the right, line by line.
  //
  // Read as a backlog rather than as a test run. A step the spec cannot support
  // is the ordinary state of a journey written before the thing it demands, so
  // nothing here is styled as a failure; the gaps are marked as gaps and the
  // summary counts what holds rather than what does not.

  import type { JourneyReport } from "../api/JourneyReport";
  import type { Walk } from "../api/Walk";
  import type { Verdict as VerdictKind } from "../api/Verdict";
  import Verdict from "../panels/Verdict.svelte";
  import Cast from "./Cast.svelte";
  import { MARK, MEANING, needsAttention, tally, worst } from "./verdicts";

  interface Props {
    report: JourneyReport | null;
    failure: string | null;
  }

  const { report, failure }: Props = $props();

  /** Every walk in the report, with the file it came from. */
  const walks = $derived(
    (report?.files ?? []).flatMap((file) =>
      file.walks.map((walk) => ({ walk, file: file.name, path: file.path })),
    ),
  );

  const broken = $derived((report?.files ?? []).filter((file) => file.error !== null));

  let chosen = $state<string | null>(null);
  const current = $derived(
    walks.find((entry) => entry.walk.name === chosen) ?? walks[0] ?? null,
  );

  /**
   * Which step the cast panel is reading, by number rather than index.
   *
   * Null means the end, which is the right default: a reader arrives asking
   * whether the journey worked, and only then asks where it stopped working.
   * Held by number so that switching journeys does not silently point at a
   * different step of a different walk.
   */
  let at = $state<{ journey: string; step: number } | null>(null);
  const stepIndex = $derived.by(() => {
    const steps = current?.walk.steps ?? [];
    const reading = at;
    if (!current || reading === null || reading.journey !== current.walk.name) {
      return steps.length - 1;
    }
    const found = steps.findIndex((step) => step.number === reading.step);
    return found === -1 ? steps.length - 1 : found;
  });

  function scrub(journey: string, step: number) {
    at = at?.journey === journey && at.step === step ? null : { journey, step };
  }

  function verdictOf(walk: Walk): VerdictKind {
    return worst(walk.steps.map((step) => worst(step.outcomes.map((o) => o.verdict))));
  }

  function stepVerdict(step: Walk["steps"][number]): VerdictKind {
    return worst(step.outcomes.map((outcome) => outcome.verdict));
  }
</script>

<div class="journeys">
  <aside class="list" aria-label="Journeys">
    <h2>Journeys</h2>
    {#if walks.length === 0}
      <p class="prose empty">
        {#if failure}
          {failure}
        {:else}
          None loaded. Start with <code>--journeys &lt;path&gt;</code> pointing at
          a <code>.journey</code> file or a directory of them, and they are
          re-walked every time a spec changes.
        {/if}
      </p>
    {:else}
      <p class="prose count">
        {report?.holding} of {report?.total} hold end to end.
      </p>
      <ul>
        {#each walks as entry (entry.walk.name)}
          {@const verdict = verdictOf(entry.walk)}
          <li>
            <button
              type="button"
              class:current={current?.walk.name === entry.walk.name}
              aria-current={current?.walk.name === entry.walk.name ? "true" : undefined}
              onclick={() => (chosen = entry.walk.name)}
            >
              <Verdict kind={MARK[verdict]} label={MEANING[verdict]} />
              <span class="name">{entry.walk.name}</span>
              <span class="file">{entry.file}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    {#if broken.length > 0}
      <div class="broken">
        <h3>Would not parse</h3>
        {#each broken as file (file.path)}
          <p class="prose"><strong>{file.name}</strong><br />{file.error}</p>
        {/each}
      </div>
    {/if}
  </aside>

  <section class="walk" aria-label="What the spec says about this journey">
    {#if !current}
      <p class="prose placeholder">
        A journey names the people, the acts and the surfaces the spec already
        declares, and says what should be true afterwards. This view walks each
        one through the same simulator the Simulate view uses, and reports which
        steps the specification supports.
      </p>
    {:else}
      {@const walk = current.walk}
      {@const verdict = verdictOf(walk)}
      <header>
        <div class="title">
          <Verdict kind={MARK[verdict]} label={MEANING[verdict]} />
          <h2>{walk.name}</h2>
        </div>
        <p class="address">{current.path}:{walk.line}</p>
      </header>

      {#if walk.goal.length > 0}
        <p class="prose goal">{walk.goal.join(" ")}</p>
      {/if}

      <ul class="tally">
        {#each tally(walk.steps.flatMap((s) => s.outcomes.map((o) => o.verdict))) as entry (entry.verdict)}
          <li>
            <Verdict kind={MARK[entry.verdict]} label={MEANING[entry.verdict]} />
            <span class="count">{entry.count}</span>
            <span class="meaning">{MEANING[entry.verdict]}</span>
          </li>
        {/each}
      </ul>

      {#if walk.stipulated.length > 0}
        <!-- First, and always. An agent can make any journey pass; it cannot
             make one pass invisibly, and this is where that is enforced. -->
        <div class="stipulated">
          <h3>Told rather than shown</h3>
          <p class="prose">
            The journey set these directly instead of reaching them through an
            act. Everything below stands on them.
          </p>
          <ul>
            {#each walk.stipulated as fact (fact)}
              <li><code>{fact}</code></li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if walk.notes.length > 0}
        <!-- Faults that belong to no step: a cast member the spec cannot
             supply, a `given` that wrote nothing. They sit above the steps
             because they make every step below them meaningless, and a reader
             who stops at the first thing they see has still been told. -->
        <ul class="notes">
          {#each walk.notes as note (note.line + note.about)}
            <li>
              <Verdict kind={MARK[note.verdict]} label={MEANING[note.verdict]} />
              <div class="line-body">
                <code>{note.about}</code>
                {#if note.detail}
                  <p class="detail">{note.detail}</p>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}

      <ol class="steps">
        {#each walk.steps as step, index (step.number)}
          {@const stepMark = stepVerdict(step)}
          <li class:attention={needsAttention(stepMark)}>
            <!-- Selecting a step points the cast panel at the world that step
                 left behind. It is a scrub rather than a navigation: nothing
                 else on the page moves. -->
            <button
              type="button"
              class="step-head"
              class:reading={index === stepIndex}
              aria-pressed={index === stepIndex}
              title="Read the cast as it stood after this step"
              onclick={() => scrub(walk.name, step.number)}
            >
              <Verdict kind={MARK[stepMark]} label={MEANING[stepMark]} />
              <span class="number">{step.number}</span>
              <span class="title-text">{step.title}</span>
            </button>
            <ul class="lines">
              {#each step.outcomes as outcome (outcome.line + outcome.about)}
                <li class:quiet={!needsAttention(outcome.verdict)}>
                  <Verdict
                    kind={MARK[outcome.verdict]}
                    label={MEANING[outcome.verdict]}
                  />
                  <div class="line-body">
                    <code>{outcome.about}</code>
                    {#if outcome.detail}
                      <p class="detail">{outcome.detail}</p>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          </li>
        {/each}
      </ol>

      {#if walk.ends.length > 0}
        <p class="prose ends">{walk.ends.join(" ")}</p>
      {/if}
    {/if}
  </section>

  {#if current}
    <Cast walk={current.walk} at={stepIndex} />
  {/if}
</div>

<style>
  .journeys {
    display: grid;
    /* Wider than the standard sidebar. These are CamelCase identifiers with no
       break in them, so a narrow column snaps every one mid-word. */
    grid-template-columns: 16rem minmax(0, 1fr) var(--inspector);
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .list {
    border-right: 1px solid var(--line);
    background: var(--ground-panel);
    padding: var(--gap-4) var(--gap-3);
    overflow-y: auto;
  }

  h2 {
    margin: 0;
    font-size: var(--t-micro);
    font-weight: 600;
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--ink-faint);
  }

  .count {
    margin: var(--gap-2) 0 var(--gap-3);
    color: var(--ink-dim);
  }

  .list ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .list button {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: baseline;
    gap: var(--gap-2);
    width: 100%;
    padding: var(--gap-2);
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--ink-dim);
    font: inherit;
    font-size: var(--t-small);
    text-align: left;
    cursor: pointer;
  }

  .list button:hover {
    background: var(--ground-raised);
    color: var(--ink);
  }

  .list button.current {
    background: var(--ground-raised);
    color: var(--ink);
    box-shadow: inset 2px 0 0 var(--behaviour);
  }

  .list .name {
    font-family: var(--font-mono);
    overflow-wrap: anywhere;
  }

  .list .file {
    grid-column: 2;
    font-size: var(--t-micro);
    color: var(--ink-faint);
  }

  .broken {
    margin-top: var(--gap-6);
    padding-top: var(--gap-3);
    border-top: 1px solid var(--line);
  }

  .broken h3 {
    margin: 0 0 var(--gap-2);
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--severity-error);
  }

  .walk {
    overflow-y: auto;
    padding: var(--gap-6);
    /* A measure. These are sentences, and a verdict list running the width of a
       27-inch display is unreadable however well it is coloured. */
    max-width: 54rem;
  }

  .walk header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--gap-4);
    flex-wrap: wrap;
  }

  .title {
    display: flex;
    align-items: center;
    gap: var(--gap-2);
  }

  .title h2 {
    font-family: var(--font-mono);
    font-size: var(--t-display);
    font-weight: 500;
    letter-spacing: var(--track-tight);
    text-transform: none;
    color: var(--ink);
  }

  .address {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    color: var(--ink-faint);
  }

  .goal {
    margin: var(--gap-3) 0 var(--gap-4);
    color: var(--ink-dim);
    max-width: 46rem;
  }

  .tally {
    display: flex;
    flex-wrap: wrap;
    gap: var(--gap-3);
    list-style: none;
    margin: 0 0 var(--gap-6);
    padding: var(--gap-2) 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }

  .tally li {
    display: flex;
    align-items: center;
    gap: var(--gap-1);
    font-size: var(--t-small);
    color: var(--ink-faint);
  }

  .tally .count {
    margin: 0;
    font-family: var(--font-mono);
    color: var(--ink);
  }

  .notes {
    list-style: none;
    margin: 0 0 var(--gap-4);
    padding: var(--gap-3);
    display: grid;
    gap: var(--gap-2);
    border: 1px solid var(--verdict-false);
    background: var(--ground-panel);
  }

  .notes li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: var(--gap-2);
    align-items: start;
  }

  .stipulated {
    margin-bottom: var(--gap-6);
    padding: var(--gap-3);
    border: 1px solid var(--verdict-unknown);
    border-radius: var(--radius);
    background: var(--verdict-unknown-fill);
  }

  .stipulated h3 {
    margin: 0 0 var(--gap-1);
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--verdict-unknown);
  }

  .stipulated ul {
    list-style: none;
    margin: var(--gap-2) 0 0;
    padding: 0;
  }

  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--gap-4);
  }

  .step-head {
    display: flex;
    align-items: baseline;
    gap: var(--gap-2);
    width: 100%;
    padding: var(--gap-1) var(--gap-2);
    margin-left: calc(var(--gap-2) * -1);
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .step-head:hover {
    background: var(--ground-raised);
  }

  /* The step the cast panel is reading. A left mark rather than a fill, so it
     reads as a playhead rather than as another verdict. */
  .step-head.reading {
    box-shadow: inset 2px 0 0 var(--behaviour);
  }

  .step-head .number {
    font-family: var(--font-mono);
    font-size: var(--t-small);
    color: var(--ink-faint);
  }

  .title-text {
    color: var(--ink);
    font-size: var(--t-title);
  }

  .lines {
    list-style: none;
    margin: var(--gap-2) 0 0;
    padding: 0 0 0 var(--gap-4);
    border-left: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: var(--gap-2);
  }

  .lines li {
    display: flex;
    gap: var(--gap-2);
    align-items: baseline;
  }

  /* A line that held has nothing to add. It stays legible — a reader wants to
     see what the journey said — but it does not compete with the four that
     need them. */
  .lines li.quiet code {
    color: var(--ink-faint);
  }

  .line-body {
    min-width: 0;
  }

  .lines code {
    font-family: var(--font-mono);
    font-size: var(--t-small);
    color: var(--ink-dim);
    overflow-wrap: anywhere;
  }

  .detail {
    margin: var(--gap-1) 0 0;
    font-size: var(--t-small);
    color: var(--ink-faint);
    max-width: 42rem;
  }

  .ends {
    margin-top: var(--gap-6);
    padding-top: var(--gap-3);
    border-top: 1px solid var(--line);
    color: var(--ink-dim);
    max-width: 46rem;
  }

  .placeholder,
  .empty {
    color: var(--ink-faint);
    max-width: 34rem;
  }

  .empty code {
    font-family: var(--font-mono);
    color: var(--ink-dim);
  }
</style>
