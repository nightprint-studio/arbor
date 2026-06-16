<script lang="ts">
  /**
   * Fader — a vertical channel-strip level control (DAW mixer style). Drag the
   * thumb (or click anywhere on the track) to set the level; double-click resets
   * to `default`; ↑/↓ nudge (Shift = fine). Fills its parent's height, so a strip
   * can let it grow tall — the drag maps against the live track height, not a
   * fixed pixel count.
   *
   * Value is linear in `[min, max]` (the caller owns any dB conversion for the
   * readout). A `unity` tick can mark a reference level (e.g. 0 dB). Generic +
   * app-agnostic (no Arbor concepts) → lives in shared/ui/.
   */
  let {
    value = $bindable(0),
    min = 0,
    max = 1,
    default: dflt,
    /** Reference tick (e.g. unity gain). Omit → no tick. */
    unity,
    color = 'var(--accent)',
    disabled = false,
    ariaLabel,
    onchange,
  }: {
    value?: number;
    min?: number;
    max?: number;
    /** Reset target on double-click (defaults to `max`, the usual unity-at-top). */
    default?: number;
    unity?: number;
    color?: string;
    disabled?: boolean;
    ariaLabel?: string;
    onchange?: (v: number) => void;
  } = $props();

  const span = $derived(max - min || 1);
  const norm = $derived(Math.max(0, Math.min(1, (value - min) / span)));
  const pct = $derived(norm * 100);
  const unityPct = $derived(unity != null ? Math.max(0, Math.min(1, (unity - min) / span)) * 100 : null);
  const resetTo = $derived(dflt ?? max);

  let trackEl = $state<HTMLElement | null>(null);

  function commit(v: number) {
    const clamped = Math.max(min, Math.min(max, v));
    if (clamped !== value) { value = clamped; onchange?.(clamped); }
  }

  function onPointerDown(e: PointerEvent) {
    if (disabled || !trackEl) return;
    e.preventDefault();
    trackEl.setPointerCapture?.(e.pointerId);
    const apply = (clientY: number) => {
      const r = trackEl!.getBoundingClientRect();
      const frac = 1 - (clientY - r.top) / (r.height || 1); // top = max
      commit(min + frac * span);
    };
    apply(e.clientY);
    const move = (ev: PointerEvent) => apply(ev.clientY);
    const up = (ev: PointerEvent) => {
      trackEl?.releasePointerCapture?.(ev.pointerId);
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  function reset() { if (!disabled) commit(resetTo); }

  function onKey(e: KeyboardEvent) {
    if (disabled) return;
    const step = (e.shiftKey ? 0.01 : 0.05) * span;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') { e.preventDefault(); commit(value + step); }
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') { e.preventDefault(); commit(value - step); }
  }
</script>

<div class="fader-ctl" class:disabled style="--fc: {color}">
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="fd-track"
    bind:this={trackEl}
    role="slider"
    tabindex={disabled ? -1 : 0}
    aria-label={ariaLabel}
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={value}
    aria-disabled={disabled || undefined}
    onpointerdown={onPointerDown}
    ondblclick={reset}
    onkeydown={onKey}
  >
    <div class="fd-fill" style="height: {pct}%"></div>
    {#if unityPct != null}<div class="fd-unity" style="bottom: {unityPct}%"></div>{/if}
    <div class="fd-thumb" style="bottom: {pct}%"></div>
  </div>
</div>

<style>
  .fader-ctl {
    display: flex; align-items: stretch; justify-content: center;
    width: 22px; height: 100%; min-height: 40px;
    /* Headroom for the thumb at the extremes, INSIDE the box (border-box) so the
       track never overflows its container onto the readout below. */
    padding: 6px 0; box-sizing: border-box;
  }
  .fader-ctl.disabled { opacity: 0.45; pointer-events: none; }

  .fd-track {
    position: relative;
    width: 6px; height: 100%;
    border-radius: 999px;
    background: var(--bg-input);
    box-shadow: inset 0 0 0 1px var(--border-subtle);
    cursor: ns-resize; outline: none; touch-action: none;
  }
  .fd-track:focus-visible { box-shadow: inset 0 0 0 1px var(--border-subtle), 0 0 0 2px var(--accent); }

  .fd-fill {
    position: absolute; left: 0; right: 0; bottom: 0;
    border-radius: 999px;
    background: linear-gradient(180deg,
      color-mix(in srgb, var(--fc) 90%, transparent),
      color-mix(in srgb, var(--fc) 45%, transparent));
  }
  /* Reference (unity) tick — a faint line across the track. */
  .fd-unity {
    position: absolute; left: -2px; right: -2px; height: 1px;
    background: color-mix(in srgb, var(--text-muted) 70%, transparent);
    pointer-events: none;
  }
  .fd-thumb {
    position: absolute; left: 50%; width: 16px; height: 8px;
    transform: translate(-50%, 50%); /* centre the thumb on its value line */
    border-radius: 3px;
    background: var(--bg-elevated);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--fc) 70%, var(--border)),
                0 1px 2px rgba(0, 0, 0, 0.35),
                inset 0 1px 0 color-mix(in srgb, #fff 18%, transparent);
    pointer-events: none;
  }
  /* A thin grip line on the thumb. */
  .fd-thumb::after {
    content: ''; position: absolute; left: 3px; right: 3px; top: 50%;
    height: 1px; transform: translateY(-50%);
    background: color-mix(in srgb, var(--fc) 80%, transparent);
  }
</style>
