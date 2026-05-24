<script lang="ts" module>
  /**
   * Branded square logo tile for third-party providers (Git hosts + issue
   * trackers). Encapsulates the brand colour, the brand SVG (via Iconify's
   * `simple-icons` collection — the canonical brand assets), and — crucially —
   * a HARD-CODED `#fff` foreground.
   *
   * ── Why is the foreground absolute (not a theme token)? ───────────────────
   * Brand colours (#24292e GitHub, #fc6d26 GitLab, #0052cc Jira, #5e6ad2
   * Linear, …) are owned by their respective brand guidelines. They do not
   * adapt to our app theme — a GitHub mark is *always* the dark grey GitHub
   * background, on every site. To stay legible on those fixed brand
   * backgrounds, the glyph must keep a fixed bright contrast.
   *
   * Earlier versions used `var(--text-on-accent)` for the glyph colour. That
   * resolves to white in the original dark theme, but in light/inverted
   * themes (introduced by "themes part 2") it can resolve to dark, making
   * the glyph nearly invisible against the brand background. Brand tiles
   * MUST NOT borrow theme tokens for the glyph — they own their contrast.
   * ──────────────────────────────────────────────────────────────────────────
   */
  import type { ProviderBrand } from '$lib/utils/brand-icons';
  export type Brand = ProviderBrand;

  /** Canonical brand colours — absolute, not theme-derived. */
  export const BRAND_BG: Record<Brand, string> = {
    github:    '#24292e',
    gitlab:    '#fc6d26',
    bitbucket: '#0052cc',
    linear:    '#5e6ad2',
    jira:      '#0052cc',
  };
</script>

<script lang="ts">
  import Icon from '@iconify/svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { PROVIDER_ICON } from '$lib/utils/brand-icons';

  interface Props {
    brand: Brand;
    /** Pixel size of the inner glyph. Default 20. */
    size?: number;
    /** Pixel size of the outer square. Default = max(size + 16, 36). */
    tileSize?: number;
    /** Disabled visual treatment (greyed). */
    disabled?: boolean;
    /** Override the title attribute (defaults to a capitalised brand name). */
    title?: string;
    /** Extra class on the root span. */
    class?: string;
  }

  let {
    brand,
    size     = 20,
    tileSize,
    disabled = false,
    title,
    class: rootClass = '',
  }: Props = $props();

  const tileBg  = $derived(BRAND_BG[brand]);
  const tile    = $derived(tileSize ?? Math.max(size + 16, 36));
  const iconObj = $derived(PROVIDER_ICON[brand]);
  const label   = $derived(title ?? brand.charAt(0).toUpperCase() + brand.slice(1));
</script>

<span
  class="brand-tile {rootClass}"
  class:disabled
  style="--bt-size: {tile}px; --bt-bg: {tileBg};"
  use:tooltip={label}
>
  <Icon icon={iconObj} width={size} height={size} />
</span>

<style>
  .brand-tile {
    width: var(--bt-size);
    height: var(--bt-size);
    border-radius: var(--radius-sm);
    background: var(--bt-bg);
    /* Foreground is hard-coded #fff: brand backgrounds are absolute, the
       glyph contrast must not depend on the active app theme. */
    color: #fff;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: opacity var(--transition-fast), filter var(--transition-fast);
  }
  .brand-tile.disabled {
    opacity: 0.55;
    filter: grayscale(0.5);
  }
</style>
