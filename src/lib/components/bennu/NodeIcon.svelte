<script lang="ts">
  /**
   * Node's own mark, monochrome — the rail button for the Node tool window.
   *
   * Same reasoning as `GradleIcon` and `MavenIcon`: the button names a *product*, and a rail draws
   * every icon in one colour, so the glyph is the brand mark that takes `currentColor` rather than a
   * metaphor. Takes `size` so it drops into any icon slot typed for lucide.
   */
  import IconifyIconView from '@iconify/svelte';
  import { PROJECT_ICON } from '$lib/utils/brand-icons';

  let { size = 18 }: { size?: number } = $props();
</script>

<IconifyIconView icon={PROJECT_ICON.node_js} width={size} height={size} />
