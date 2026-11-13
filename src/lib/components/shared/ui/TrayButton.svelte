<script lang="ts">
  /**
   * TrayButton — one segment of a status strip: an icon, a word, a count, and optionally a line of detail that
   * takes the room left ("Plan 3/6 · Running the tests").
   *
   * Flat, so several sit side by side in one strip and read as one object rather than a row of pills; the strip
   * draws the frame and the dividers. It is a real button that opens a panel, and holds a pressed look while the
   * panel is open. `tone` lights the icon and the count when the thing has work in it — `accent` for something
   * going on, `warning` for something waiting on you, `success` for something finished — and `neutral` leaves
   * it quiet. The icon is a snippet so the caller can draw a spinner or an icon of its own.
   */
  import type { Snippet } from 'svelte';
  import { tooltip as tooltipAction } from '$lib/actions/tooltip';
  import type { TooltipInput } from '$lib/stores/tooltip.svelte';

  let {
    label,
    count,
    detail = '',
    grow = false,
    tone = 'neutral',
    pressed = false,
    tooltip = '',
    onclick,
    icon,
  }: {
    label: string;
    /** Shown in its own bubble after the label; a string for "2/7". */
    count?: string | number;
    /** A line after the count, cut with an ellipsis when it does not fit. */
    detail?: string;
    /** Take the room the other segments leave. */
    grow?: boolean;
    tone?: 'neutral' | 'accent' | 'warning' | 'success';
    /** The panel it opens is open. */
    pressed?: boolean;
    tooltip?: TooltipInput;
    onclick: () => void;
    icon?: Snippet;
  } = $props();
</script>

<button
  type="button"
  class="tray-btn tone-{tone}"
  class:pressed
  class:grow
  aria-pressed={pressed}
  aria-expanded={pressed}
  use:tooltipAction={tooltip}
  {onclick}
>
  {#if icon}<span class="ico">{@render icon()}</span>{/if}
  <span class="lbl">{label}</span>
  {#if count !== undefined && count !== ''}<span class="count">{count}</span>{/if}
  {#if detail}<span class="detail">{detail}</span>{/if}
</button>

<style>
  .tray-btn {
    display: inline-flex; align-items: center; gap: 7px; min-width: 0;
    height: 30px; padding: 0 11px;
    border: none; background: transparent;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans); font-size: var(--font-size-xs); font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .tray-btn.grow { flex: 1; }
  .tray-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tray-btn:focus-visible { outline: none; box-shadow: inset 0 0 0 2px var(--accent); }

  .ico { display: inline-flex; align-items: center; flex-shrink: 0; color: var(--text-muted); }
  .lbl { flex-shrink: 0; }
  .count {
    flex-shrink: 0;
    font-size: var(--font-size-2xs); font-weight: 600; font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }
  .detail {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; text-align: left;
    padding-left: 8px; border-left: 1px solid var(--border-subtle);
    color: var(--text-secondary); font-weight: 400;
  }

  .tone-accent .ico, .tone-accent .count { color: var(--accent); }
  .tone-warning .ico, .tone-warning .count { color: var(--warning); }
  .tone-success .ico, .tone-success .count { color: var(--success); }

  .pressed { background: var(--accent-subtle); color: var(--text-primary); }
  .pressed:hover { background: var(--accent-subtle); }
</style>
