<script lang="ts">
  // An edge drawn along the route the layout engine chose for it.
  //
  // Svelte Flow's own edges run from one node's handle to the other's, which is
  // the right answer for a canvas whose nodes the reader drags around. These
  // nodes are placed by ELK, which routed the edges at the same time — around
  // the boxes rather than across them — and following that route is the whole
  // difference between a diagram and a bowl of spaghetti.
  //
  // Falls back to a bezier when there is no route, which happens only when the
  // layout failed and the grid fallback took over.

  import { BaseEdge, getBezierPath, type EdgeProps } from "@xyflow/svelte";

  import { midpoint, pathThrough, type Point } from "./route";

  const {
    id,
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    label,
    labelStyle,
    style,
    markerEnd,
    data,
  }: EdgeProps = $props();

  const points = $derived((data as { points?: Point[] } | undefined)?.points);

  const drawn = $derived.by(() => {
    if (points !== undefined && points.length > 1) {
      const middle = midpoint(points);
      return { path: pathThrough(points), labelX: middle.x, labelY: middle.y };
    }
    const [path, labelX, labelY] = getBezierPath({
      sourceX,
      sourceY,
      sourcePosition,
      targetX,
      targetY,
      targetPosition,
    });
    return { path, labelX, labelY };
  });
</script>

<BaseEdge
  {id}
  path={drawn.path}
  {label}
  labelX={drawn.labelX}
  labelY={drawn.labelY}
  {labelStyle}
  {style}
  {markerEnd}
/>
