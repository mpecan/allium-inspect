<script lang="ts">
  // The run so far, as a strip you can walk back through.
  //
  // Each step is marked by what it did rather than only by its name: a step
  // where nothing fired looks different from one that changed the world, and one
  // the simulator could not decide looks different again — because that is the
  // one worth going back to.

  import type { Frame } from "./history";

  interface Props {
    frames: Frame[];
    at: number;
    ongo: (index: number) => void;
  }

  const { frames, at, ongo }: Props = $props();

  type Mark = "start" | "fired" | "quiet" | "undecided" | "broke";

  function markOf(frame: Frame): Mark {
    if (!frame.outcome) {
      return "start";
    }
    if (frame.outcome.invariants.some((i) => i.truth === "false" && !i.already_broken)) {
      return "broke";
    }
    if (frame.outcome.rules.some((rule) => rule.disposition === "undecided")) {
      return "undecided";
    }
    return frame.outcome.rules.some((rule) => rule.disposition === "fired") ? "fired" : "quiet";
  }

  const DESCRIPTION: Record<Mark, string> = {
    start: "the world you began with",
    fired: "a rule fired",
    quiet: "nothing fired",
    undecided: "a rule could not be decided",
    broke: "an invariant broke",
  };
</script>

<nav class="timeline" aria-label="Steps taken">
  <ol>
    {#each frames as frame, index (index)}
      {@const mark = markOf(frame)}
      <li>
        <button
          type="button"
          class={mark}
          class:current={index === at}
          aria-current={index === at ? "step" : undefined}
          title="{frame.label} — {DESCRIPTION[mark]}"
          onclick={() => ongo(index)}
        >
          <span class="dot" aria-hidden="true"></span>
          <span class="label">{frame.label}</span>
        </button>
      </li>
    {/each}
  </ol>
</nav>

<style>
  .timeline {
    flex: none;
    border-bottom: 1px solid var(--line);
    background: var(--ground-panel);
    overflow-x: auto;
  }

  ol {
    display: flex;
    align-items: stretch;
    list-style: none;
    margin: 0;
    padding: 0 var(--gap-2);
    min-height: 1.9rem;
  }

  li + li button::before {
    content: "";
    position: absolute;
    left: 0;
    top: 50%;
    width: var(--gap-2);
    height: 1px;
    background: var(--line-strong);
  }

  button {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--gap-1);
    padding: 0 var(--gap-2) 0 var(--gap-2);
    height: 100%;
    white-space: nowrap;
    color: var(--ink-faint);
    font-size: var(--t-micro);
  }
  button:hover {
    color: var(--ink);
  }
  button.current {
    color: var(--ink);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 999px;
    border: 1px solid currentColor;
    flex: none;
  }
  button.current .dot {
    background: currentColor;
  }

  .start {
    color: var(--ink-faint);
  }
  .fired {
    color: var(--verdict-true);
  }
  .quiet {
    color: var(--ink-faint);
  }
  /* The two worth going back to. */
  .undecided {
    color: var(--verdict-unknown);
  }
  .broke {
    color: var(--verdict-false);
  }

  .label {
    max-width: 16ch;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
