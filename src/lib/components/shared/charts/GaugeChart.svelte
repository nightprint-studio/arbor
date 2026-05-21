<script lang="ts">
  /**
   * Semi-circle gauge — generic, props-driven, zero domain coupling.
   *
   * Render order:
   *   1. background arc (full half-circle, muted)
   *   2. coloured `segments` (each `{ from, to, color }` in domain space)
   *   3. needle ROTATED to the interpolated value
   *   4. centred numeric label + sub-label
   *
   * The first consumer is `<RiskScoreGauge>` for the security dashboard,
   * but this widget knows nothing about severity / risk — pass any
   * domain-specific colour stops via `segments`.
   */
  import { arcPath } from './chart-utils';

  export interface GaugeSegment {
    from:  number;
    to:    number;
    color: string;
  }

  interface Props {
    value:        number;
    min?:         number;            // default 0
    max?:         number;            // default 100
    segments?:    GaugeSegment[];
    label?:       string;            // sub-label under the numeric value
    size?:        'sm' | 'md' | 'lg';
    formatValue?: (v: number) => string;
    /** Override the needle / value text colour. Defaults to current segment colour at `value`. */
    valueColor?:  string;
  }

  let {
    value,
    min = 0,
    max = 100,
    segments = [],
    label,
    size = 'md',
    formatValue,
    valueColor,
  }: Props = $props();

  const SIZES = {
    sm: { w: 120, h: 78,  stroke: 10, valueFs: 18, labelFs: 10 },
    md: { w: 200, h: 120, stroke: 14, valueFs: 30, labelFs: 12 },
    lg: { w: 280, h: 160, stroke: 18, valueFs: 40, labelFs: 13 },
  };
  const dims = $derived(SIZES[size]);

  // Pre-compute geometry. The arc spans 180° (π) starting at the LEFT
  // (angle = π) and ending at the RIGHT (angle = 2π = 0 mod 2π).
  const cx = $derived(dims.w / 2);
  const cy = $derived(dims.h - 6);                    // baseline padding
  const radius = $derived((dims.w / 2) - dims.stroke);

  const START_RAD = Math.PI;                          // 180°
  const END_RAD   = 2 * Math.PI;                      // 360°
  const SWEEP     = END_RAD - START_RAD;              // π

  function valueToRad(v: number): number {
    const span = max - min;
    if (span === 0) return START_RAD;
    const clamped = Math.max(min, Math.min(max, v));
    return START_RAD + ((clamped - min) / span) * SWEEP;
  }

  const bgArc = $derived(arcPath(cx, cy, radius, START_RAD, END_RAD));

  const segmentArcs = $derived.by(() => {
    return segments.map((seg) => ({
      d: arcPath(cx, cy, radius, valueToRad(seg.from), valueToRad(seg.to)),
      color: seg.color,
    }));
  });

  // Needle endpoint
  const needleAngle = $derived(valueToRad(value));
  const needleLen   = $derived(radius - dims.stroke / 2 - 4);
  const needleEnd   = $derived({
    x: cx + Math.cos(needleAngle) * needleLen,
    y: cy + Math.sin(needleAngle) * needleLen,
  });

  // Pick a colour for the value text. Uses the segment containing `value`
  // when available, otherwise the explicit `valueColor` prop, otherwise
  // a sensible default.
  const computedValueColor = $derived.by(() => {
    if (valueColor) return valueColor;
    const seg = segments.find((s) => value >= s.from && value <= s.to);
    return seg?.color ?? 'var(--text-primary)';
  });

  const formattedValue = $derived(
    formatValue ? formatValue(value) : String(Math.round(value)),
  );
</script>

<div class="gauge-chart" class:sm={size === 'sm'} class:lg={size === 'lg'}>
  <svg
    width={dims.w}
    height={dims.h}
    viewBox="0 0 {dims.w} {dims.h}"
    role="img"
    aria-label={label ? `${formattedValue} — ${label}` : formattedValue}
  >
    <!-- background arc -->
    <path
      d={bgArc}
      stroke="var(--bg-elevated)"
      stroke-width={dims.stroke}
      stroke-linecap="round"
      fill="none"
    />

    <!-- coloured segments -->
    {#each segmentArcs as arc, i (i)}
      <path
        d={arc.d}
        stroke={arc.color}
        stroke-width={dims.stroke}
        stroke-linecap="butt"
        fill="none"
      />
    {/each}

    <!-- needle -->
    <line
      x1={cx} y1={cy}
      x2={needleEnd.x} y2={needleEnd.y}
      stroke={computedValueColor}
      stroke-width={Math.max(2, Math.round(dims.stroke / 4))}
      stroke-linecap="round"
    />
    <circle cx={cx} cy={cy} r={Math.max(3, dims.stroke / 3)}
      fill={computedValueColor} stroke="var(--bg-base)" stroke-width="1.5"/>
  </svg>

  <div class="gauge-readout">
    <div class="gauge-value" style:color={computedValueColor} style:font-size="{dims.valueFs}px">
      {formattedValue}
    </div>
    {#if label}
      <div class="gauge-label" style:font-size="{dims.labelFs}px">{label}</div>
    {/if}
  </div>
</div>

<style>
  .gauge-chart {
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    font-family: var(--font-ui-sans);
  }
  .gauge-chart svg { display: block; }
  .gauge-readout {
    margin-top: -6px;
    text-align: center;
  }
  .gauge-value {
    font-weight: 700;
    line-height: 1.1;
    font-variant-numeric: tabular-nums;
  }
  .gauge-label {
    margin-top: 2px;
    color: var(--text-secondary);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .gauge-chart.sm .gauge-readout { margin-top: -10px; }
  .gauge-chart.lg .gauge-readout { margin-top: -4px; }
</style>
