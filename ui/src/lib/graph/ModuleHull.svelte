<script lang="ts">
  // The boundary of one file, drawn behind its constructs.
  //
  // Neutral on purpose. Colour in this interface means *what a construct is* —
  // eleven hues in four bands — and spending a twelfth on "which file" would
  // put two unrelated vocabularies in the same channel. A file is a structural
  // fact, so it gets a structural device: a dashed line and a name.

  interface Props {
    data: {
      module: string;
      held: number;
      width: number;
      height: number;
      /** Where in the stack this box sits, so its name clears the others. */
      depth: number;
    };
  }

  const { data }: Props = $props();
</script>

<div class="hull" style="width: {data.width}px; height: {data.height}px;">
  <span class="name" style="top: {3 + data.depth * 14}px;">
    {data.module}<span class="held">{data.held}</span>
  </span>
</div>

<style>
  .hull {
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius);
    /* No fill. A tint reads fine on one box and compounds on five: because
       the layout does not group by file, these end up nearly co-extensive,
       and five stacked washes dim every construct they are drawn behind. The
       line alone is the whole device. */
    pointer-events: none;
    box-sizing: border-box;
  }

  .name {
    position: absolute;
    left: 10px;
    display: flex;
    align-items: baseline;
    gap: var(--gap-2);
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    letter-spacing: var(--track-wide);
    text-transform: uppercase;
    color: var(--ink-faint);
  }

  .held {
    font-variant-numeric: tabular-nums;
    color: color-mix(in srgb, var(--ink-faint) 70%, transparent);
  }
</style>
