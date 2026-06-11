<script lang="ts">
  /**
   * One arrangement lane drawn from **real haps** (the `grove_query` result),
   * not a fake waveform. Each hap is a small block positioned by its `[start,end)`
   * in cycles; pitched haps stack as a mini piano-roll (high notes on top),
   * unpitched (drum/sample) haps stack one row per distinct sound, and continuous
   * signals (no onset) render as a faint full-height band. A block lights up while
   * the transport playhead is inside it.
   *
   * Geometry is `$derived` once per arrangement; only the per-block "active" class
   * reacts to the playhead, so following playback stays cheap.
   */
  import type { VizLane } from './arrangement.svelte';

  let {
    lane,
    color,
    view,
    px,
    dimmed = false,
    playCycle,
    playing,
  }: {
    lane: VizLane;
    color: string;
    /** Visible timeline width in cycles. */
    view: number;
    /** Pixels per cycle. */
    px: number;
    dimmed?: boolean;
    /** Live playhead position in cycles (from the transport store). */
    playCycle: number;
    playing: boolean;
  } = $props();

  const VPAD = 10;    // % vertical padding inside the lane
  const PITCH_H = 9;  // % block height for a pitched note
  const DRUM_H = 12;  // % block height for a drum / sample hit

  interface Block {
    x: number; w: number; top: number; h: number;
    start: number; end: number; cont: boolean;
  }

  function clamp(v: number) { return Math.max(0, Math.min(100 - 4, v)); }

  const blocks = $derived.by<Block[]>(() => {
    const out: Block[] = [];
    const lo = lane.noteLo;
    const hi = lane.noteHi;
    const pitchSpan = lo != null && hi != null ? Math.max(1, hi - lo) : 1;
    // Each distinct unpitched sound gets its own row (kick / snare / hat separate).
    const drumRow = new Map<string, number>();
    lane.sounds.forEach((s, i) => drumRow.set(s, i));
    const drumRows = Math.max(1, lane.sounds.length);

    for (const h of lane.haps) {
      const x = h.start * px;
      const w = Math.max(2, (h.end - h.start) * px);
      if (!h.has_onset) {
        out.push({ x, w, top: VPAD, h: 100 - 2 * VPAD, start: h.start, end: h.end, cont: true });
        continue;
      }
      let center: number;
      if (h.note != null && lo != null) {
        const frac = (h.note - lo) / pitchSpan;             // 0 = lowest, 1 = highest
        center = VPAD + (1 - frac) * (100 - 2 * VPAD);       // high notes near the top
        out.push({ x, w, top: clamp(center - PITCH_H / 2), h: PITCH_H, start: h.start, end: h.end, cont: false });
      } else {
        const row = h.sound ? (drumRow.get(h.sound) ?? 0) : 0;
        const frac = drumRows === 1 ? 0.5 : row / (drumRows - 1);
        center = VPAD + frac * (100 - 2 * VPAD);
        out.push({ x, w, top: clamp(center - DRUM_H / 2), h: DRUM_H, start: h.start, end: h.end, cont: false });
      }
    }
    return out;
  });
</script>

<div class="haplane" class:dimmed style="--c: {color}; width: {view * px}px;">
  {#each blocks as b, i (i)}
    <div
      class="hap"
      class:cont={b.cont}
      class:active={playing && playCycle >= b.start && playCycle < b.end}
      style="left: {b.x}px; width: {b.w}px; top: {b.top}%; height: {b.h}%;"
    ></div>
  {/each}
</div>

<style>
  .haplane { position: absolute; inset: 0; pointer-events: none; }
  .haplane.dimmed { opacity: 0.4; }

  .hap {
    position: absolute;
    min-width: 2px;
    border-radius: 3px;
    background: linear-gradient(180deg,
      color-mix(in srgb, var(--c) 78%, transparent),
      color-mix(in srgb, var(--c) 52%, transparent));
    border: 1px solid color-mix(in srgb, var(--c) 70%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, #fff 14%, transparent);
    transition: background var(--transition-fast), box-shadow var(--transition-fast);
  }
  /* Continuous signal (no onset): a soft band rather than a discrete block. */
  .hap.cont {
    border-radius: 4px;
    background: color-mix(in srgb, var(--c) 18%, transparent);
    border-color: color-mix(in srgb, var(--c) 34%, transparent);
    box-shadow: none;
  }
  /* Sounding right now — brighten + halo. */
  .hap.active {
    background: linear-gradient(180deg,
      color-mix(in srgb, var(--c) 96%, #fff),
      color-mix(in srgb, var(--c) 78%, #fff));
    border-color: color-mix(in srgb, #fff 55%, var(--c));
    box-shadow: 0 0 0 1px color-mix(in srgb, #fff 35%, transparent),
                0 0 8px color-mix(in srgb, var(--c) 70%, transparent);
  }
  .hap.cont.active {
    background: color-mix(in srgb, var(--c) 40%, transparent);
    box-shadow: 0 0 8px color-mix(in srgb, var(--c) 50%, transparent);
  }
</style>
