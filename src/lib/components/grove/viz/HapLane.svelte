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

  // Waveform sampling: small N keeps the path cheap (the timeline redraws often).
  const WAVE_N = 24;            // sample points across the block width
  const WAVE_MIN_W = 14;        // px below which a wave would be noise → skip it
  // SVG drawing space is a normalized 0..100 box, mapped to the block by viewBox.
  const SVG_W = 100;
  const SVG_H = 100;
  const SVG_MID = SVG_H / 2;

  type Kind = 'pitched' | 'unpitched' | 'cont';

  interface Block {
    x: number; w: number; top: number; h: number;
    start: number; end: number; kind: Kind;
    /** SVG path `d` for the synthesized waveform, or '' when too narrow to draw. */
    wave: string;
    /** Pitch-driven oscillation count, only meaningful for pitched blocks. */
    cycles: number;
  }

  function clamp(v: number) { return Math.max(0, Math.min(100 - 4, v)); }

  /**
   * Synthesize a stylized waveform path inside the normalized 0..100 SVG box.
   * No real PCM exists — the shape is generated from the hap's character:
   *  - pitched   → damped sine oscillation filling the held note
   *  - unpitched → sharp transient burst (fast attack, exponential decay) at onset
   *  - cont      → slow gentle full-width swell
   * `widthPx` gates detail so very short blocks degrade instead of aliasing.
   */
  function wavePath(kind: Kind, widthPx: number, cycles: number): string {
    if (widthPx < WAVE_MIN_W) return '';
    const n = Math.max(6, Math.min(WAVE_N, Math.round(widthPx / 5)));
    let d = '';
    for (let i = 0; i <= n; i++) {
      const t = i / n;                 // 0..1 along the width
      const x = t * SVG_W;
      let amp: number;                 // -1..1 displacement from the mid line
      if (kind === 'pitched') {
        // Damped sine: oscillation density rises a touch with pitch.
        const decay = Math.exp(-1.6 * t);
        amp = Math.sin(t * Math.PI * 2 * cycles) * decay;
      } else if (kind === 'unpitched') {
        // Transient burst: loud fast wobble at the onset, quick exponential decay.
        const env = Math.exp(-5 * t);
        amp = Math.sin(t * Math.PI * 2 * 5) * env;
      } else {
        // Continuous: one slow swell across the whole band.
        amp = Math.sin(t * Math.PI) * 0.6;
      }
      const y = SVG_MID - amp * (SVG_MID - 4);
      d += `${i === 0 ? 'M' : 'L'}${x.toFixed(1)} ${y.toFixed(1)}`;
    }
    return d;
  }

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
        out.push({
          x, w, top: VPAD, h: 100 - 2 * VPAD, start: h.start, end: h.end,
          kind: 'cont', cycles: 0, wave: wavePath('cont', w, 0),
        });
        continue;
      }
      let center: number;
      if (h.note != null && lo != null) {
        const frac = (h.note - lo) / pitchSpan;             // 0 = lowest, 1 = highest
        center = VPAD + (1 - frac) * (100 - 2 * VPAD);       // high notes near the top
        // Higher pitch → a little denser oscillation, kept in a readable range.
        const cycles = 3 + Math.round(frac * 4);
        out.push({
          x, w, top: clamp(center - PITCH_H / 2), h: PITCH_H, start: h.start, end: h.end,
          kind: 'pitched', cycles, wave: wavePath('pitched', w, cycles),
        });
      } else {
        const row = h.sound ? (drumRow.get(h.sound) ?? 0) : 0;
        const frac = drumRows === 1 ? 0.5 : row / (drumRows - 1);
        center = VPAD + frac * (100 - 2 * VPAD);
        out.push({
          x, w, top: clamp(center - DRUM_H / 2), h: DRUM_H, start: h.start, end: h.end,
          kind: 'unpitched', cycles: 0, wave: wavePath('unpitched', w, 0),
        });
      }
    }
    return out;
  });
</script>

<div class="haplane" class:dimmed style="--c: {color}; width: {view * px}px;">
  {#each blocks as b, i (i)}
    {@const active = playing && playCycle >= b.start && playCycle < b.end}
    <div
      class="hap"
      class:cont={b.kind === 'cont'}
      class:active
      style="left: {b.x}px; width: {b.w}px; top: {b.top}%; height: {b.h}%;"
    >
      {#if b.wave}
        <svg
          class="wave"
          class:active
          viewBox="0 0 {SVG_W} {SVG_H}"
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          <path d={b.wave} />
        </svg>
      {/if}
    </div>
  {/each}
</div>

<style>
  .haplane { position: absolute; inset: 0; pointer-events: none; }
  .haplane.dimmed { opacity: 0.4; }

  .hap {
    position: absolute;
    min-width: 2px;
    border-radius: 3px;
    overflow: hidden;
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

  /* Synthesized waveform painted over the block — purely decorative, never
     captures pointer events so the block underneath stays clickable. */
  .wave {
    position: absolute;
    inset: 1px;
    width: calc(100% - 2px);
    height: calc(100% - 2px);
    pointer-events: none;
    overflow: visible;
  }
  .wave path {
    fill: none;
    stroke: color-mix(in srgb, #fff 70%, var(--c));
    stroke-width: 1.4;
    stroke-linecap: round;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
    opacity: 0.7;
    transition: stroke var(--transition-fast), opacity var(--transition-fast);
  }
  .hap.cont .wave path {
    stroke: color-mix(in srgb, var(--c) 60%, #fff);
    opacity: 0.5;
  }
  /* Sounding now → the wave flares to a near-white, fully opaque trace. */
  .wave.active path {
    stroke: color-mix(in srgb, #fff 88%, var(--c));
    opacity: 1;
    stroke-width: 1.7;
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
