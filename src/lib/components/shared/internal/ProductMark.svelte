<script lang="ts">
  /**
   * The mark for one product of the suite — its initial, in `currentColor`.
   *
   * `currentColor` is the load-bearing part: a tab paints its own glyph, so inactive grey,
   * active foreground and the external tint all come for free and there is no per-state asset
   * to keep in step. Drop it in like any lucide icon.
   *
   * The geometry lives in `$lib/utils/product-marks` — this is only the renderer, so the same
   * table serves the tab strip, the launcher and anywhere else a product needs a face.
   *
   * An unknown id draws nothing (the accessor returns no parts), which is the right failure:
   * a placeholder mark standing in for a product would be a lie in a strip whose whole job is
   * telling products apart.
   */
  import { markParts, PRODUCT_LETTER, isProductId, type MarkWeight } from '$lib/utils/product-marks';

  interface Props {
    /** Product id (`corvus`, `bennu`, …). */
    id: string;
    /** How heavily to draw it. */
    weight?: MarkWeight;
    size?: number;
    /** Accessible name. Omit for a decorative mark sitting beside its own label — which is
     *  the tab strip's case, and where announcing "B" after "Bennu" is noise. */
    title?: string;
  }

  let { id, weight = 'line', size = 24, title }: Props = $props();

  const parts = $derived(markParts(id, weight));
  const label = $derived(title ?? (isProductId(id) ? PRODUCT_LETTER[id] : ''));
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  role={title ? 'img' : undefined}
  aria-label={title || undefined}
  aria-hidden={title ? undefined : 'true'}
  data-letter={label || undefined}
  style="display:block;flex-shrink:0"
>
  {#each parts as part, i (i)}
    {#if part.tag === 'path'}
      <path {...part.attrs} />
    {:else if part.tag === 'circle'}
      <circle {...part.attrs} />
    {:else if part.tag === 'rect'}
      <rect {...part.attrs} />
    {/if}
  {/each}
</svg>
