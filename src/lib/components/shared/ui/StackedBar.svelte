<script lang="ts">
  /**
   * A bar divided into parts of one whole — what a usage meter shows when "how full" is not enough
   * and "full of what" is the point (a context window: system, files, conversation).
   *
   * Parts are drawn in order from the left, each as wide as its share of `max`; whatever the parts do
   * not add up to stays as empty track, so the bar reads as a fraction of the whole and not as a
   * proportion of itself. `ProgressBar` is the single-value cousin; this is for when there are
   * several.
   */
  export interface BarSegment {
    id: string;
    /** Spoken by assistive tech, and the part's name wherever the caller draws a legend. */
    label: string;
    value: number;
    /** A semantic tone, or a CSS colour for a part that has its own (`color`). */
    tone?: 'accent' | 'info' | 'success' | 'warning' | 'danger' | 'muted';
    color?: string;
  }

  let {
    segments,
    max,
    height = 6,
    ariaLabel,
  }: {
    segments: readonly BarSegment[];
    /** What the whole bar stands for. A part past it is clipped. */
    max: number;
    height?: number;
    ariaLabel?: string;
  } = $props();

  const TONE: Record<NonNullable<BarSegment['tone']>, string> = {
    accent: 'var(--accent)',
    info: 'var(--info)',
    success: 'var(--success)',
    warning: 'var(--warning)',
    danger: 'var(--error)',
    muted: 'var(--text-muted)',
  };

  const colourOf = (s: BarSegment) => s.color ?? TONE[s.tone ?? 'accent'];
  const widthOf = (s: BarSegment) => (max > 0 ? Math.max(0, Math.min(100, (s.value / max) * 100)) : 0);
  const total = $derived(segments.reduce((sum, s) => sum + s.value, 0));
</script>

<div
  class="sb"
  style:height="{height}px"
  role="meter"
  aria-label={ariaLabel}
  aria-valuemin={0}
  aria-valuemax={max}
  aria-valuenow={Math.min(total, max)}
>
  {#each segments as s (s.id)}
    {#if s.value > 0}
      <span class="sb-seg" style:width="{widthOf(s)}%" style:background={colourOf(s)} title={s.label}></span>
    {/if}
  {/each}
</div>

<style>
  .sb {
    display: flex;
    width: 100%;
    gap: 1px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--bg-overlay);
  }
  .sb-seg {
    flex: none;
    min-width: 2px;
    transition: width var(--transition-base, 200ms) ease;
  }
  /* The ends of the filled run follow the track's rounding; the joints between parts stay square. */
  .sb-seg:first-child { border-radius: 999px 0 0 999px; }
  .sb-seg:last-child  { border-radius: 0 999px 999px 0; }
  .sb-seg:only-child  { border-radius: 999px; }
</style>
