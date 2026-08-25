<script lang="ts" module>
  /**
   * `value + by`, clamped to `[min, max]` and rounded — the one copy of the
   * arithmetic, exported because the buttons are not the only thing that zooms.
   *
   * A host that also zooms on the wheel (a graph canvas, a diagram) has to clamp the
   * same way or the two disagree at the ends of the range, which is precisely how
   * both existing call sites had ended up with the expression written out by hand.
   *
   * Rounded to two decimals on purpose: repeated `+= 0.1` on a float drifts, and a
   * level that reads `70.00000000000001%` is a bug report.
   */
  export function clampZoom(
    value: number,
    by: number,
    range: { min: number; max: number },
  ): number {
    return Math.min(range.max, Math.max(range.min, Number((value + by).toFixed(2))));
  }
</script>

<script lang="ts">
  /**
   * The zoom cluster: out, the current level, in — and optionally "fit".
   *
   * A segmented strip rather than three loose buttons, because the three are one
   * control and read as one. The level in the middle is itself a button: clicking it
   * goes back to 100%, which is where everybody's hand goes first and what a plain
   * label wastes.
   *
   * ## Why it is a widget
   *
   * It was written twice — Bennu's module graph and Picus's plan diagram — as raw
   * `<button class="…">` with the clamping arithmetic copied alongside:
   * `Math.min(MAX, Math.max(MIN, +(zoom + by).toFixed(2)))`. That expression is the
   * whole reason this exists: it is easy to write, easy to write *slightly*
   * differently, and the difference only shows at the ends of the range where nobody
   * looks. Here it lives once, in {@link nudge}, and both the buttons and a host's own
   * wheel handler go through it.
   *
   * `shared/ui` and app-agnostic by construction: a number, a range, and callbacks.
   * It knows nothing about what is being zoomed.
   */
  import { Minus, Plus, Maximize } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Current zoom, where `1` is 100%. */
    value: number;
    onChange: (next: number) => void;
    min?: number;
    max?: number;
    /** How much one press moves it. */
    step?: number;
    /** Where the level button goes back to. */
    resetTo?: number;
    /**
     * Show a "fit" button after the level. Omit for a host that has nothing to fit
     * to — the plan diagram scrolls rather than fitting, so it does not offer one.
     */
    onFit?: () => void;
    fitLabel?: string;
    ariaLabel?: string;
  }

  let {
    value,
    onChange,
    min = 0.4,
    max = 2,
    step = 0.2,
    resetTo = 1,
    onFit,
    fitLabel = 'Fit',
    ariaLabel = 'Zoom',
  }: Props = $props();

  const nudge = (by: number) => clampZoom(value, by, { min, max });

  const atMin = $derived(value <= min);
  const atMax = $derived(value >= max);
</script>

<span class="zc" role="group" aria-label={ariaLabel}>
  <button
    type="button"
    aria-label="Zoom out"
    disabled={atMin}
    use:tooltip={'Zoom out'}
    onclick={() => onChange(nudge(-step))}
  >
    <Minus size={12} />
  </button>
  <button
    type="button"
    class="zc-level"
    use:tooltip={`Back to ${Math.round(resetTo * 100)}%`}
    onclick={() => onChange(resetTo)}
  >
    {Math.round(value * 100)}%
  </button>
  <button
    type="button"
    aria-label="Zoom in"
    disabled={atMax}
    use:tooltip={'Zoom in'}
    onclick={() => onChange(nudge(step))}
  >
    <Plus size={12} />
  </button>
  {#if onFit}
    <button type="button" aria-label={fitLabel} use:tooltip={fitLabel} onclick={onFit}>
      <Maximize size={11} />
    </button>
  {/if}
</span>

<style>
  .zc {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    overflow: hidden;
    flex-shrink: 0;
  }
  .zc button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 20px;
    padding: 0 5px;
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
  }
  .zc button:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  /* An end of the range is not an error, so it goes quiet rather than being removed —
     the strip keeps its width and the two arrows stay where the hand left them. */
  .zc button:disabled { color: var(--text-disabled); cursor: default; }
  .zc-level {
    min-width: 40px;
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
    font-variant-numeric: tabular-nums;
  }
</style>
