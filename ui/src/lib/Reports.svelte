<script lang="ts">
  // Everything the analyser said, in one place.
  //
  // `allium analyse` reports reachability, deadlock, conflict and data-flow
  // problems across the whole spec set, and this tool goes to the trouble of
  // running it. Five conflicts in a real five-module set — pairs of rules that
  // can both fire on one entity — is the single most valuable thing it ingests,
  // and it used to be the number 5 in the rail's footer, in a paragraph that
  // was not a control. The findings were reachable only by guessing which of
  // three hundred and fifty constructs to select.
  //
  // The diagnostics that no construct can carry are here for the same reason. A
  // parse error is reported where the parser gave up, which is inside no
  // declaration, so there is nowhere on the canvas to badge it.

  import type { Diagnostic } from "./api/Diagnostic";
  import type { Finding } from "./api/Finding";

  interface Props {
    findings: Finding[];
    /** Only the ones no construct carries; the rest are on their own node. */
    loose: Diagnostic[];
    /**
     * Open the named construct. The analyser reports names rather than ids, and
     * two modules can declare the same one, so its module comes with it.
     */
    onselect: (name: string, module: string) => void;
    onclose: () => void;
  }

  const { findings, loose, onselect, onclose }: Props = $props();

  let panel = $state<HTMLDialogElement | null>(null);

  $effect(() => {
    panel?.showModal();
  });

  /** Every construct a finding names, entities first, without repeats. */
  function named(finding: Finding): string[] {
    return [...new Set([...finding.entities, ...finding.rules])];
  }
</script>

<dialog
  bind:this={panel}
  aria-label="Analysis"
  onclose={onclose}
  onclick={(event) => {
    if (event.target === panel) {
      panel?.close();
    }
  }}
>
  <header>
    <div>
      <p class="eyebrow">Analysis</p>
      <h2>What allium found</h2>
    </div>
    <button type="button" class="close" title="Close" onclick={() => panel?.close()}>×</button>
  </header>

  <div class="body">
    {#if findings.length === 0 && loose.length === 0}
      <p class="prose empty">
        The analyser found nothing to report, and every diagnostic is attached to
        a construct — select one on the canvas to read it there.
      </p>
    {/if}

    {#if findings.length > 0}
      <section>
        <h3>{findings.length} finding{findings.length === 1 ? "" : "s"}</h3>
        <ul>
          {#each findings as finding, index (index)}
            <li>
              <p class="line">
                <span class="category">{finding.kind}</span>
                <span class="address">{finding.module}</span>
              </p>
              <p class="prose">{finding.summary}</p>
              <p class="links">
                {#each named(finding) as name (name)}
                  <button type="button" onclick={() => onselect(name, finding.module)}>
                    {name}
                  </button>
                {/each}
              </p>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if loose.length > 0}
      <section>
        <h3>{loose.length} not attached to a construct</h3>
        <p class="prose caveat">
          Reported against a file rather than a declaration, so nothing on the
          canvas can carry them.
        </p>
        <ul>
          {#each loose as diagnostic, index (index)}
            <li>
              <p class="line">
                <span class="category {diagnostic.severity}">{diagnostic.severity}</span>
                <span class="address">
                  {diagnostic.module}{#if diagnostic.location}:{diagnostic.location.line}{/if}
                </span>
              </p>
              <p class="prose">{diagnostic.message}</p>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
</dialog>

<style>
  dialog {
    width: min(80vw, 760px);
    max-height: min(80vh, 720px);
    padding: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    color: var(--ink);
    background: var(--ground-panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
  }
  dialog::backdrop {
    background: color-mix(in srgb, var(--ground-canvas) 72%, transparent);
  }

  header {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: var(--gap-3);
    padding: var(--gap-3);
    border-bottom: 1px solid var(--line);
  }

  h2 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 500;
  }

  h3 {
    margin: 0 0 var(--gap-2);
    font-size: var(--t-micro);
    text-transform: uppercase;
    color: var(--ink-dim);
  }

  .body {
    overflow-y: auto;
    padding: var(--gap-3);
  }

  section + section {
    margin-top: var(--gap-4);
    padding-top: var(--gap-3);
    border-top: 1px solid var(--line);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li + li {
    margin-top: var(--gap-3);
  }

  .line {
    display: flex;
    align-items: baseline;
    gap: var(--gap-2);
    margin: 0;
  }

  .category {
    font-size: var(--t-micro);
    text-transform: uppercase;
    color: var(--constraint);
  }
  .category.warning {
    color: var(--verdict-unknown);
  }
  .category.error {
    color: var(--verdict-false);
  }

  .prose {
    margin: 2px 0 0;
  }

  .links {
    display: flex;
    flex-wrap: wrap;
    gap: var(--gap-2);
    margin: var(--gap-2) 0 0;
  }
  .links button {
    padding: 1px var(--gap-2);
    font-size: var(--t-small);
    color: var(--behaviour);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  .links button:hover {
    border-color: var(--behaviour);
  }

  .empty,
  .caveat {
    color: var(--ink-dim);
  }
  .caveat {
    font-size: var(--t-micro);
    margin: 0 0 var(--gap-2);
  }

  .close {
    font-size: 1.1rem;
    line-height: 1;
    padding: 0 var(--gap-2);
    color: var(--ink-faint);
  }
  .close:hover {
    color: var(--ink);
  }
</style>
