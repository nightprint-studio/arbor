<script lang="ts">
  /**
   * Gradle's own mark, monochrome — the rail button for the Gradle tool window.
   *
   * For the reason `MavenIcon` is a brand and not a metaphor: this button names a *product*, and
   * a rail draws every icon in one colour, so the glyph is `simple-icons/gradle` (a single path
   * that takes `currentColor`) rather than the coloured file-type icon the tree gives
   * `build.gradle`.
   *
   * Takes `size` (not `width`/`height`) so it drops into any icon slot typed for lucide.
   */
  import IconifyIconView from '@iconify/svelte';
  import { PROJECT_ICON } from '$lib/utils/brand-icons';

  let { size = 18 }: { size?: number } = $props();
</script>

<IconifyIconView icon={PROJECT_ICON.java_gradle} width={size} height={size} />
