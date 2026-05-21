<script lang="ts">
  import { edgePath, laneColor } from '$lib/utils/graph-renderer';
  import type { GraphEdge } from '$lib/types/git';

  let { edge }: { edge: GraphEdge } = $props();

  const color    = $derived(laneColor(edge.color_index));
  const d        = $derived(edgePath(edge));
  const isSquash = $derived(edge.edge_type === 'squash_merge');
</script>

<!-- Crisp main line — glow is handled separately in CommitGraph (blurred group pass) -->
<path
  {d}
  fill="none"
  stroke={color}
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-dasharray={isSquash ? '5 3' : undefined}
  opacity={isSquash ? 0.45 : 0.88}
/>
