<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Loader2 } from 'lucide-svelte';
  import { tooltip as tooltipAction } from '$lib/actions/tooltip';
  import type { TooltipInput } from '$lib/stores/tooltip.svelte';

  type Variant = 'primary' | 'secondary' | 'ghost' | 'outline' | 'danger' | 'icon' | 'tonal';
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
    /** Set when this button opens something — a dropdown, a disclosure. Omitted
     *  entirely when absent, so an ordinary button gains no spurious state. */
    ariaExpanded?: boolean;
    /** Che **cosa** apre, per chi legge con uno screen reader: `aria-expanded` da solo
     *  dice «aperto/chiuso» e non dice di che. Va insieme a `ariaExpanded`, e come quello
     *  sparisce del tutto quando non c'è. */
    ariaHaspopup?: 'menu' | 'listbox' | 'tree' | 'grid' | 'dialog' | true;
    /** Optional CSS color override (e.g. 'var(--brand-linear)') applied to background for primary,
        text for ghost/icon. Use sparingly — most callers should pick a variant. */
    color?: string;
    /**
     * Colour the **icon** only, leaving the label in ordinary text.
     *
     * For toolbars, where a row of identical grey glyphs makes the reader parse
     * shapes to find Run. Colour here marks what an action *does to the world* —
     * green starts something, red stops it, accent writes something — and nothing
     * else takes a colour, because colouring every button is the same as colouring
     * none of them.
     *
     * Distinct from {@link color}, which tints the whole control (label included)
     * and turns a toolbar button into a call to action. Same name and same meaning
     * as `iconColor` on `Dropdown` and `ContextMenu` items.
     *
     * Drops out when the button is unavailable: a bright green Run on a greyed-out
     * button says "go" while the button says "you cannot".
     */
    iconColor?: string;
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
    ariaExpanded,
    ariaHaspopup,
    color,
    iconColor,
    onclick,
    iconStart,
    iconEnd,
    children,
    element    = $bindable(),
  }: Props = $props();

  const tipInput = $derived<TooltipInput>(tooltip ?? title ?? '');

  /** Both colour overrides, as one `style` — either, neither or both. */
  const styleVars = $derived(
    [
      color ? `--btn-color:${color}` : '',
      iconColor ? `--btn-icon-color:${iconColor}` : '',
    ].filter(Boolean).join(';') || undefined,
  );

  /** Unavailable, for either reason. */
  const inert = $derived(disabled || loading);

  /** Is there actually something to say if the user asks why it is greyed out? */
  const hasExplanation = $derived.by(() => {
    if (typeof tipInput === 'string') return tipInput.trim().length > 0;
    if (!tipInput || tipInput.disabled) return false;
    return typeof tipInput.content === 'string' && tipInput.content.trim().length > 0;
  });

  /**
   * A greyed-out button that can explain itself uses `aria-disabled` instead of
   * the native attribute — and that is the whole point of this component.
   *
   * A `disabled` control dispatches **no** mouse events and cannot be focused, so
   * its tooltip is unreachable by mouse *and* by keyboard: every "why is this
   * greyed out?" in Arbor was silently dead. `aria-disabled` keeps the button in
   * the accessibility tree and in the tab order, which is also what WAI-ARIA
   * recommends for exactly this reason, so the explanation can be read by hovering
   * **or** by tabbing to it — the keyboard-first rule applies to finding out why
   * you cannot do something, not only to doing it.
   *
   * Inertness is then enforced here rather than by the browser: the click handler
   * refuses and stops propagation (so delegated handlers upstream see nothing),
   * and `type` drops to `button` so an inert submit cannot submit its form.
   *
   * With no tooltip there is nothing to reach, so the native attribute stays — it
   * is stricter, and strictness is free when it costs no information.
   */
  const ariaInert = $derived(inert && hasExplanation);

  function handleClick(e: MouseEvent) {
    if (inert) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    onclick?.(e);
  }
</script>

<button
  bind:this={element}
  type={inert ? 'button' : type}
  use:tooltipAction={tipInput}
  aria-label={ariaLabel}
  aria-expanded={ariaExpanded}
  aria-haspopup={ariaHaspopup}
  aria-busy={loading || undefined}
  aria-disabled={inert || undefined}
  disabled={inert && !ariaInert}
  class="btn btn-{variant} sz-{size}"
  class:block
  class:has-color={!!color}
  class:has-icon-color={!!iconColor}
  style={styleVars}
  onclick={handleClick}
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
  /* Same guard as the variant rules in app.css: `aria-disabled` buttons are not
     `:disabled`, so the hover affordance has to be excluded explicitly. */
  .btn-primary.has-color:hover:not(:disabled):not([aria-disabled='true']) { filter: brightness(1.12); }

  .btn-ghost.has-color,
  .btn-icon.has-color  { color: var(--btn-color); }

  /* Tonal — soft translucent fill in the (overridable) accent colour, with
     matching text + border. Used for low-emphasis-but-coloured actions (e.g.
     the launcher's per-product Avvia / Apri / Stop). */
  .btn-tonal.has-color {
    background: color-mix(in srgb, var(--btn-color) 15%, transparent);
    color: var(--btn-color);
    border-color: color-mix(in srgb, var(--btn-color) 32%, transparent);
  }
  .btn-tonal.has-color:hover:not(:disabled):not([aria-disabled='true']) { filter: brightness(1.12); }

  .btn-danger.has-color { color: var(--btn-color); }
  .btn-danger.has-color:hover:not(:disabled):not([aria-disabled='true']) {
    background: color-mix(in srgb, var(--btn-color) 15%, transparent);
    border-color: var(--btn-color);
  }

  /* Outline + colour = the "destructive but not shouting" button: neutral at
     rest, the colour appears only under the pointer. `Disconnect` on a
     connected provider is the canonical use. */
  .btn-outline.has-color:hover:not(:disabled):not([aria-disabled='true']) {
    background: color-mix(in srgb, var(--btn-color) 15%, transparent);
    color: var(--btn-color);
    border-color: var(--btn-color);
  }

  /* ---- Icon colour (--btn-icon-color) ----
     Lucide draws with `stroke="currentColor"`, so colouring the svg is enough and
     the label keeps the button's own text colour. */
  .btn.has-icon-color :global(svg) { color: var(--btn-icon-color); }
  /* Unavailable wins. Falling back to `inherit` hands the glyph back to whatever
     the button is currently painted in — which for a greyed-out one is grey, and
     for a hovered one is the hover colour. */
  .btn.has-icon-color:disabled :global(svg),
  .btn.has-icon-color[aria-disabled='true'] :global(svg) { color: inherit; }

  /* ---- Loading ---- */
  :global(.btn-spin) { animation: btn-spin-anim 1s linear infinite; }
  @keyframes btn-spin-anim {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
