<script lang="ts" module>
  /**
   * Shared icon resolver for plugin-supplied `icon:` strings.
   *
   * Plugins can pass either:
   *   - a single-character emoji (e.g. `"🧩"`)  → rendered as text
   *   - a Lucide icon name (e.g. `"Play"`)       → rendered as an SVG component
   *
   * The Lucide name → component map lives in `$lib/utils/plugin-icons` so it
   * can be shared with non-Svelte call sites (sidebar combos, form fields,
   * pipeline editors). If a plugin picks a name that isn't in the map, it
   * falls back to a generic `Zap` icon so the button still renders.
   */
  import { Zap } from 'lucide-svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';

  /** Backward-compatible alias for the old in-component map. New code should
   *  import `PLUGIN_ICONS` from `$lib/utils/plugin-icons` directly. */
  export const LUCIDE_MAP = PLUGIN_ICONS;

  /** A string is treated as an emoji when its extended grapheme length ≤ 2.
   *  This catches common two-codepoint emojis (skin tones, flags) without
   *  pulling in a full Unicode library. */
  export function isEmojiIcon(s: string): boolean {
    return [...s].length <= 2;
  }
</script>

<script lang="ts">
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import IconifySvg from '@iconify/svelte';
  import { CLOUD_PROVIDER_ICON } from '$lib/utils/brand-icons';
  import type { IconifyIcon } from '@iconify/svelte';

  /** Sentinel names that resolve to an animated `<Spinner>` instead of a
   *  static Lucide icon. Plugins pass these as `icon = "Spinner"` (etc.) on
   *  tree rows / toolbar items when they want a "loading…" indicator that
   *  actually moves. The legacy `"Loader"` / `"Loader2"` names still render
   *  as static icons (kept for plugins that want a non-animated chevron). */
  const SPINNER_VARIANTS: Record<string, 'spin' | 'dots' | 'bars'> = {
    Spinner:     'spin',
    SpinnerDots: 'dots',
    SpinnerBars: 'bars',
  };

  /** `brand:<id>` resolves to a build-time-imported Iconify glyph from
   *  `@iconify-icons/simple-icons`. Bundled in `$lib/utils/brand-icons.ts`
   *  so we never hit `api.iconify.design` at runtime. Plugins pass e.g.
   *  `icon = "brand:google_cloud"`. */
  const BRAND_ICONS: Record<string, IconifyIcon> = {
    'google_cloud':    CLOUD_PROVIDER_ICON.google_cloud,
    'amazon_s3':       CLOUD_PROVIDER_ICON.amazon_s3,
    'microsoft_azure': CLOUD_PROVIDER_ICON.microsoft_azure,
  };
  const BRAND_PREFIX = 'brand:';

  interface Props {
    /** `"🧩"` (emoji), `"Play"` (Lucide name, see LUCIDE_MAP above), or
     *  `"plugin:<plugin>:<id>"` referencing a raw SVG registered by a plugin
     *  via `arbor.ui.icon.register({id, svg})`. */
    name?:  string;
    /** Size in px. Matches Lucide's convention; emoji glyphs are sized via CSS. */
    size?:  number;
    /** Extra CSS class — typically used to color the icon via `color: inherit`. */
    class?: string;
  }
  let { name, size = 14, class: cls = '' }: Props = $props();

  // A missing name renders nothing — plugins can opt out of icons per-item.
  const isCustom  = $derived(typeof name === 'string' && name.startsWith('plugin:'));
  const customSvg = $derived(isCustom ? contributionStore.customIcon(name as string) : null);
  const spinnerVariant = $derived(typeof name === 'string' ? SPINNER_VARIANTS[name] : undefined);
  const brandIcon = $derived(
    typeof name === 'string' && name.startsWith(BRAND_PREFIX)
      ? BRAND_ICONS[name.slice(BRAND_PREFIX.length)] ?? null
      : null
  );
  const isEmoji  = $derived(!isCustom && !spinnerVariant && !brandIcon && typeof name === 'string' && name.length > 0 && isEmojiIcon(name));
  const Lucide   = $derived(typeof name === 'string' && !isEmoji && !isCustom && !spinnerVariant && !brandIcon ? (LUCIDE_MAP[name] ?? Zap) : null);
</script>

{#if !name}
  <!-- no icon -->
{:else if spinnerVariant}
  <Spinner size={size} variant={spinnerVariant} />
{:else if brandIcon}
  <span class="brand-icon-wrap {cls}" style:width="{size}px" style:height="{size}px">
    <IconifySvg icon={brandIcon} width={size} height={size} />
  </span>
{:else if isCustom}
  {#if customSvg}
    <!-- Plugin-supplied raw SVG — sized via wrapper so currentColor still works.
         Trusted: only loaded plugins write to this registry, scoped to their
         own namespace, and the registry is wiped on reload/disable. -->
    <span
      class="custom-icon {cls}"
      style:width="{size}px"
      style:height="{size}px"
    >{@html customSvg}</span>
  {:else}
    <!-- Resolution failed (icon registered after first paint, or plugin
         disabled) — render an invisible placeholder with the same footprint
         so layout doesn't jump when the icon arrives. -->
    <span class="custom-icon {cls}" style:width="{size}px" style:height="{size}px"></span>
  {/if}
{:else if isEmoji}
  <span class="emoji-icon {cls}" style:font-size="{size}px" style:width="{size}px">{name}</span>
{:else if Lucide}
  <Lucide {size} class={cls} />
{/if}

<style>
  .emoji-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    flex-shrink: 0;
    /* Emojis should follow the surrounding text color where possible
       (monochrome emojis respect color; full-color ones ignore it —
       which is fine, they stay recognisable). */
    color: inherit;
  }

  /* Plugin-registered raw SVG. The wrapper enforces the requested size and
     ensures the inner <svg> inherits color via `currentColor`. Authors should
     set stroke="currentColor" / fill="currentColor" inside their SVG so the
     icon picks up the surrounding text color. */
  .custom-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: inherit;
  }
  .custom-icon :global(svg) {
    width: 100%;
    height: 100%;
    color: inherit;
  }
  .brand-icon-wrap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: inherit;
  }
</style>
