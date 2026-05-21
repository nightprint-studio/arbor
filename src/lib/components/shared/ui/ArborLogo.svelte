<script lang="ts">
  /**
   * Single source of truth for the Arbor app mark.
   *
   * Default: the bundled `static/arbor-logo.svg` rendered via `<img>` so
   * the browser cache handles it once for the whole session.
   * Override:  inline SVG markup supplied by a plugin via
   * `arbor.ui.set_branding{ svg = "..." }` — rendered as `{@html}` so it
   * inherits surrounding text color when the SVG uses `currentColor`.
   *
   * Use this widget anywhere the Arbor identity appears in-app so a single
   * branding override paints every surface at once. The matching backend
   * helper (`AppState.branding`) ensures HTML stats exports stay branded
   * with the same logo without a second round-trip through the plugin.
   */
  import { brandingStore } from '$lib/stores/branding.svelte';

  interface Props {
    size?: number;
    /** Optional `aria-label` / `<img alt>` override (defaults to "Arbor"). */
    alt?:  string;
    class?: string;
  }
  let { size = 24, alt = 'Arbor', class: cls = '' }: Props = $props();

  const override = $derived(brandingStore.logoSvg);
</script>

{#if override}
  <span
    class="arbor-logo arbor-logo-inline {cls}"
    role="img"
    aria-label={alt}
    style:width="{size}px"
    style:height="{size}px"
  >{@html override}</span>
{:else}
  <img
    class="arbor-logo arbor-logo-img {cls}"
    src="/arbor-logo.svg"
    {alt}
    width={size}
    height={size}
    draggable="false"
  />
{/if}

<style>
  .arbor-logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    line-height: 0;
    user-select: none;
  }
  .arbor-logo-img {
    object-fit: contain;
  }
  .arbor-logo-inline :global(svg) {
    width:  100%;
    height: 100%;
    display: block;
  }
</style>
