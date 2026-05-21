<script lang="ts">
  /**
   * Generic counter-card grid — KPI tiles laid out responsively.
   *
   * Each tile shows a large primary value, an upper-case label coloured by
   * the tile's accent, and an optional muted sub-line (delta, age, units…).
   * Empty tiles (`empty: true` or `value === 0`) render dimmed and ignore
   * clicks.
   *
   * Domain coupling: zero. The first consumer is `<SeverityCounterGrid>`;
   * any plugin can render `kind = 'counter_grid'` form-nodes against the
   * same widget.
   */
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';

  export interface CounterItem {
    /** Stable identifier; surfaced to `onSelect`. */
    key:    string;
    /** Header label (typically rendered in upper-case). */
    label:  string;
    /** Big primary value. Numbers render as-is; strings pass through. */
    value:  number | string;
    /** Optional muted sub-line under the value (e.g. delta, median age). */
    hint?:  string;
    /** Accent colour — CSS expression (`var(--severity-high)`, `#f97316`). */
    color?: string;
    /** Optional lucide icon name (curated subset — see PLUGIN_ICONS). */
    icon?:  string;
    /** When true (or when `value` is 0), the tile renders dimmed and is unclickable. */
    empty?: boolean;
  }

  interface Props {
    items:    CounterItem[];
    /** CSS `minmax(N, 1fr)` minimum tile width. Default 120. */
    minWidth?: number;
    /** Grid gap in px. Default 8. */
    gap?:     number;
    /** Outer padding (CSS). Default '12px'. */
    padding?: string;
    onSelect?: (key: string) => void;
  }

  let {
    items,
    minWidth = 120,
    gap      = 8,
    padding  = '12px',
    onSelect,
  }: Props = $props();

  function isEmpty(it: CounterItem): boolean {
    if (it.empty) return true;
    return typeof it.value === 'number' && it.value === 0;
  }
</script>

<div
  class="cg-grid"
  style:--cg-min={`${minWidth}px`}
  style:--cg-gap={`${gap}px`}
  style:padding
>
  {#each items as it (it.key)}
    {@const Icon  = it.icon ? PLUGIN_ICONS[it.icon] : null}
    {@const empty = isEmpty(it)}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <button
      class="cg-card"
      class:empty
      style:--cg-color={it.color ?? 'var(--accent)'}
      type="button"
      disabled={empty || !onSelect}
      onclick={() => { if (!empty) onSelect?.(it.key); }}
      aria-label={`${it.label}: ${it.value}`}
    >
      <span class="cg-label">
        {#if Icon}<Icon size={11} />{/if}
        <span>{it.label}</span>
      </span>
      <span class="cg-value">{it.value}</span>
      <span class="cg-hint">{empty ? '—' : (it.hint ?? '')}</span>
    </button>
  {/each}
</div>

<style>
  .cg-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--cg-min), 1fr));
    gap: var(--cg-gap);
  }

  .cg-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 12px 14px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-left: 3px solid var(--cg-color);
    border-radius: var(--radius-md);
    cursor: pointer;
    text-align: left;
    font-family: var(--font-ui-sans);
    transition:
      background var(--transition-fast),
      border-color var(--transition-fast),
      transform var(--transition-fast);
  }
  .cg-card:hover:not(.empty):not(:disabled) {
    background: color-mix(in srgb, var(--cg-color) 12%, transparent);
    border-color: var(--cg-color);
    transform: translateY(-1px);
  }
  .cg-card.empty,
  .cg-card:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .cg-card.empty:hover,
  .cg-card:disabled:hover {
    transform: none;
  }

  .cg-label {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--cg-color);
  }
  .cg-value {
    font-size: 22px;
    font-weight: 700;
    line-height: 1;
    color: var(--text-primary);
  }
  .cg-hint {
    font-size: 10px;
    color: var(--text-muted);
    margin-top: 2px;
    min-height: 1em;
  }
</style>
