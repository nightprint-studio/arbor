<script lang="ts" module>
  /**
   * Monochrome brand mark — same simple-icons glyphs as <BrandTile>, but with
   * no tile background and no fixed brand color. The glyph is rendered with
   * `currentColor`, so it adopts whatever color the parent sets (e.g. the
   * sidebar's `--text-secondary` / `--accent`). Use this where a colored
   * brand square would clash with the rest of the icon set — most notably
   * the activity bar, where every icon must share one color so the rail
   * reads as a unified surface.
   *
   * For situations where the brand owns its own swatch (auth tiles, settings
   * cards, welcome screens) keep using <BrandTile> instead.
   */
  import type { ProviderBrand } from '$lib/utils/brand-icons';
  export type Brand = ProviderBrand;
</script>

<script lang="ts">
  import Icon from '@iconify/svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { PROVIDER_ICON } from '$lib/utils/brand-icons';

  interface Props {
    brand: Brand;
    size?: number;
    title?: string;
    class?: string;
  }

  let { brand, size = 20, title, class: rootClass = '' }: Props = $props();

  const iconObj = $derived(PROVIDER_ICON[brand]);
  const label   = $derived(title ?? brand.charAt(0).toUpperCase() + brand.slice(1));
</script>

<span class="brand-icon {rootClass}" use:tooltip={label} aria-label={label}>
  <Icon icon={iconObj} width={size} height={size} />
</span>

<style>
  .brand-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: inherit;
    flex-shrink: 0;
  }
</style>
