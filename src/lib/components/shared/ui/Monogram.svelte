<script lang="ts" module>
  /**
   * Generic 1-2 letter monogram tile — used for workspaces, plugins, and any
   * other "thing" identity that benefits from a colour-coded glyph (icons
   * are reserved for actions, monograms for entities).
   *
   * Sibling: `Avatar.svelte` covers PERSON identity (round, hue-from-hash).
   * Use Monogram for objects (workspace, plugin, project), Avatar for users.
   */

  /** Derive the canonical 1-2 letter monogram from a name. */
  export function deriveInitials(name: string): string {
    const parts = name.trim().split(/\s+/).filter(Boolean);
    if (parts.length === 0) return '?';
    if (parts.length === 1) {
      const p = parts[0];
      const first  = p.charAt(0).toUpperCase();
      const second = p.length > 1 ? p.charAt(1).toUpperCase() : '';
      return (first + second).trim() || first;
    }
    return (parts[0].charAt(0) + parts[1].charAt(0)).toUpperCase();
  }
</script>

<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';

  type Variant = 'square' | 'circle' | 'outline' | 'dot';

  interface Props {
    /** Drives the initials when `initials` is not supplied, and `title`. */
    name: string;
    /** Override the auto-derived initials (e.g. plugin shows just first letter). */
    initials?: string;
    /** Background colour — any CSS color or `var(--…)` reference. */
    color?: string;
    /** Pixel size of the shorter edge. Reasonable range: 12–48. */
    size?: number;
    /**
     * Visual style:
     *   'square'  — IntelliJ-style rounded square with sheen + bright text (default)
     *   'circle'  — same fill, full circle
     *   'outline' — transparent bg, coloured border + initials in `color`
     *   'dot'     — solid disc, no glyph (status indicator)
     */
    variant?: Variant;
    /** Hide the tile (greyed) — used to indicate disabled/unavailable items. */
    disabled?: boolean;
    /** Hover/focus title; falls back to `name`. */
    title?: string;
    /** Foreground override (for `square`/`circle`/`outline`). When unset, defaults to `--ws-color-fg` for square/circle, and `color` for outline. */
    fg?: string;
    /** Extra class on the root span. */
    class?: string;
  }

  let {
    name,
    initials,
    color    = 'var(--accent)',
    size     = 18,
    variant  = 'square',
    disabled = false,
    title,
    fg,
    class: rootClass = '',
  }: Props = $props();

  const text = $derived(initials ?? deriveInitials(name));
</script>

<span
  class="mono v-{variant} {rootClass}"
  class:disabled
  style="--mono-size: {size}px; --mono-bg: {color};{fg ? ` --mono-fg: ${fg};` : ''}"
  use:tooltip={title ?? name}
  aria-hidden="true"
>
  {#if variant !== 'dot'}
    <span class="mono-text">{text}</span>
  {/if}
</span>

<style>
  .mono {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--mono-size);
    height: var(--mono-size);
    flex-shrink: 0;
    user-select: none;
    /* Subtle tonal ring keeps the tile readable on either elevated or base bg. */
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--mono-bg) 38%, transparent);
    transition: opacity var(--transition-fast), filter var(--transition-fast);
  }
  .mono.disabled {
    opacity: 0.55;
    filter: grayscale(0.55);
  }

  .mono-text {
    color: var(--mono-fg, var(--ws-color-fg, #ffffff));
    font-family: var(--font-ui-sans);
    font-weight: 600;
    font-size: calc(var(--mono-size) * 0.44);
    letter-spacing: 0.01em;
    line-height: 1;
    text-shadow: 0 1px 0 rgba(0, 0, 0, 0.35);
  }

  /* ── Square (default) ──────────────────────────────────────────────── */
  .mono.v-square {
    border-radius: calc(var(--mono-size) * 0.22);
    background:
      linear-gradient(
        180deg,
        color-mix(in srgb, white 14%, transparent) 0%,
        color-mix(in srgb, black 18%, transparent) 100%
      ),
      var(--mono-bg);
  }

  /* ── Circle ─────────────────────────────────────────────────────────── */
  .mono.v-circle {
    border-radius: 50%;
    background:
      linear-gradient(
        180deg,
        color-mix(in srgb, white 14%, transparent) 0%,
        color-mix(in srgb, black 18%, transparent) 100%
      ),
      var(--mono-bg);
  }

  /* ── Outline ────────────────────────────────────────────────────────── */
  .mono.v-outline {
    border-radius: calc(var(--mono-size) * 0.22);
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--mono-bg);
  }
  .mono.v-outline .mono-text {
    color: var(--mono-fg, var(--mono-bg));
    text-shadow: none;
    letter-spacing: 0.02em;
  }

  /* ── Dot (no glyph) ─────────────────────────────────────────────────── */
  .mono.v-dot {
    border-radius: 50%;
    background: var(--mono-bg);
  }
</style>
