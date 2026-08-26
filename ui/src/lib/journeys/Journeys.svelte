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
  import SourceStrip from "../panels/SourceStrip.svelte";
  import { spanOfLine } from "../panels/source";
  import Cast from "./Cast.svelte";
  import Evidence from "./Evidence.svelte";
  import { MARK, MEANING, needsAttention, tally, worst } from "./verdicts";
  import {
    WORD,
    at as evidenceAt,
    axes as evidenceAxes,
    type Narrowing,
    undeclaredIn,
    summary as evidenceSummary,
    worthShowing,
  } from "./evidence";

  interface Props {
    report: JourneyReport | null;
    failure: string | null;
  }

  const { report, failure }: Props = $props();

  /** Every walk in the report, with the file it came from. */
  const walks = $derived(
    (report?.files ?? []).flatMap((file) =>
      file.walks.map((walk) => ({ walk, file: file.name, path: file.path, text: file.text })),
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

  /**
   * Where the source strip is looking.
   *
   * The line of the step being read, or the journey's own line when the reader
   * has not picked one. A journey is a document somebody wrote, the same as the
   * spec it is written against, and until now it was the one thing in this tool
   * you could not read — you could see what the spec said about a line without
   * ever seeing the line.
   */
  const sourceSpan = $derived.by(() => {
    if (!current) {
      return null;
    }
    const step = current.walk.steps[stepIndex];
    const line = at?.journey === current.walk.name && step ? step.line : current.walk.line;
    return spanOfLine(current.text, line);
  });

  let sourceOpen = $state(false);

  function scrub(journey: string, step: number) {
    at = at?.journey === journey && at.step === step ? null : { journey, step };
  }

  /**
   * Which picture is open, by image name.
   *
   * Held here rather than per step so that opening one closes the last: two
   * screenshots at full width, of two different steps, is a page nobody can
   * read against the clauses either of them is under.
   */
  let openFrame = $state<string | null>(null);

  /** Where each step of the current walk stands, and what it was shown by. */
  const standings = $derived(report?.evidence);

  /**
   * The questions the evidence can be narrowed by, and the reader's answers.
   *
   * Per journey. This was across the whole report at first, so the controls
   * would not come and go — but a journey can now *declare* its axes, and a
   * declaration belongs to the journey that made it. A set where one journey
   * cares about `platform` should not offer that control on the ones that do
   * not, and the control appearing is now information rather than noise.
   */
  const axes = $derived(current ? evidenceAxes(standings, current.walk.name) : []);
  const odd = $derived(current ? undeclaredIn(standings, current.walk.name) : []);
  let narrowing = $state<Narrowing>({});

  function narrowTo(key: string, value: string) {
    // An empty value is "either", which is the default: showing both a dark and
    // a light picture of one step is the ordinary reading, and the dropdown is
    // for when a reader wants one of them rather than for choosing at all.
    const { [key]: _dropped, ...rest } = narrowing;
    narrowing = value === "" ? rest : { ...rest, [key]: value };
  }

  const shownHere = $derived(
    current
      ? evidenceSummary(
          standings,
          current.walk.name,
          current.walk.steps.map((step) => step.number),
        )
      : [],
  );

  /** Every verdict in a walk, including the ones outside its steps. */
  function verdictsOf(walk: Walk): VerdictKind[] {
    // The notes count. A cast the spec cannot supply makes every step below it
    // meaningless, and leaving them out of the summary put a tick beside a
    // journey whose people do not exist — the panel showed the fault and the
    // heading above it disagreed.
    return [
      ...walk.steps.flatMap((step) => step.outcomes.map((outcome) => outcome.verdict)),
      ...walk.notes.map((note) => note.verdict),
    ];
  }

  function verdictOf(walk: Walk): VerdictKind {
    return worst(verdictsOf(walk));
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
              <!-- The readable name. The identifier is what everything keys
                   on and is one hover away, because an evidence marker spells
                   it and somebody reading a marker has to find it here. -->
              <span class="name" title={entry.walk.name}>{entry.walk.title}</span>
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
          <h2 title={walk.name}>{walk.title}</h2>
        </div>
      </header>

      {#if walk.goal.length > 0}
        <p class="prose goal">{walk.goal.join(" ")}</p>
      {/if}

      <ul class="tally">
        {#each tally(verdictsOf(walk)) as entry (entry.verdict)}
          <li>
            <Verdict kind={MARK[entry.verdict]} label={MEANING[entry.verdict]} />
            <span class="count">{entry.count}</span>
            <span class="meaning">{MEANING[entry.verdict]}</span>
          </li>
        {/each}
      </ul>

      <!-- A separate line from the tally above it, and separate on purpose.
           That one says what the *specification* supports; this says whether
           anybody has shown the software doing it. Reading either as the other
           gets the wrong answer in both directions. -->
      {#if shownHere.length > 0 || axes.length > 0}
        <div class="evidence-line">
          <ul class="tally evidence-tally">
            {#each shownHere as entry (entry.standing)}
              <li>
                <span class="count">{entry.count}</span>
                <span class="meaning standing-{entry.standing}">{WORD[entry.standing]}</span>
              </li>
            {/each}
          </ul>

          <!-- One control per question the pictures answer. The keys come from
               what the harness wrote, so this is empty until somebody tags
               something and grows a control the day they do. -->
          {#each axes as axis (axis.key)}
            <label class="axis">
              <span>{axis.key}</span>
              <select
                value={narrowing[axis.key] ?? ""}
                onchange={(event) => narrowTo(axis.key, event.currentTarget.value)}
              >
                <option value="">either</option>
                {#each axis.values as value (value)}
                  <!-- A declared value with nothing behind it is still offered.
                       It is what the journey asked for, and hiding it would
                       leave the demand invisible — the reader picks it and is
                       told plainly that nothing has answered it yet. -->
                  <option {value}>
                    {value}{axis.missing.includes(value) ? " — none yet" : ""}
                  </option>
                {/each}
              </select>
            </label>
          {/each}
        </div>
      {/if}

      <!-- Tags outside what this journey asked to be shown. Usually a typo for
           one of them, and a typo here is a second axis nobody meant rather
           than an error anybody sees. -->
      {#if odd.length > 0}
        <ul class="odd-tags">
          {#each odd as tag (tag.image + tag.key)}
            <li>
              <span class="what">{tag.key_undeclared ? "no such tag" : "no such value"}</span>
              <code>{tag.key}={tag.value}</code>
              <span class="where">{tag.image}</span>
            </li>
          {/each}
        </ul>
      {/if}

      {#if walk.after}
        <!-- Before everything, because everything below is answered in a world
             this journey did not make. A list of lines cannot be given for the
             end state of another journey — it is a world — so what is given is
             the thing that decides whether any of this means anything. -->
        <div class="stipulated ground" class:unsound={walk.after.verdict !== "specified"}>
          <h3>Continues from</h3>
          <p class="prose">
            This journey starts where <strong>{walk.after.title}</strong> ended,
            and never says again what that one established.
            {#if walk.after.verdict === "specified"}
              That one holds end to end, so this one begins somewhere the
              specification supports.
            {:else}
              <strong>That one does not hold</strong>, so this one begins
              somewhere the specification does not fully support — and cannot
              come out better than it.
            {/if}
          </p>
          <ul>
            <li>
              <code>{walk.after.journey}</code>
              <span class="whereabouts">
                {walk.after.held} of {walk.after.of} steps held
              </span>
            </li>
          </ul>
        </div>
      {/if}

      {#if walk.inherited.length > 0}
        <!-- Before the journey's own, because it was there before the journey
             said anything. A step holding on account of a line elsewhere in
             the file is the same passing invisibly, one level out — so the
             whole of what the file laid out is here, without opening it. -->
        <div class="stipulated inherited">
          <h3>Laid out by the file</h3>
          <p class="prose">
            This journey's file sets these up before any of its journeys begin.
            An <em>overridden</em> line was set here and then set again by this
            journey, which is the one worth reading twice.
          </p>
          <ul>
            {#each walk.inherited as line (line.said)}
              <li>
                {#if line.overridden}<span class="overridden">overridden</span>{/if}
                <code>{line.said}</code>
                <span class="whereabouts">line {line.line}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

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
            {#each walk.stipulated as fact (fact.said + (fact.through ?? ""))}
              <li>
                {#if fact.through}<span class="whereabouts">through {fact.through}</span>{/if}
                <code>{fact.said}</code>
              </li>
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
          {@const shown = evidenceAt(standings, walk.name, step.number)}
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
            {#if worthShowing(shown)}
              <Evidence
                evidence={shown}
                open={openFrame}
                onopen={(image) => (openFrame = image)}
                {narrowing}
              />
            {/if}
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

  {#if current}
    <div class="source">
      <SourceStrip
        path={current.path}
        text={current.text}
        span={sourceSpan}
        open={sourceOpen}
        ontoggle={() => (sourceOpen = !sourceOpen)}
        label="Journey source"
      />
    </div>
  {/if}
</div>

<style>
  .journeys {
    display: grid;
    /* Wider than the standard sidebar. These are CamelCase identifiers with no
       break in them, so a narrow column snaps every one mid-word. */
    grid-template-columns: 16rem minmax(0, 1fr) var(--inspector);
    /* The strip spans all three columns along the bottom, the way it does over
       the canvas — the journey is the artifact here, so it gets the same
       permanent space the spec gets rather than a panel you have to go and
       find. */
    grid-template-rows: minmax(0, 1fr) auto;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .source {
    grid-column: 1 / -1;
    min-width: 0;
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

  /* Reported rather than styled as a failure: nothing here is broken, and the
     picture it names is a real picture of a real run. */
  .odd-tags {
    list-style: none;
    margin: var(--gap-2) 0 0;
    padding: 0;
    font-size: var(--t-small);
  }
  .odd-tags li {
    display: flex;
    gap: var(--gap-2);
    align-items: baseline;
  }
  .odd-tags .what {
    color: var(--verdict-unknown);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .odd-tags .where {
    color: var(--ink-dim);
  }

  .evidence-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--gap-3);
  }

  .axis {
    display: flex;
    align-items: center;
    gap: var(--gap-2);
    font-size: var(--t-small);
    color: var(--ink-dim);
  }
  .axis select {
    font: inherit;
    font-size: var(--t-small);
    color: var(--ink);
    background: var(--ground-input);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 1px var(--gap-2);
  }

  .evidence-tally .standing-shown {
    color: var(--verdict-true);
  }
  .evidence-tally .standing-failing,
  .evidence-tally .standing-stale,
  .evidence-tally .standing-claimed {
    color: var(--verdict-unknown);
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

  /* The same device, a step quieter: what a file laid out is context, and what
     this journey was *told* is the thing a reader is being asked to weigh. */
  .inherited,
  .ground {
    border-color: var(--line-strong);
    background: var(--ground-raised);
  }

  /* Ground that does not hold is the one thing here a reader must not skim. */
  .ground.unsound {
    border-color: var(--verdict-unknown);
  }

  .inherited h3,
  .ground h3 {
    color: var(--ink-dim);
  }

  .ground.unsound h3 {
    color: var(--verdict-unknown);
  }

  .overridden {
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    letter-spacing: var(--track-wide);
    text-transform: uppercase;
    color: var(--verdict-unknown);
  }

  .whereabouts {
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    color: var(--ink-faint);
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
