<script lang="ts">
  /**
   * Generic linear progress bar. App-agnostic — drives off either a raw
   * `pct` (0–100) or a `value`/`max` pair. Pass `indeterminate` for an
   * animated sweep when no total is known yet.
   */
  let {
    value,
    max = 100,
    pct,
    indeterminate = false,
    height = 4,
    color,
    ariaLabel,
  }: {
    value?: number;
    max?: number;
    pct?: number;
    indeterminate?: boolean;
    height?: number;
    /** Fill colour (any CSS colour) — for a bar whose colour carries meaning, such as a severity. Defaults to the accent. */
    color?: string;
    ariaLabel?: string;
  } = $props();

  const resolved = $derived(
    pct !== undefined
      ? pct
      : value !== undefined && max > 0
        ? (value / max) * 100
        : 0,
  );
  const clamped = $derived(Math.max(0, Math.min(100, resolved)));
</script>

<div
  class="progress-track"
  style="height: {height}px"
  role="progressbar"
  aria-label={ariaLabel}
  aria-valuemin={indeterminate ? undefined : 0}
  aria-valuemax={indeterminate ? undefined : 100}
  aria-valuenow={indeterminate ? undefined : Math.round(clamped)}
>
  <div
    class="progress-fill"
    class:indeterminate
    style={indeterminate ? '' : `width: ${clamped}%${color ? `; background: ${color}` : ''}`}
  ></div>
</div>

<style>
  .progress-track {
    width: 100%;
    /* Visible neutral so the empty remainder reads as the bar's full extent
       (its "end delimiter") on any background, dark panels included. */
    background: var(--border);
    border-radius: 999px;
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 999px;
    transition: width 120ms ease;
  }
  .progress-fill.indeterminate {
    width: 35%;
    animation: progress-sweep 1.1s ease-in-out infinite;
  }
  @keyframes progress-sweep {
    0%   { margin-left: -35%; }
    100% { margin-left: 100%; }
  }
</style>
