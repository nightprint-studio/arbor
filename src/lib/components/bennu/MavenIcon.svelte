<script lang="ts">
  /**
   * Maven's own mark, monochrome — the rail button for the Maven tool window.
   *
   * It used to be the same `Hammer` the Build button wears, which made the one rail icon
   * that names a *product* look like the one that names an *action*, and made two different
   * buttons in the same window identical. A brand is the one case where the literal mark
   * beats a metaphor: nobody has to be taught what it stands for.
   *
   * `simple-icons/apachemaven` rather than the colourful `vscode-icons` file glyph the tree
   * gives `pom.xml`: a rail draws every icon in one colour so the surface reads as one
   * surface, and this glyph is a single path that takes `currentColor`.
   *
   * Takes `size` (not `width`/`height`) so it drops into any icon slot typed for lucide —
   * which is what the activity rail's `icon` is.
   */
  import IconifyIconView from '@iconify/svelte';
  import { PROJECT_ICON } from '$lib/utils/brand-icons';

  let { size = 18 }: { size?: number } = $props();
</script>

<IconifyIconView icon={PROJECT_ICON.java_maven} width={size} height={size} />
