<script lang="ts">
  // The specification itself, given permanent space.
  //
  // In most graph tools the source is a tooltip: the picture is the product and
  // the text is a footnote. That is backwards here. An Allium spec *is* the
  // artifact — someone wrote it, someone reviews it, and the graph is one way of
  // looking at it. So the source gets a strip along the bottom of the window
  // that is always there, always shows where the selection is declared, and
  // opens to half the screen when you want to read rather than glance.
  //
  // Collapsed it is one line: the address, and the line the construct starts on.
  // That is usually enough to confirm you are looking at what you think.

  import type { Span } from "../api/Span";
  import { tokens } from "./highlight";
  import { sliceLines } from "./source";

  interface Props {
    path: string;
    text: string;
    span: Span | null;
    open: boolean;
    ontoggle: () => void;
  }

  const { path, text, span, open, ontoggle }: Props = $props();

  const view = $derived(sliceLines(text, span, open ? 40 : 1));
</script>

<section class="strip" class:open aria-label="Specification source">
  <header>
    <button
      type="button"
      onclick={ontoggle}
      aria-expanded={open}
      title={open ? "Collapse the source" : "Open the source"}
    >
      <span class="chevron" class:open aria-hidden="true">▸</span>
      <span class="address">
        {path}{view.firstLine > 0 ? `:${view.firstLine}` : ""}
      </span>
    </button>
    {#if !open && view.lines.length > 0}
      <code class="peek">{view.lines[0]?.text}</code>
    {/if}
    <span class="address count">
      {open ? `${view.lines.length} lines` : "open"}
    </span>
  </header>

  {#if open}
    <div class="body">
      {#if view.lines.length === 0}
        <p class="prose empty">
          Nothing selected. Pick a construct to read the source it was declared
          in.
        </p>
      {:else}
        <pre><code
            >{#each view.lines as line (line.number)}<span
                class="line"
                class:highlit={line.highlit}
                class:opens={line.opens}
                ><span class="gutter">{line.number}</span>{#each tokens(line.text) as token, at (at)}<span
                    class="t-{token.kind}">{token.text}</span>{/each}
</span>{/each}</code
          ></pre>
      {/if}
    </div>
  {/if}
</section>

<style>
  /* The document, as against the workbench around it. It follows the viewer's
   * theme like everything else; what tells the two apart is temperature rather
   * than lightness — see the `--ground-source` note in theme.css. */
  .strip {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--ground-source);
    color: var(--ink-source);
    border-top: 1px solid var(--line-strong);
  }

  .strip.open {
    height: min(46vh, 30rem);
  }

  header {
    display: flex;
    align-items: center;
    gap: var(--gap-3);
    padding: 0 var(--gap-3);
    height: 1.75rem;
    flex: none;
    background: var(--ground-source-gutter);
    border-bottom: 1px solid transparent;
  }

  .strip.open header {
    border-bottom-color: color-mix(in srgb, var(--ink-source) 12%, transparent);
  }

  header button {
    display: flex;
    align-items: center;
    gap: var(--gap-2);
    flex: none;
  }

  header .address {
    color: var(--ink-source-dim);
  }

  .chevron {
    display: inline-block;
    color: var(--ink-source-dim);
    transition: transform 140ms ease;
  }
  .chevron.open {
    transform: rotate(90deg);
  }

  .peek {
    flex: 1;
    min-width: 0;
    font-size: var(--t-small);
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--ink-source);
    opacity: 0.75;
  }

  .count {
    margin-left: auto;
    flex: none;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  pre {
    margin: 0;
    padding: var(--gap-2) 0;
  }

  code {
    font-size: var(--t-small);
    line-height: 1.5;
  }

  .line {
    display: block;
    padding-right: var(--gap-4);
    white-space: pre;
  }

  /* The declaration, marked as a *range* rather than a wash. A forty-line rule
   * flooded with highlighter is not easier to read than one without — the
   * marking has to say where the construct begins and ends without competing
   * with the text it is pointing at. So: a rule down the left edge for the
   * extent, a faint tint behind it, and a stronger band on the opening line
   * where the eye should land. */
  .highlit {
    background: color-mix(in srgb, var(--source-highlight) 34%, transparent);
    box-shadow: inset 3px 0 0 var(--source-highlight-edge);
  }
  .highlit.opens {
    background: color-mix(in srgb, var(--source-highlight) 78%, transparent);
  }

  /* Syntax. Every colour is one the reader already learned from the canvas —
   * see the `--source-*` block in theme.css for why — so nothing here picks a
   * value, it only says which run of characters wears which. */
  .t-keyword {
    color: var(--source-keyword);
  }
  .t-type {
    color: var(--source-type);
  }
  .t-string {
    color: var(--source-string);
  }
  .t-number {
    color: var(--source-number);
  }
  .t-annotation {
    color: var(--source-annotation);
  }
  /* Prose, and there is a lot of it in a real spec — six separator rules and a
   * paragraph above most declarations. Quietened rather than coloured: it is
   * the thing a reader skips past on the way to the clause. */
  .t-comment {
    color: var(--source-comment);
  }
  .t-punctuation {
    color: var(--source-punctuation);
  }

  .gutter {
    display: inline-block;
    width: 4ch;
    padding-right: var(--gap-3);
    text-align: right;
    color: var(--ink-source-dim);
    user-select: none;
    font-variant-numeric: tabular-nums;
  }

  .empty {
    padding: var(--gap-4);
    color: var(--ink-source-dim);
  }
</style>
