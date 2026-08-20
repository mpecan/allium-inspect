<script lang="ts">
  // Everything about the selected construct that would not fit on its node.
  //
  // The address at the top is not decoration. Every construct in this tool is
  // anchored to a byte offset in a file, and showing where a thing is declared
  // — always, in the same place, in the same form — is what separates reading a
  // picture of a spec from reading the spec.

  import type { Diagnostic } from "../api/Diagnostic";
  import type { Finding } from "../api/Finding";
  import type { Node } from "../api/Node";
  import type { Obligation } from "../api/Obligation";
  import type { Position } from "../api/Position";
  import { inline, paragraphs } from "../prose";
  import type { FieldLinks } from "../spec";
  import { familyOf } from "../graph/layout";
  import Verdict from "./Verdict.svelte";

  interface Props {
    node: Node | null;
    position: Position | null;
    modulePath: string;
    diagnostics: Diagnostic[];
    findings: Finding[];
    obligations: Obligation[];
    /** Where each field points, and which rules write it. */
    links: FieldLinks;
    /**
     * Open the construct with this id.
     *
     * Ids, not names — the field links come from edges the linker resolved, and
     * they already say exactly which construct is meant.
     */
    onselect: (id: string) => void;
    /**
     * Open the construct with this name.
     *
     * The analyser reports names rather than ids, so a finding's rules can only
     * be offered this way. Separate from `onselect` because both are
     * `(string) => void` and passing one where the other belongs is a mistake
     * no type can catch — which is exactly what happened.
     */
    onselectByName: (name: string) => void;
    /** The name to show for a node id, for a link out of a field. */
    nameOf: (id: string) => string;
  }

  const {
    node,
    position,
    modulePath,
    diagnostics,
    findings,
    obligations,
    links,
    onselect,
    onselectByName,
    nameOf,
  }: Props = $props();

  const family = $derived(node ? familyOf(node.kind) : "thing");
  const address = $derived(
    position ? `${modulePath}:${position.line}:${position.column}` : modulePath,
  );
</script>

<!-- The authors write `code` and **emphasis** into their comments in earnest:
     593 of the first and 75 of the second across one spec set. Showing the
     markers rather than what they mark shows the reader the scaffolding. -->
{#snippet marked(line: string)}
  {#each inline(line) as piece, index (index)}
    {#if piece.kind === "code"}<code>{piece.text}</code>
    {:else if piece.kind === "strong"}<strong>{piece.text}</strong>
    {:else}{piece.text}{/if}
  {/each}
{/snippet}

<aside class="inspector {family}" aria-label="Construct details">
  {#if !node}
    <div class="empty">
      <p class="eyebrow">Nothing selected</p>
      <p class="prose">
        Pick a construct on the canvas to see its fields, its clauses and the
        source it was declared in.
      </p>
    </div>
  {:else}
    <header>
      <p class="eyebrow">{node.kind}</p>
      <h2>{node.name}</h2>
      <p class="address">{address}</p>
    </header>

    {#if node.prose.note.length > 0}
      <!-- Above the fields, because it is why the fields are what they are. -->
      <section class="prose-block">
        {#each paragraphs(node.prose.note) as line, index (index)}
          <p class="prose">{@render marked(line)}</p>
        {/each}
      </section>
    {/if}

    {#if node.detail.type === "entity"}
      {@const detail = node.detail}
      {#if detail.kind !== "internal"}
        <p class="note prose">
          {detail.kind === "external"
            ? "Declared here, governed by another specification."
            : "A value type: compared by value, with no identity or lifecycle."}
        </p>
      {/if}

      {#if detail.fields.length > 0}
      <section>
        <h3>Fields</h3>
        <dl class="fields">
          {#each detail.fields as field, index (index)}
            <dt class:derived={field.derived} class:navigable={field.relationship}>
              {field.name}
            </dt>
            <dd>
              {#if field.enum_values.length > 0}
                {field.enum_values.join(" | ")}
              {:else if links.points.has(field.name)}
                <!-- The linker already resolved this type to a construct. It
                     was rendered as text, which left the reader searching for
                     a name the tool was holding an id for. -->
                <button
                  type="button"
                  class="to-type"
                  onclick={() => onselect(links.points.get(field.name) ?? "")}
                >{field.type_expr}</button>
              {:else}
                {field.type_expr}
              {/if}
              {#if field.when}<span class="when">when {field.when}</span>{/if}
              {#if field.note.length > 0}
                <span class="field-note">
                  {#each paragraphs(field.note) as line, index (index)}
                    <span class="prose">{@render marked(line)}</span>
                  {/each}
                </span>
              {/if}
              {#if links.written.has(field.name)}
                <span class="written">
                  <span class="written-by">written by</span>
                  {#each links.written.get(field.name) ?? [] as rule (rule)}
                    <button type="button" onclick={() => onselect(rule)}>{nameOf(rule)}</button>
                  {/each}
                </span>
              {/if}
            </dd>
          {/each}
        </dl>
        <!-- Only when a field actually shows one. A variant declares its own
             field inside its base expression rather than in its body, so it can
             be written by rules and list no fields at all — and the note would
             be explaining a column that is not there. -->
        {#if detail.fields.some((field) => links.written.has(field.name))}
          <p class="prose caveat">
            "Written by" lists rules whose postconditions name the field. A rule
            that writes one through a name this tool could not resolve to an
            entity is not listed.
          </p>
        {/if}
      </section>
      {/if}

      {#each detail.transitions as lifecycle (lifecycle.field)}
        <section>
          <h3>Lifecycle · {lifecycle.field}</h3>
          <ul class="transitions">
            {#each lifecycle.edges as step, index (index)}
              <li>
                <span>{step.from}</span>
                <span class="arrow" aria-hidden="true">→</span>
                <span class:terminal={lifecycle.terminal.includes(step.to)}>
                  {step.to}
                </span>
              </li>
            {/each}
          </ul>
          {#if lifecycle.terminal.length > 0}
            <p class="address">
              terminal: {lifecycle.terminal.join(", ")}
            </p>
          {/if}
        </section>
      {/each}
    {:else if node.detail.type === "rule"}
      {@const detail = node.detail}
      <section>
        <h3>Clauses</h3>
        <ul class="clauses">
          {#each detail.clauses as clause, index (index)}
            <li>
              <span class="keyword {clause.keyword}">{clause.keyword}</span>
              <code>{clause.text}</code>
            </li>
          {/each}
        </ul>
      </section>
      {#if detail.creates.length > 0 || detail.emits.length > 0}
        <section>
          <h3>Effects</h3>
          <ul class="effects">
            {#each detail.creates as entity, index (index)}
              <li><span class="keyword">creates</span>{entity}</li>
            {/each}
            {#each detail.emits as trigger, index (index)}
              <li><span class="keyword">emits</span>{trigger}</li>
            {/each}
          </ul>
        </section>
      {/if}
    {:else if node.detail.type === "trigger"}
      {@const detail = node.detail}
      <section>
        <h3>Happens</h3>
        <p class="prose">
          {#if detail.source === "external"}
            When something outside the system does it. You can fire this in the
            simulator.
          {:else if detail.source === "temporal"}
            When the clock passes a condition on {detail.entity ?? "an entity"}.
            The simulator offers it once you advance the current time.
          {:else}
            When {detail.entity ?? "an entity"} reaches a state that satisfies it.
            The simulator offers it as soon as a change makes it hold.
          {/if}
        </p>
        {#if detail.parameters.length > 0}
          <p class="address">carries: {detail.parameters.join(", ")}</p>
        {/if}
      </section>
    {:else if node.detail.type === "surface"}
      {@const detail = node.detail}
      {#if detail.provides.length > 0}
      <section>
        <h3>Provides</h3>
        <ul class="clauses">
          {#each detail.provides as operation, index (index)}
            <li>
              <code>{operation.trigger}({operation.parameters.join(", ")})</code>
              {#if operation.when}
                <span class="when">when {operation.when}</span>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
      {/if}
      {#if detail.exposes.length > 0}
        <section>
          <h3>Exposes</h3>
          <ul class="effects">
            {#each detail.exposes as exposed, index (index)}<li>{exposed}</li>{/each}
          </ul>
        </section>
      {/if}
      {#each detail.guarantees as guarantee, index (index)}
        <section>
          <h3>Guarantee</h3>
          <p class="name-line">{guarantee}</p>
          <p class="prose">
            Stated as prose in the spec. Nothing checks it, and the simulator
            will not claim it holds.
          </p>
        </section>
      {/each}
    {:else if node.detail.type === "invariant"}
      {@const detail = node.detail}
      <section>
        <h3>Must always hold</h3>
        {#if detail.expression}
          <code class="block">{detail.expression}</code>
        {:else}
          <p class="prose">
            Written as prose. It is part of the specification and the simulator
            will report it as unchecked rather than as holding.
          </p>
        {/if}
      </section>
    {:else if node.detail.type === "enum"}
      <section>
        <h3>Values</h3>
        <ul class="effects">
          {#each node.detail.values as value, index (index)}<li>{value}</li>{/each}
        </ul>
      </section>
    {:else if node.detail.type === "config"}
      <section>
        <h3>Parameters</h3>
        <dl class="fields">
          {#each node.detail.parameters as parameter, index (index)}
            <dt>{parameter.name}</dt>
            <dd>{parameter.default_expr ?? parameter.type_expr}</dd>
          {/each}
        </dl>
      </section>
    {:else if node.detail.type === "actor"}
      {@const detail = node.detail}
      <section>
        <h3>Identified by</h3>
        {#if detail.entity}
          <code class="block">
            {detail.entity}{detail.condition ? ` where ${detail.condition}` : ""}
          </code>
        {:else}
          <p class="prose">The spec does not say who this actor is.</p>
        {/if}
      </section>
    {:else if node.kind === "external"}
      <section>
        <h3>Not declared here</h3>
        <p class="prose">
          Something in this spec set refers to <code>{node.qualified}</code>, and
          no module in it declares that. Either the spec that governs it is not
          loaded, or the name is wrong.
        </p>
      </section>
    {/if}

    {#if node.prose.guidance.length > 0}
      <section>
        <h3>Guidance</h3>
        {#each paragraphs(node.prose.guidance) as line, index (index)}
          <p class="prose">{@render marked(line)}</p>
        {/each}
        <p class="prose caveat">
          Written as <code>@guidance</code> in the spec. It is advice to whoever
          builds this, and nothing checks it.
        </p>
      </section>
    {/if}

    {#if obligations.length > 0}
      <section>
        <h3>Owes {obligations.length} test{obligations.length === 1 ? "" : "s"}</h3>
        <ul class="obligations">
          {#each obligations as obligation (obligation.id)}
            <li>
              <span class="category">{obligation.category.replace(/_/g, " ")}</span>
              <span class="prose">{obligation.description}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if diagnostics.length > 0}
      <section>
        <h3>Reported</h3>
        <ul class="reports">
          {#each diagnostics as diagnostic, index (index)}
            <li>
              <Verdict kind={diagnostic.severity} />
              <span class="prose">{diagnostic.message}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if findings.length > 0}
      <section>
        <h3>Analysis</h3>
        <ul class="reports">
          {#each findings as finding, index (index)}
            <li>
              <span class="category">{finding.kind}</span>
              <span class="prose">{finding.summary}</span>
              {#if finding.rules.length > 0}
                <span class="rule-links">
                  {#each finding.rules as rule, index (index)}
                    <button type="button" onclick={() => onselectByName(rule)}>
                      {rule}
                    </button>
                  {/each}
                </span>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</aside>

<style>
  .inspector {
    display: flex;
    flex-direction: column;
    gap: var(--gap-4);
    height: 100%;
    overflow-y: auto;
    padding: var(--gap-4);
    background: var(--ground-panel);
    border-left: 1px solid var(--line);
  }

  .thing {
    --accent: var(--thing);
  }
  .behaviour {
    --accent: var(--behaviour);
  }
  .boundary {
    --accent: var(--boundary);
  }
  .constraint {
    --accent: var(--constraint);
  }
  .unresolved {
    --accent: var(--unresolved);
  }

  .empty {
    margin-top: 20vh;
    text-align: center;
    max-width: 26ch;
    align-self: center;
  }

  header h2 {
    margin: 1px 0 3px;
    font-size: var(--t-display);
    font-weight: 500;
    letter-spacing: var(--track-tight);
    color: var(--accent);
  }

  header .eyebrow {
    margin: 0;
  }

  header .address {
    margin: 0;
  }

  section {
    border-top: 1px solid var(--line);
    padding-top: var(--gap-3);
  }

  h3 {
    margin: 0 0 var(--gap-2);
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    font-weight: 500;
    color: var(--ink-faint);
  }

  .note {
    margin: 0;
    color: var(--accent);
  }

  .fields {
    display: grid;
    grid-template-columns: minmax(0, auto) minmax(0, 1fr);
    gap: 2px var(--gap-3);
    margin: 0;
    font-size: var(--t-small);
  }

  .fields dt {
    color: var(--ink);
  }
  .fields dt.derived {
    color: var(--ink-dim);
    font-style: italic;
  }
  .fields dt.navigable::after {
    content: " →";
    color: var(--accent);
  }
  .fields dd {
    margin: 0;
    color: var(--ink-faint);
    overflow-wrap: anywhere;
  }

  /* A type that resolved is a way through to what it names. Underlined on
   * hover only: three hundred underlines in a field list is a hedge, and the
   * reader already knows a qualified name is a reference. */
  .to-type {
    color: inherit;
    text-align: right;
  }
  .to-type:hover {
    color: var(--ink);
    text-decoration: underline;
  }

  /* Which rules assign this field, which is the question asked just before
   * changing how it is written. */
  .written {
    display: block;
    margin-top: 2px;
    font-size: var(--t-micro);
  }
  .written-by {
    color: var(--ink-faint);
  }
  .written button {
    margin-left: var(--gap-1);
    font-size: var(--t-micro);
    color: var(--behaviour);
  }
  .written button:hover {
    text-decoration: underline;
  }

  /* The paragraph an author wrote above the declaration. Set on the warm
   * document ground the source strip uses, because it *is* the document: the
   * rest of this panel is the tool's reading of a spec and this is the spec. */
  .prose-block {
    padding: var(--gap-2) var(--gap-3);
    border-left: 2px solid var(--line-strong);
    background: color-mix(in srgb, var(--ground-raised) 70%, transparent);
    border-radius: var(--radius);
  }
  .prose-block .prose + .prose {
    margin-top: var(--gap-2);
  }

  /* A field's note belongs to the field, so it sits under it rather than in a
   * section of its own — sixty-nine of them in one file would otherwise be
   * sixty-nine headings. */
  .field-note {
    display: block;
    margin-top: 2px;
    font-size: var(--t-micro);
    line-height: 1.5;
    color: var(--ink-dim);
    text-align: left;
  }
  .field-note .prose + .prose {
    display: block;
    margin-top: var(--gap-1);
  }

  .when {
    color: var(--boundary);
    margin-left: var(--gap-2);
  }

  .transitions,
  .clauses,
  .effects,
  .obligations,
  .reports {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: var(--t-small);
  }

  .transitions li {
    display: flex;
    gap: var(--gap-2);
    align-items: baseline;
  }
  .arrow {
    color: var(--ink-faint);
  }
  .terminal {
    color: var(--accent);
  }
  .terminal::after {
    content: " ▪";
  }

  .clauses li,
  .effects li,
  .obligations li,
  .reports li {
    display: flex;
    gap: var(--gap-2);
    align-items: baseline;
    flex-wrap: wrap;
    padding: 2px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
  }

  .keyword {
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--ink-faint);
    flex: none;
    min-width: 5.5em;
  }
  .keyword.ensures {
    color: var(--accent);
  }

  code {
    font-size: var(--t-small);
    color: var(--ink-dim);
    overflow-wrap: anywhere;
  }

  code.block {
    display: block;
    white-space: pre-wrap;
    padding: var(--gap-2);
    background: var(--ground-input);
    border-radius: var(--radius);
    color: var(--ink);
  }

  .category {
    font-size: var(--t-micro);
    letter-spacing: var(--track-micro);
    text-transform: uppercase;
    color: var(--accent);
    flex: none;
  }

  .name-line {
    margin: 0 0 2px;
    color: var(--accent);
  }

  .rule-links {
    display: flex;
    gap: var(--gap-2);
    flex-wrap: wrap;
  }
  .rule-links button {
    font-size: var(--t-micro);
    color: var(--behaviour);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
</style>
