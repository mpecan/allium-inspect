<script lang="ts">
  // The tool's one point of view, as a glyph.
  //
  // Three-valued logic is the honest shape for a specification language: an
  // Allium expression can be true, false, or something a simulator has no way
  // to decide. The obvious treatment for that third case is to grey it out and
  // move on. This does the opposite — `unknown` is the only mark that gets a
  // fill, because it is the only one that needs a person to look at it.
  //
  // The severities share the component because they are the same idea seen from
  // the checker's side: error and warning are things the reader must attend to,
  // info is not.

  type Kind = "true" | "false" | "unknown" | "error" | "warning" | "info";

  interface Props {
    kind: Kind;
    label?: string;
  }

  const { kind, label }: Props = $props();

  const GLYPH: Record<Kind, string> = {
    true: "✓",
    false: "✗",
    // A hollow diamond: not a tick, not a cross, and not a dimmed version of
    // either. Undecided is its own answer.
    unknown: "◈",
    error: "✗",
    warning: "▲",
    info: "·",
  };

  const DESCRIPTION: Record<Kind, string> = {
    true: "holds",
    false: "does not hold",
    unknown: "could not be decided",
    error: "error",
    warning: "warning",
    info: "note",
  };
</script>

<span
  class="verdict {kind}"
  title={label ?? DESCRIPTION[kind]}
  aria-label={label ?? DESCRIPTION[kind]}
  role="img">{GLYPH[kind]}</span
>

<style>
  .verdict {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.15rem;
    height: 1.15rem;
    flex: none;
    border-radius: var(--radius);
    font-size: var(--t-small);
    line-height: 1;
  }

  .true {
    color: var(--verdict-true);
  }

  .false {
    color: var(--verdict-false);
  }

  /* The one that is filled. Everything else on the panel is a mark on the
   * ground; this is a chip, so the eye finds it first in a list of thirty. */
  .unknown {
    color: var(--verdict-unknown);
    background: var(--verdict-unknown-fill);
    box-shadow: inset 0 0 0 1px
      color-mix(in srgb, var(--verdict-unknown) 45%, transparent);
  }

  .error {
    color: var(--severity-error);
  }

  .warning {
    color: var(--severity-warning);
  }

  .info {
    color: var(--severity-info);
  }
</style>
