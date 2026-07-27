<script lang="ts">
  /**
   * ColorPalettePicker — pick one slot out of a fixed identity palette.
   *
   * Not a colour *editor* (that's `ColorSwatch` with `onchange`, which overlays
   * a native picker): this is the "choose which of the N house colours this
   * thing wears" control — workspaces, groups, database connections. The caller
   * owns the palette by handing over a list of CSS colour values, so the theme
   * keeps deciding what those colours actually are.
   *
   * Keyboard: the swatches are ordinary buttons in a grid, so Tab reaches them
   * and Space/Enter picks — no arrow-key trap, no mouse requirement.
   *
   * NOTE (shared/ui contract): no Arbor concepts, no stores — colours in,
   * selected index out.
   */
  import { Check } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Any CSS colour values — typically `var(--ws-color-N)` references. */
    colors: string[];
    /** Index of the selected colour. */
    value: number;
    onChange: (index: number) => void;
    /** Swatches per row. Defaults to the palette length (a single row). */
    columns?: number;
    /** Swatch height in px. */
    size?: number;
    /** Tooltip/aria wording: `${labelPrefix} ${n}`. */
    labelPrefix?: string;
    ariaLabel?: string;
  }

  let {
    colors,
    value,
    onChange,
    columns,
    size = 26,
    labelPrefix = 'Colour',
    ariaLabel = 'Colour',
  }: Props = $props();

  const cols = $derived(columns ?? colors.length);
</script>

<div
  class="cpp"
  role="radiogroup"
  aria-label={ariaLabel}
  style:grid-template-columns={`repeat(${cols}, 1fr)`}
>
  {#each colors as color, i (i)}
    <button
      type="button"
      class="cpp-swatch"
      class:cpp-selected={value === i}
      style:background={color}
      style:height={`${size}px`}
      role="radio"
      aria-checked={value === i}
      aria-label={`${labelPrefix} ${i + 1}`}
      use:tooltip={`${labelPrefix} ${i + 1}`}
      onclick={() => onChange(i)}
    >
      {#if value === i}<Check size={Math.round(size * 0.46)} />{/if}
    </button>
  {/each}
</div>

<style>
  .cpp {
    display: grid;
    gap: 6px;
  }
  .cpp-swatch {
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid color-mix(in srgb, currentColor 30%, transparent);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--ws-color-fg);
    transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  }
  .cpp-swatch:hover { transform: scale(1.08); }
  .cpp-swatch.cpp-selected {
    box-shadow: 0 0 0 2px var(--bg-elevated), 0 0 0 4px currentColor;
  }
  .cpp-swatch:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--bg-elevated), 0 0 0 4px var(--border-focus);
  }
</style>
