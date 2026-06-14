<script lang="ts">
  /**
   * One arrangement lane drawn from **real haps** (the `nemus_query` result),
   * Logic-Pro style. Two event families, drawn the way a DAW actually renders
   * them — never with a decorative wave that doesn't match the data:
   *
   *  1. **MIDI notes** (`note != null`) → clean **piano-roll blocks**, positioned
   *     by pitch (high notes near the top), sized by duration. No waveform: a note
   *     is a discrete pitched event, so it reads as a solid block. With the
   *     `labels` option on, the note name is printed inside blocks wide enough.
   *
   *  2. **Audio regions** — sample / drum hits (`note == null`, a `sound`) and
   *     continuous signals (`has_onset == false`). These are real *audio*, so a
   *     waveform is meaningful here — but only when the `waveform` option is on.
   *     Off (the default) they render as clean region blocks too; on, a single
   *     synthesized region shape (no PCM exists) spans each region's range, drawn
   *     cleanly inside the clip box rather than as lane-wide noise.
   *
   * Every event is a real click target (picks the hap → Inspector) and carries a
   * hover tooltip. Geometry is `$derived` once per arrangement; only the per-event
   * "active" class reacts to the playhead, so following playback stays cheap. No
   * RAF — the "alive" feel comes from the active highlight + the transport overlay.
   */
  import { tooltip } from '$lib/actions/tooltip';
  import type { VizLane } from './arrangement.svelte';
  import { noteName } from './arrangement.svelte';
  import type { NemusQueryHap } from '$lib/ipc/nemus';
  import { arrViewOptions } from './arr-view-options.svelte';

  let {
    lane,
    color,
    view,
    px,
    dimmed = false,
    playCycle,
    playing,
    selectedKey = null,
    inSelection,
    onpick,
    ongoto,
    writtenNote,
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
    /** Key of the currently selected event (`<track>:<idx>`), highlighted here. */
    selectedKey?: string | null;
    /** True when a hap's source span overlaps the editor's text selection — the
     *  editor→DAW link boxes these regions. */
    inSelection?: (hap: NemusQueryHap) => boolean;
    /** Picked an event — opens it in the Inspector. */
    onpick?: (hap: NemusQueryHap) => void;
    /** Ctrl/Cmd+clicked an event — reveal the source span that produced it. */
    ongoto?: (hap: NemusQueryHap) => void;
    /** Written note literal behind a hap when a transform shifted its pitch
     *  (e.g. `.add(-24)`), for the "sounds X · written Y" tooltip hint. */
    writtenNote?: (hap: NemusQueryHap) => string | null;
  } = $props();

  const VPAD = 12;     // % vertical padding inside the lane
  const PITCH_H = 12;  // % block height for a pitched note
  const DRUM_H = 15;   // % block height for a drum / sample hit

  // Waveform tuning (only consulted when the option is on for an audio region).
  const WAVE_HALF = 36;     // % half-height the wave fills inside its clip box
  const SVG_H = 100;
  const SVG_MID = SVG_H / 2;

  type Kind = 'note' | 'audio';

  interface Block {
    /** Stable key into the selection highlight (`<track>:<hapIndex>`). */
    key: string;
    /** The real hap (for the pick callback + tooltip). */
    hap: NemusQueryHap;
    x: number; w: number; top: number; h: number;
    start: number; end: number; kind: Kind;
    /** Note-name label (pitched events only), shown when the block is wide. */
    label: string | null;
    /** Gain mapped to 0..1 (unity = 1), for the velocity heatmap. */
    vel: number;
  }

  // Gain → 0..1 velocity. Unity (1.0) and louder read as full; attenuated events
  // dim. `null` gain = unity. The heatmap uses this to fade quiet events back.
  const velOf = (h: NemusQueryHap) => Math.max(0, Math.min(1, h.gain ?? 1));
  const showVelocity = $derived(arrViewOptions.velocity);

  function clamp(v: number) { return Math.max(0, Math.min(100 - PITCH_H, v)); }

  // bar:beat + duration for the hover tooltip (1 cycle = 1 bar, 4 beats / bar).
  function barBeat(cyc: number): string {
    const bar = Math.floor(cyc) + 1;
    const beat = (cyc - Math.floor(cyc)) * 4 + 1;
    return `${bar}:${beat.toFixed(2).replace(/\.?0+$/, '')}`;
  }
  function hapTip(h: NemusQueryHap) {
    const name = h.note != null ? noteName(h.note) : (h.sound ?? (h.has_onset ? 'event' : 'signal'));
    const durBeats = (h.end - h.start) * 4;
    const parts = [
      `bar ${barBeat(h.start)}`,
      h.has_onset ? `${durBeats.toFixed(2).replace(/\.?0+$/, '')} beat${durBeats === 1 ? '' : 's'}` : 'continuous',
    ];
    if (h.note != null) {
      parts.push(`MIDI ${Math.round(h.note)}`);
      const written = writtenNote?.(h);
      if (written) parts.push(`written ${written}`);
    }
    if (h.gain != null) parts.push(`gain ${h.gain.toFixed(2)}`);
    return { content: name, description: parts.join(' · ') };
  }

  // ── Piano-roll blocks: every discrete hap, positioned + click-targeted ────────
  const blocks = $derived.by<Block[]>(() => {
    const out: Block[] = [];
    const lo = lane.noteLo;
    const hi = lane.noteHi;
    const pitchSpan = lo != null && hi != null ? Math.max(1, hi - lo) : 1;
    const drumRow = new Map<string, number>();
    lane.sounds.forEach((s, i) => drumRow.set(s, i));
    const drumRows = Math.max(1, lane.sounds.length);

    lane.haps.forEach((h, i) => {
      if (!h.has_onset) return; // continuous → drawn as a region band below
      const x = h.start * px;
      const w = Math.max(2, (h.end - h.start) * px);
      const key = `${lane.track}:${i}`;
      if (h.note != null && lo != null) {
        const frac = (h.note - lo) / pitchSpan;            // 0 = lowest, 1 = highest
        const center = VPAD + (1 - frac) * (100 - 2 * VPAD); // high notes near the top
        out.push({ key, hap: h, x, w, top: clamp(center - PITCH_H / 2), h: PITCH_H,
          start: h.start, end: h.end, kind: 'note', label: noteName(h.note), vel: velOf(h) });
      } else {
        const row = h.sound ? (drumRow.get(h.sound) ?? 0) : 0;
        const frac = drumRows === 1 ? 0.5 : row / (drumRows - 1);
        const center = VPAD + frac * (100 - 2 * VPAD);
        out.push({ key, hap: h, x, w, top: clamp(center - DRUM_H / 2), h: DRUM_H,
          start: h.start, end: h.end, kind: 'audio', label: h.sound ?? null, vel: velOf(h) });
      }
    });
    return out;
  });

  // An *audio* lane (no pitched notes; carries sounds and/or a continuous signal)
  // is the only place a waveform is meaningful — gate the wave on this + the option.
  const isAudioLane = $derived(lane.noteCount === 0 && (lane.sounds.length > 0 || lane.hasContinuous));
  const showWave = $derived(arrViewOptions.waveform && isAudioLane);

  // Continuous (no-onset) signal → one full-width region band (always, even
  // without the waveform option, since there are no discrete blocks for it).
  const isContinuous = $derived(lane.haps.length > 0 && lane.haps.every((h) => !h.has_onset));

  // ── Audio region waveform: one synthesized shape per region, drawn ONLY for
  // audio lanes when the option is on. A carrier scaled by an onset-driven
  // envelope, kept inside the clip box (no lane-wide noise). ─────────────────────
  interface Wave { start: number; end: number; x: number; w: number; fill: string; line: string; }

  const wave = $derived.by<Wave | null>(() => {
    if (!showWave) return null;
    const haps = lane.haps;
    if (!haps.length) return null;

    let regStart = Infinity, regEnd = -Infinity;
    for (const h of haps) {
      if (h.start < regStart) regStart = h.start;
      if (h.end > regEnd) regEnd = h.end;
    }
    if (!Number.isFinite(regStart) || regEnd <= regStart) return null;

    const x = regStart * px;
    const wPx = Math.max(6, (regEnd - regStart) * px);
    const spanCyc = regEnd - regStart;
    const n = Math.max(8, Math.min(900, Math.round(wPx / 4)));

    const onsets: { t: number; amp: number }[] = [];
    if (!isContinuous) {
      for (const h of haps) {
        if (!h.has_onset) continue;
        onsets.push({ t: h.start - regStart, amp: Math.max(0.4, Math.min(1, h.gain ?? 1)) });
      }
      onsets.sort((a, b) => a.t - b.t);
    }

    const ATTACK = 0.04, RELEASE = 0.55, BASE = 0.16;
    function envAt(cyc: number): number {
      if (isContinuous) {
        const u = spanCyc > 0 ? cyc / spanCyc : 0;
        return 0.55 + 0.35 * Math.sin(u * Math.PI);
      }
      let e = BASE;
      for (const o of onsets) {
        const d = cyc - o.t;
        if (d < -ATTACK) continue;
        const shape = d < 0 ? (d + ATTACK) / ATTACK : Math.exp(-d / RELEASE);
        const v = o.amp * shape;
        if (v > e) e = v;
      }
      return Math.min(1, e);
    }

    const carrierFreq = Math.max(6, Math.min(64, spanCyc * 2.2));
    const top: string[] = [];
    const bot: number[] = [];
    for (let i = 0; i <= n; i++) {
      const t = i / n;
      const cyc = t * spanCyc;
      const env = envAt(cyc);
      const carrier = Math.sin(t * Math.PI * 2 * carrierFreq * (isContinuous ? 0.5 : 1));
      const half = (0.22 + 0.78 * Math.abs(carrier)) * env * (WAVE_HALF / 100) * SVG_H;
      const x100 = t * 100;
      top.push(`${i === 0 ? 'M' : 'L'}${x100.toFixed(2)} ${(SVG_MID - half).toFixed(1)}`);
      bot.push(SVG_MID + half);
    }
    let fill = top.join('');
    for (let i = n; i >= 0; i--) fill += `L${((i / n) * 100).toFixed(2)} ${bot[i].toFixed(1)}`;
    fill += 'Z';

    return { start: regStart, end: regEnd, x, w: wPx, fill, line: top.join('') };
  });

  const waveActive = $derived(!!wave && playing && playCycle >= wave.start && playCycle < wave.end);

  // Continuous region box (drawn whenever a no-onset signal exists, with or
  // without the carved waveform on top).
  const contBox = $derived.by(() => {
    if (!isContinuous) return null;
    let regStart = Infinity, regEnd = -Infinity;
    for (const h of lane.haps) {
      if (h.start < regStart) regStart = h.start;
      if (h.end > regEnd) regEnd = h.end;
    }
    if (!Number.isFinite(regStart) || regEnd <= regStart) return null;
    return { x: regStart * px, w: Math.max(6, (regEnd - regStart) * px), start: regStart, end: regEnd };
  });
  const contActive = $derived(!!contBox && playing && playCycle >= contBox.start && playCycle < contBox.end);

  function pick(h: NemusQueryHap, e: MouseEvent) {
    e.stopPropagation(); // don't let the lane-level click steal the event pick
    // Ctrl/Cmd+click jumps to the source span that produced this hap (IDE-style);
    // a plain click opens it in the Inspector.
    if (e.ctrlKey || e.metaKey) ongoto?.(h);
    else onpick?.(h);
  }
</script>

<div class="haplane" class:dimmed style="--c: {color}; width: {view * px}px;">
  <!-- Continuous-signal region: one band spanning the sounded range. The lane
       carries no discrete blocks, so this is its only representation. -->
  {#if contBox}
    {@const h = lane.haps[0]}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="region cont"
      class:active={contActive}
      class:selected={selectedKey === `${lane.track}:cont`}
      class:span-sel={inSelection?.(lane.haps[0])}
      style="left: {contBox.x}px; width: {contBox.w}px;"
      use:tooltip={hapTip(h)}
      onclick={(e) => pick(h, e)}
    >
      {#if wave}
        <svg class="wave" viewBox="0 0 100 {SVG_H}" preserveAspectRatio="none" aria-hidden="true">
          <path class="wave-fill" d={wave.fill} />
          <path class="wave-line" d={wave.line} />
        </svg>
      {/if}
    </div>
  {/if}

  <!-- Audio-region waveform overlay (sample/drum lanes), only when the option is
       on. The blocks below stay the click targets; this is a visual skin. -->
  {#if wave && !isContinuous}
    <div class="region wave-skin" class:active={waveActive} style="left: {wave.x}px; width: {wave.w}px;">
      <svg class="wave" viewBox="0 0 100 {SVG_H}" preserveAspectRatio="none" aria-hidden="true">
        <path class="wave-fill" d={wave.fill} />
        <path class="wave-line" d={wave.line} />
      </svg>
    </div>
  {/if}

  <!-- Discrete events: piano-roll notes + sample/drum hits. Clean blocks. -->
  {#each blocks as b (b.key)}
    {@const active = playing && playCycle >= b.start && playCycle < b.end}
    {@const wide = b.w >= 26}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="hap"
      class:drum={b.kind === 'audio'}
      class:active
      class:selected={selectedKey === b.key}
      class:span-sel={inSelection?.(b.hap)}
      class:muffled={showWave && b.kind === 'audio'}
      class:vel={showVelocity}
      style="left: {b.x}px; width: {b.w}px; top: {b.top}%; height: {b.h}%; --vel: {b.vel};"
      use:tooltip={hapTip(b.hap)}
      onclick={(e) => pick(b.hap, e)}
    >
      {#if arrViewOptions.labels && wide && b.label}
        <span class="hap-label">{b.label}</span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .haplane { position: absolute; inset: 0; }
  .haplane.dimmed { opacity: 0.4; }

  /* ── Audio region (continuous band + waveform skin) ─────────────────────────── */
  .region {
    position: absolute;
    top: 10%;
    height: 80%;
    border-radius: 4px;
    overflow: hidden;
  }
  .region.cont {
    cursor: pointer;
    background: color-mix(in srgb, var(--c) 14%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--c) 26%, transparent);
    transition: background var(--transition-fast), box-shadow var(--transition-fast);
  }
  .region.cont:hover { background: color-mix(in srgb, var(--c) 20%, transparent); }
  /* The waveform skin sits behind the (interactive) blocks — purely visual. */
  .region.wave-skin {
    pointer-events: none;
    background: linear-gradient(180deg,
      color-mix(in srgb, var(--c) 20%, transparent),
      color-mix(in srgb, var(--c) 9%, transparent));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--c) 26%, transparent);
  }
  .wave { position: absolute; inset: 0; width: 100%; height: 100%; display: block; }
  .wave-fill { fill: color-mix(in srgb, var(--c) 55%, transparent); stroke: none; transition: fill var(--transition-fast); }
  .wave-line {
    fill: none;
    stroke: color-mix(in srgb, #fff 55%, var(--c));
    stroke-width: 1.2;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
    opacity: 0.8;
    transition: stroke var(--transition-fast), opacity var(--transition-fast);
  }
  .region.active { box-shadow: inset 0 0 0 1px color-mix(in srgb, #fff 26%, transparent), 0 0 9px color-mix(in srgb, var(--c) 50%, transparent); }
  .region.active .wave-fill { fill: color-mix(in srgb, var(--c) 78%, #fff); }
  .region.active .wave-line { stroke: color-mix(in srgb, #fff 85%, var(--c)); opacity: 1; }
  .region.cont.selected { box-shadow: inset 0 0 0 1.5px color-mix(in srgb, #fff 70%, var(--c)), 0 0 8px color-mix(in srgb, var(--c) 45%, transparent); }

  /* ── Piano-roll / sample blocks (clean, interactive) ────────────────────────── */
  .hap {
    position: absolute;
    min-width: 2px;
    border-radius: 2px;
    cursor: pointer;
    overflow: hidden;
    background: color-mix(in srgb, var(--c) 92%, #fff 6%);
    box-shadow: 0 0 0 1px color-mix(in srgb, #000 18%, transparent),
                inset 0 1px 0 color-mix(in srgb, #fff 25%, transparent);
    transition: background var(--transition-fast), box-shadow var(--transition-fast), opacity var(--transition-fast);
  }
  .hap.drum { border-radius: 3px; }
  /* Velocity / gain heatmap: fade attenuated events toward the lane background so
     dynamics read at a glance. `--vel` is the event's gain mapped to 0..1 (unity =
     vivid). Hover / active / selected (declared after) still take over. */
  .hap.vel { background: color-mix(in srgb, var(--c) calc(30% + var(--vel) * 65%), var(--bg-base)); }
  .hap:hover { background: color-mix(in srgb, var(--c) 80%, #fff); box-shadow: 0 0 0 1px color-mix(in srgb, #fff 35%, transparent); }

  /* When the waveform skin is shown, dim the underlying audio blocks so the wave
     reads as the primary visual but the blocks stay there as click targets. */
  .hap.muffled { opacity: 0.45; }
  .hap.muffled:hover { opacity: 0.85; }

  .hap-label {
    display: block;
    padding: 0 3px;
    font-size: 8.5px;
    line-height: 1;
    font-family: var(--font-code);
    color: color-mix(in srgb, #000 72%, var(--c));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    position: absolute;
    top: 50%;
    left: 0;
    right: 0;
    transform: translateY(-50%);
    pointer-events: none;
  }

  /* Sounding right now — brighten + halo. */
  .hap.active {
    background: color-mix(in srgb, var(--c) 70%, #fff);
    box-shadow: 0 0 0 1px color-mix(in srgb, #fff 45%, transparent),
                0 0 8px color-mix(in srgb, var(--c) 70%, transparent);
  }
  /* Selected event — crisp ring so it's findable from the Inspector. */
  .hap.selected {
    background: color-mix(in srgb, var(--c) 60%, #fff);
    box-shadow: 0 0 0 1.5px color-mix(in srgb, #fff 75%, var(--c)),
                0 0 9px color-mix(in srgb, var(--c) 55%, transparent);
    opacity: 1;
    z-index: 2;
  }

  /* Editor→DAW link: the hap's source span overlaps the editor's text selection.
     A distinct accent box (vs the lane-tinted Inspector ring) so it reads as "this
     is what the text you selected produces" without being confused for selection. */
  .hap.span-sel,
  .region.cont.span-sel {
    outline: 1.5px solid var(--accent);
    outline-offset: 1px;
    box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 55%, transparent);
    opacity: 1;
    z-index: 3;
  }
</style>
