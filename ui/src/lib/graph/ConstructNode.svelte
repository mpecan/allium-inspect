<script lang="ts">
  // The canvas adapter: a construct's appearance, plus the two connection
  // points Svelte Flow routes edges through.
  //
  // Deliberately thin. Everything about how a construct *looks* is in
  // ConstructBody, which knows nothing about the graph library and can
  // therefore be rendered in a test without mounting a canvas.

  import { Handle, Position } from "@xyflow/svelte";

  import type { Node as SpecNode } from "../api/Node";
  import type { Severity } from "../api/Severity";
  import ConstructBody from "./ConstructBody.svelte";

  interface Props {
    data: { node: SpecNode; severity: Severity | null; dimmed: boolean };
    selected?: boolean;
  }

  const { data, selected = false }: Props = $props();
</script>

<Handle type="target" position={Position.Left} />
<ConstructBody node={data.node} severity={data.severity} dimmed={data.dimmed} {selected} />
<Handle type="source" position={Position.Right} />
