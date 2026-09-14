<script lang="ts">
  /**
   * StatusBarItem — one reading on an IntelliJ-style status strip (a window footer).
   *
   * Two shapes, one look: without `onclick` it is plain information (a `<span>`); with it, the
   * same text becomes a real, focusable `<button>`. Styled as text either way — the strip is
   * information, and button chrome there would compete with the editor for attention — so an
   * item can turn actionable (a warning that offers a fix) without the row visibly changing.
   *
   * `tone` is the only colour decision, and it colours the icon with the label:
   *   default — ordinary reading, icon dimmed;
   *   muted   — an absence ("JDK —", "No project open");
   *   accent  — work in progress (indexing, a server starting);
   *   warning — something wrong rather than informative (a server down, a failed build).
   *
   * Icons are passed as children; the leading lucide glyph is tinted through `currentColor`.
   * Pair with `StatusBarSeparator` between groups.
   */
  import type { Snippet } from 'svelte';
  import { tooltip as tooltipAction } from '$lib/actions/tooltip';
  import type { TooltipInput } from '$lib/stores/tooltip.svelte';

  type Tone = 'default' | 'muted' | 'accent' | 'warning';

  interface Props {
    tone?: Tone;
    tooltip?: TooltipInput;
    /** Makes the item actionable (rendered as a button). */
    onclick?: (e: MouseEvent) => void;
    /** Caps the width in `ch` and ellipsises — for free text a strip of one row cannot trust,
     *  such as a language server's progress message. */
    maxWidth?: number;
    /** Accessible name for an actionable item whose label alone is not enough. */
    ariaLabel?: string;
    /** Held-down look — an item whose menu is open (a `Dropdown` trigger). */
    active?: boolean;
    /** ARIA for an item that opens a menu. */
    ariaHaspopup?: 'menu' | 'listbox' | true;
    ariaExpanded?: boolean;
    children: Snippet;
  }

  let {
    tone = 'default',
    tooltip = '',
    onclick,
    maxWidth,
    ariaLabel,
    active = false,
    ariaHaspopup,
    ariaExpanded,
    children,
  }: Props = $props();

  const style = $derived(maxWidth ? `max-width:${maxWidth}ch` : undefined);
</script>

{#if onclick}
  <button
    type="button"
    class="sbi tone-{tone} actionable"
    class:truncate={!!maxWidth}
    class:active
    {style}
    aria-label={ariaLabel}
    aria-haspopup={ariaHaspopup}
    aria-expanded={ariaExpanded}
    use:tooltipAction={tooltip}
    {onclick}
  >
    {@render children()}
  </button>
{:else}
  <span class="sbi tone-{tone}" class:truncate={!!maxWidth} {style} use:tooltipAction={tooltip}>
    {@render children()}
  </span>
{/if}

<style>
  .sbi {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    --sbi-icon: var(--text-disabled);
  }
  /* The glyph is the consumer's child, so it is only reachable globally — scoped to this item. */
  .sbi :global(svg) { color: var(--sbi-icon); flex-shrink: 0; }

  .tone-muted { color: var(--text-disabled); }
  .tone-accent { color: var(--accent); --sbi-icon: var(--accent); }
  .tone-warning { color: var(--warning); --sbi-icon: var(--warning); }

  .actionable {
    background: none;
    border: none;
    padding: 0;
    margin: 0;
    font: inherit;
    font-size: var(--font-size-xs);
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: color var(--transition-fast);
  }
  .actionable:focus-visible { outline: 1px solid var(--accent); outline-offset: 2px; }
  .actionable.tone-default:hover,
  .actionable.tone-muted:hover { color: var(--text-primary); --sbi-icon: var(--text-secondary); }
  .actionable.tone-accent:hover,
  .actionable.tone-warning:hover { filter: brightness(1.15); }
  /* A menu trigger reads as held down while its menu is up. The padding only appears with the
     fill, so a plain item stays flush with the text around it. */
  .actionable.active {
    background: var(--bg-hover);
    color: var(--text-secondary);
    --sbi-icon: var(--text-secondary);
    padding: 2px 6px;
    margin: -2px -6px;
  }

  /* `min-width: 0` is what lets a flex child shrink below its content at all. */
  .truncate { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
</style>
