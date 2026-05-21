<script lang="ts">
  /**
   * Generic legend for SVG charts. Renders one chip per series and
   * (optionally) toggles their visibility via a `Set<string>` of hidden
   * series ids passed back via the `onToggle` callback.
   *
   * The chip itself is stateless — the parent owns `hidden` and decides
   * how to act on toggles (hide/show, dim, etc).
   */

  interface LegendEntry {
    id:    string;
    label: string;
    color: string;
  }

  interface Props {
    entries:     LegendEntry[];
    /** Set of hidden series ids. If omitted, the legend is informational only. */
    hidden?:     Set<string>;
    /** Called when a chip is clicked. Receives the series id. */
    onToggle?:   (id: string) => void;
    /** Stack vertically rather than the default horizontal flow. */
    vertical?:   boolean;
  }

  import { tooltip } from '$lib/actions/tooltip';

  let { entries, hidden, onToggle, vertical = false }: Props = $props();

  const interactive = $derived(typeof onToggle === 'function');
</script>

<ul class="chart-legend" class:vertical>
  {#each entries as e (e.id)}
    {@const isHidden = hidden?.has(e.id) ?? false}
    <li>
      <button
        type="button"
        class="legend-chip"
        class:hidden={isHidden}
        class:interactive
        disabled={!interactive}
        onclick={() => onToggle?.(e.id)}
        use:tooltip={interactive ? (isHidden ? `Show ${e.label}` : `Hide ${e.label}`) : e.label}
      >
        <span class="chip-swatch" style:background={e.color}></span>
        <span class="chip-label">{e.label}</span>
      </button>
    </li>
  {/each}
</ul>

<style>
  .chart-legend {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    font-family: var(--font-ui-sans);
  }
  .chart-legend.vertical { flex-direction: column; gap: 4px; }
  .chart-legend li { display: contents; }

  .legend-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 11px;
    cursor: default;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .legend-chip.interactive { cursor: pointer; }
  .legend-chip.interactive:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .legend-chip.hidden {
    color: var(--text-disabled);
  }
  .legend-chip.hidden .chip-swatch {
    background: var(--text-disabled) !important;
    opacity: 0.6;
  }

  .chip-swatch {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    flex-shrink: 0;
  }
</style>
