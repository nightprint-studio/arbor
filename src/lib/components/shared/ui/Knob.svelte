<script lang="ts">
  /**
   * Knob — a rotary control (DAW-style). Click-and-drag vertically to turn it
   * (up = increase); double-click resets to `default`. Renders an arc track + a
   * value arc + a pointer indicator. `bipolar` fills the value arc from the
   * 12-o'clock centre (for pan-style −/+ controls) instead of from the start.
   *
   * Generic + app-agnostic (no Arbor concepts) → lives in shared/ui/.
   */
  let {
    value = $bindable(0),
    min = 0,
    max = 1,
    default: dflt,
    size = 34,
    color = 'var(--accent)',
    bipolar = false,
    disabled = false,
    label,
    ariaLabel,
    onchange,
  }: {
    value?: number;
    min?: number;
    max?: number;
    /** Reset target on double-click. Defaults to the midpoint. */
    default?: number;
    size?: number;
    color?: string;
    /** Fill the arc from the centre (pan) rather than from the minimum. */
    bipolar?: boolean;
    disabled?: boolean;
    label?: string;
    ariaLabel?: string;
    onchange?: (v: number) => void;
  } = $props();

  const SWEEP = 270;            // total travel in degrees
  const A0 = -135;             // angle at `min` (0° = 12 o'clock, clockwise +)
  const resetTo = $derived(dflt ?? (min + max) / 2);

  const norm = $derived((value - min) / (max - min || 1));      // 0..1
  const valAngle = $derived(A0 + norm * SWEEP);
  const fillFrom = $derived(bipolar ? A0 + 0.5 * SWEEP : A0);    // centre or min

  const R = 38, CX = 50, CY = 50;
  function pt(deg: number) {
    const rad = (deg * Math.PI) / 180;
    return { x: CX + R * Math.sin(rad), y: CY - R * Math.cos(rad) };
  }
  function arc(a0: number, a1: number) {
    const s = pt(a0), e = pt(a1);
    const large = Math.abs(a1 - a0) > 180 ? 1 : 0;
    const sweep = a1 >= a0 ? 1 : 0;
    return `M${s.x.toFixed(2)},${s.y.toFixed(2)} A${R},${R} 0 ${large} ${sweep} ${e.x.toFixed(2)},${e.y.toFixed(2)}`;
  }
  const trackPath = $derived(arc(A0, A0 + SWEEP));
  const valuePath = $derived(arc(fillFrom, valAngle));
  const indicator = $derived(pt(valAngle));

  // ── Drag ─────────────────────────────────────────────────────────────────
  function onPointerDown(e: PointerEvent) {
    if (disabled) return;
    e.preventDefault();
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
    const startY = e.clientY;
    const startVal = value;
    const span = max - min;
    // Fine mode with Shift; ~180px of travel covers the full range.
    function move(ev: PointerEvent) {
      const sens = (ev.shiftKey ? 0.25 : 1) * span / 180;
      const next = Math.max(min, Math.min(max, startVal - (ev.clientY - startY) * sens));
      if (next !== value) { value = next; onchange?.(next); }
    }
    function up(ev: PointerEvent) {
      (e.target as HTMLElement).releasePointerCapture?.(ev.pointerId);
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    }
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }
  function reset() {
    if (disabled) return;
    value = resetTo; onchange?.(resetTo);
  }
  function onKey(e: KeyboardEvent) {
    if (disabled) return;
    const span = max - min;
    const step = (e.shiftKey ? 0.01 : 0.05) * span;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') { e.preventDefault(); value = Math.min(max, value + step); onchange?.(value); }
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') { e.preventDefault(); value = Math.max(min, value - step); onchange?.(value); }
  }
</script>

<div class="knob-wrap" class:disabled style="--knob-size: {size}px; --knob-color: {color}">
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="knob"
    role="slider"
    tabindex={disabled ? -1 : 0}
    aria-label={ariaLabel ?? label}
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={value}
    aria-disabled={disabled || undefined}
    onpointerdown={onPointerDown}
    ondblclick={reset}
    onkeydown={onKey}
  >
    <svg viewBox="0 0 100 100" aria-hidden="true">
      <path class="k-track" d={trackPath} />
      <path class="k-value" d={valuePath} />
      <circle class="k-cap" cx={CX} cy={CY} r="26" />
      <line class="k-ind" x1={CX} y1={CY} x2={indicator.x} y2={indicator.y} />
    </svg>
  </div>
  {#if label}<span class="knob-label">{label}</span>{/if}
</div>

<style>
  .knob-wrap { display: inline-flex; flex-direction: column; align-items: center; gap: 2px; }
  .knob-wrap.disabled { opacity: 0.45; }

  .knob {
    width: var(--knob-size); height: var(--knob-size);
    cursor: ns-resize; outline: none; touch-action: none;
    border-radius: 50%;
  }
  .knob:focus-visible { box-shadow: 0 0 0 2px var(--accent-subtle), 0 0 0 3px var(--accent); }
  .knob svg { width: 100%; height: 100%; display: block; }

  .k-track {
    fill: none;
    stroke: var(--border-subtle);
    stroke-width: 8;
    stroke-linecap: round;
  }
  .k-value {
    fill: none;
    stroke: var(--knob-color);
    stroke-width: 8;
    stroke-linecap: round;
  }
  .k-cap {
    fill: var(--bg-elevated);
    stroke: var(--border-subtle);
    stroke-width: 1.5;
  }
  .k-ind {
    stroke: var(--text-primary);
    stroke-width: 4;
    stroke-linecap: round;
  }

  .knob-label {
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.3px;
    color: var(--text-muted); user-select: none;
  }
</style>
