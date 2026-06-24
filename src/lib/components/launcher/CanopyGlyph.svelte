<script lang="ts">
  /**
   * Renders one bird glyph (24×24, `currentColor`) by product id. Used by the
   * tree nodes, the detail card tile and the tooltip — `color` is inherited so
   * the node accent tints the mark.
   */
  import { glyphParts } from './canopyGlyphs';

  interface Props { id: string; size?: number; }
  let { id, size = 24 }: Props = $props();

  const parts = $derived(glyphParts(id));
</script>

<svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" style="display:block">
  {#each parts as p}
    {#if p.tag === 'path'}
      <path {...p.attrs} />
    {:else if p.tag === 'circle'}
      <circle {...p.attrs} />
    {:else if p.tag === 'ellipse'}
      <ellipse {...p.attrs} />
    {:else if p.tag === 'line'}
      <line {...p.attrs} />
    {/if}
  {/each}
</svg>
