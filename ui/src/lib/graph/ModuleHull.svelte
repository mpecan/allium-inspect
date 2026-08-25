<script lang="ts">
  // The boundary of one file, drawn behind its constructs.
  //
  // Neutral on purpose. Colour in this interface means *what a construct is* —
  // eleven hues in four bands — and spending a twelfth on "which file" would
  // put two unrelated vocabularies in the same channel. A file is a structural
  // fact, so it gets a structural device: a raised plate, an outline and a
  // name. No hue anywhere, which is what keeps it out of that vocabulary.

  interface Props {
    data: {
      module: string;
      held: number;
      width: number;
      height: number;
    };
  }

  const { data }: Props = $props();
</script>

<div class="hull" style="width: {data.width}px; height: {data.height}px;">
  <span class="name">
    {data.module}<span class="held">{data.held}</span>
  </span>
</div>

<style>
  .hull {
    /* A plate, not a wash.
       The line alone was the whole device while these boxes were bounding
       boxes drawn around wherever ELK had scattered a file's constructs:
       nearly co-extensive, so any tint compounded five deep and dimmed
       everything under it. Grouping is a layout *constraint* now, and ELK
       containers do not overlap — so the tint lands once, and one flat
       surface is what tells a boundary from the edge routes crossing it.
       A dashed hairline in `--line-strong` could not: it was darker than
       `--edge`, so the thing meant to gather a file read as one more wire. */
    background: var(--ground-raised);
    border: 1px dashed color-mix(in srgb, var(--ink-faint) 45%, transparent);
    border-radius: var(--radius);
    pointer-events: none;
    box-sizing: border-box;
  }

  .name {
    position: absolute;
    top: 3px;
    left: 10px;
    display: flex;
    align-items: baseline;
    gap: var(--gap-2);
    font-family: var(--font-mono);
    font-size: var(--t-micro);
    letter-spacing: var(--track-wide);
    text-transform: uppercase;
    color: var(--ink-dim);
  }

  .held {
    font-variant-numeric: tabular-nums;
    color: color-mix(in srgb, var(--ink-faint) 70%, transparent);
  }
</style>
