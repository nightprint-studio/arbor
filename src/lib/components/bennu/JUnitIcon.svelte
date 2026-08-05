<script lang="ts">
  /**
   * JUnit's own mark, monochrome — the rail button for the Tests tool window.
   *
   * Same reasoning as {@link MavenIcon}: this button names a *product*, and a brand is the one
   * case where the literal mark beats a metaphor — nobody has to be taught what it stands for.
   * A flask would put a second lab-glassware icon in a window that already has one, and would
   * say "experiment" where the panel says "the tests this project declares".
   *
   * `simple-icons/junit5`: a single path taking `currentColor`, which is what a rail needs —
   * every icon on it is drawn in one colour so the surface reads as one surface.
   *
   * Takes `size` (not `width`/`height`) so it drops into any icon slot typed for lucide —
   * which is what the activity rail's `icon` is.
   */
  import IconifyIconView from '@iconify/svelte';
  import junitIcon from '@iconify-icons/simple-icons/junit5';

  let { size = 18 }: { size?: number } = $props();
</script>

<IconifyIconView icon={junitIcon} width={size} height={size} />
