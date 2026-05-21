<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Loader2 } from 'lucide-svelte';
  import { tooltip as tooltipAction } from '$lib/actions/tooltip';
  import type { TooltipInput } from '$lib/stores/tooltip.svelte';

  type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'icon';
  type Size    = 'xs' | 'sm' | 'md' | 'lg';

  interface Props {
    variant?: Variant;
    size?: Size;
    disabled?: boolean;
    loading?: boolean;
    block?: boolean;
    type?: 'button' | 'submit' | 'reset';
    /** Plain-text tooltip. Compat-friendly: rendered through the custom Arbor
        tooltip system (no native browser title). For rich tooltips with
        shortcut chips or descriptions, use the `tooltip` prop instead. */
    title?: string;
    /** Rich tooltip input (object with `content`, `shortcut`, `description`,
        `placement`, etc.). Wins over `title` when both are set. */
    tooltip?: TooltipInput;
    ariaLabel?: string;
    /** Optional CSS color override (e.g. 'var(--brand-linear)') applied to background for primary,
        text for ghost/icon. Use sparingly — most callers should pick a variant. */
    color?: string;
    onclick?: (e: MouseEvent) => void;
    /** Leading icon snippet — rendered before the label. */
    iconStart?: Snippet;
    /** Trailing icon snippet — rendered after the label. */
    iconEnd?: Snippet;
    children?: Snippet;
    /** Bindable reference to the underlying <button> DOM element. */
    element?: HTMLButtonElement;
  }

  let {
    variant   = 'ghost',
    size      = 'md',
    disabled  = false,
    loading   = false,
    block     = false,
    type      = 'button',
    title,
    tooltip,
    ariaLabel,
    color,
    onclick,
    iconStart,
    iconEnd,
    children,
    element    = $bindable(),
  }: Props = $props();

  const tipInput = $derived<TooltipInput>(tooltip ?? title ?? '');
</script>

<button
  bind:this={element}
  {type}
  use:tooltipAction={tipInput}
  aria-label={ariaLabel}
  aria-busy={loading || undefined}
  disabled={disabled || loading}
  class="btn btn-{variant} sz-{size}"
  class:block
  class:has-color={!!color}
  style={color ? `--btn-color:${color}` : undefined}
  onclick={onclick}
>
  {#if loading}
    <Loader2 size={size === 'xs' ? 11 : size === 'sm' ? 12 : size === 'lg' ? 16 : 14} class="btn-spin" />
  {:else if iconStart}
    {@render iconStart()}
  {/if}
  {#if children}
    <span class="btn-label">{@render children()}</span>
  {/if}
  {#if iconEnd && !loading}
    {@render iconEnd()}
  {/if}
</button>

<style>
  /* The base .btn-{variant} classes come from src/app.css. This file only
     adds size variants, block layout, the optional --btn-color override,
     and the loading-spinner animation. */

  .btn { line-height: 1; }
  .btn.block { width: 100%; justify-content: center; }

  .btn-label { display: inline-flex; align-items: center; }

  /* ---- Sizes ---- */
  .sz-xs { padding: 2px 6px;   font-size: var(--font-size-xs); gap: 4px; }
  .sz-sm { padding: 3px 9px;   font-size: var(--font-size-xs); gap: 5px; }
  .sz-md { padding: 5px 12px;  font-size: var(--font-size-sm); gap: 6px; }
  .sz-lg { padding: 7px 16px;  font-size: var(--font-size-md); gap: 8px; }

  /* Icon-only override — keep square regardless of size. */
  .btn-icon.sz-xs { width: 18px; height: 18px; padding: 0; }
  .btn-icon.sz-sm { width: 22px; height: 22px; padding: 0; }
  .btn-icon.sz-md { width: 24px; height: 24px; padding: 0; }
  .btn-icon.sz-lg { width: 30px; height: 30px; padding: 0; }

  /* ---- Color override (--btn-color) ----
     Brand-coloured fills can't assume a white foreground: themes like Ayu
     Dark / Gruvbox / Monokai set `--success` to a very light yellow-green
     where `#fff` text drops below WCAG contrast.  We let the browser pick
     black or white based on the OKLCH lightness of the background:
       L > ~0.6  → light bg, dark text
       L ≤ ~0.6  → dark bg,  white text
     The `(l - 0.6) * -10` term flips to a positive value when L < 0.6,
     `clamp(0, …, 1)` turns it into 0 (black) or 1 (white). Cross-theme,
     no per-color override needed. Falls back to `#fff` on engines without
     `oklch(from …)` support (Chrome <111) via the @supports query. */
  .btn-primary.has-color {
    background: var(--btn-color);
    border-color: var(--btn-color);
    color: #fff;
  }
  @supports (color: oklch(from red l c h)) {
    .btn-primary.has-color {
      color: oklch(from var(--btn-color) clamp(0, (l - 0.6) * -10, 1) 0 0);
    }
  }
  .btn-primary.has-color:hover:not(:disabled) { filter: brightness(1.12); }

  .btn-ghost.has-color,
  .btn-icon.has-color  { color: var(--btn-color); }

  .btn-danger.has-color { color: var(--btn-color); }
  .btn-danger.has-color:hover:not(:disabled) {
    background: color-mix(in srgb, var(--btn-color) 15%, transparent);
    border-color: var(--btn-color);
  }

  /* ---- Loading ---- */
  :global(.btn-spin) { animation: btn-spin-anim 1s linear infinite; }
  @keyframes btn-spin-anim {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
