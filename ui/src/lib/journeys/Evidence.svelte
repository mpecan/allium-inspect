<script lang="ts">
  // What a test run showed of one step.
  //
  // Sits under the step's clauses and is deliberately quieter than them: the
  // verdicts above are the tool's answer about the *specification*, and this is
  // a separate claim about the software. Drawing them alike would invite a
  // reader to read one as the other, and neither implies the other.
  //
  // Nothing is rendered for a step nobody has photographed. Most steps of most
  // journeys are demand nobody has built yet, and a strip saying "not shown"
  // under every one of them would be the loudest thing on the page while
  // carrying no information. The count above the walk is where that is
  // accounted for.

  import type { StepEvidence } from "../api/StepEvidence";
  import { MEANING, WORD, needsAttention, pictureUrl } from "./evidence";

  interface Props {
    evidence: StepEvidence;
    /** Which frame is open, by image name, or null for none. */
    open: string | null;
    onopen: (image: string | null) => void;
  }

  const { evidence, open, onopen }: Props = $props();
</script>

<div class="evidence" class:attention={needsAttention(evidence.standing)}>
  <p class="eyebrow" title={MEANING[evidence.standing]}>
    <span class="standing {evidence.standing}">{WORD[evidence.standing]}</span>
    {#if evidence.frames.length > 0}
      <span class="when">{evidence.frames[0]?.taken_at}</span>
    {/if}
  </p>

  {#if evidence.frames.length > 0}
    <ul class="frames">
      {#each evidence.frames as frame (frame.image)}
        <li class:open={open === frame.image}>
          <button
            type="button"
            class:current={open === frame.image}
            aria-pressed={open === frame.image}
            title={frame.caption ?? frame.image}
            onclick={() => onopen(open === frame.image ? null : frame.image)}
          >
            <img src={pictureUrl(frame)} alt={frame.caption ?? `a picture of this step`} />
          </button>
          {#if frame.caption}<p class="caption">{frame.caption}</p>{/if}
        </li>
      {/each}
    </ul>
  {/if}

  <!-- Both halves in front of the reader. A picture that no longer matches its
       step is only actionable if you can see what changed, which is the whole
       reason a sealed frame stores the step's words rather than a hash. -->
  {#if evidence.says_now !== null}
    <div class="drift">
      <p class="prose caveat">
        The step was reworded after this was taken, so the picture may no longer
        show what it asks for.
      </p>
      <div class="side-by-side">
        <div>
          <p class="eyebrow">it said</p>
          <pre>{evidence.frames[0]?.said ?? ""}</pre>
        </div>
        <div>
          <p class="eyebrow">it says now</p>
          <pre>{evidence.says_now}</pre>
        </div>
      </div>
    </div>
  {/if}

  <!-- Only when nothing came of them. A marker beside a picture is ordinary and
       says nothing a reader needs; a marker with no picture behind it is the
       finding this whole scan exists for. -->
  {#if evidence.frames.length === 0 && evidence.claims.length > 0}
    <ul class="claims">
      {#each evidence.claims as claim (claim.file + claim.line)}
        <li><code>{claim.file}:{claim.line}</code></li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .evidence {
    margin: var(--gap-2) 0 0 var(--gap-4);
    padding-left: var(--gap-2);
    border-left: 1px solid var(--line);
  }
  .evidence.attention {
    border-left-color: var(--verdict-unknown);
  }

  .eyebrow {
    display: flex;
    gap: var(--gap-2);
    align-items: baseline;
    margin: 0;
  }

  .standing.shown {
    color: var(--verdict-true);
  }
  .standing.failing,
  .standing.stale,
  .standing.claimed {
    color: var(--verdict-unknown);
  }

  .when {
    color: var(--ink-dim);
    font-variant-numeric: tabular-nums;
    text-transform: none;
    letter-spacing: 0;
  }

  .frames {
    display: flex;
    flex-wrap: wrap;
    gap: var(--gap-2);
    list-style: none;
    margin: var(--gap-2) 0 0;
    padding: 0;
  }
  .frames li {
    max-width: 12rem;
  }
  /* Opening one gives it the row to itself rather than a modal. A dialog would
     be a second place to be, for a picture the reader is already looking at. */
  .frames li.open {
    max-width: 100%;
    flex-basis: 100%;
  }

  .frames button {
    display: block;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--ground-canvas);
    overflow: hidden;
    cursor: zoom-in;
  }
  .frames button.current {
    border-color: var(--behaviour);
    cursor: zoom-out;
  }
  .frames button:hover {
    border-color: var(--behaviour);
  }
  .frames img {
    display: block;
    width: 100%;
    height: auto;
  }

  .caption {
    margin: 2px 0 0;
    font-size: var(--t-small);
    color: var(--ink-dim);
  }

  .drift {
    margin-top: var(--gap-2);
  }
  .side-by-side {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
    gap: var(--gap-3);
    margin-top: var(--gap-2);
  }
  .side-by-side pre {
    margin: 2px 0 0;
    font-size: var(--t-small);
    color: var(--ink-dim);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .claims {
    list-style: none;
    margin: var(--gap-2) 0 0;
    padding: 0;
    font-size: var(--t-small);
    color: var(--ink-dim);
  }
</style>
