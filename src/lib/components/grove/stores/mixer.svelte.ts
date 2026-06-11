/**
 * Mixer store — the per-track live control surface (Fase 4 · Step 3b).
 *
 * Tracks are keyed by their **arrangement-strip INDEX** — the only stable
 * identity the engine exposes (`grove_query` haps and `grove:meters` rows are
 * both index-addressed). The track *model* (name / colour / character) is derived
 * from the shared arrangement query (`arrangementStore.lanes`) plus the active
 * source; the *editable* values are **live ephemeral overrides** (gate 2): they
 * are pushed to the running session via `grove_set_track` and the source stays
 * authoritative, so every eval re-baselines and the overrides reset to neutral
 * (see {@link rebaseline}).
 *
 * Mute / solo live in the shared GroveShell store (keyed by `String(index)`) so
 * the arrangement headers (Step 3a) and these strips stay in sync; toggling here
 * also pushes the live audio override.
 *
 * Room / send are **per-event (code-first)** — there is no track-level audio
 * command for them (the surgical "knob → source literal" round-trip is the
 * future `grove_set_literal`), so the mixer renders them disabled.
 */

import { groveSetTrack } from '$lib/ipc/grove';
import { arrangementStore, noteName, type VizLane } from '../viz/arrangement.svelte';
import { projectStore } from './project.svelte';
import { groveStore } from '../grove-store.svelte';
import { laneColor } from '../mock/colors';

/** Neutral baselines — a fresh strip is unity gain, centre pan. */
export const GAIN_UNITY = 1;
export const PAN_CENTER = 0.5;

/** One mixer strip = one arrangement lane, enriched with a display name. */
export interface MixerTrack {
  /** Arrangement-strip index (0-based) — the key into meters / mute-solo. */
  index: number;
  /** Best-effort name from the source `track("…")`, else `Track N`. */
  name: string;
  /** Stable per-index accent colour. */
  color: string;
  /** Short voice/character label (sounds, else pitch range). */
  voice: string;
  sounds: string[];
  noteLo: number | null;
  noteHi: number | null;
  noteCount: number;
  hasContinuous: boolean;
  /** Hap count over the queried window (pattern density hint). */
  hapCount: number;
}

/** Track names from the active source: `track("name", …)` in declaration order
 *  == strip order (the BE query returns only indices). Best-effort regex.
 *  NOTE: ArrangementView (Step 3a) has a parallel copy — a post-merge follow-up
 *  should hoist both into `viz/arrangement.svelte`. */
function trackNames(src: string): string[] {
  const out: string[] = [];
  const re = /\btrack\(\s*"([^"]+)"/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) out.push(m[1]);
  return out;
}

/** A short voice/character label for a lane (sounds, else pitch range). */
function voiceLabel(l: VizLane): string {
  if (l.sounds.length) return l.sounds.slice(0, 3).join(' ');
  if (l.noteCount && l.noteLo != null && l.noteHi != null) {
    return `${noteName(l.noteLo)}–${noteName(l.noteHi)}`;
  }
  if (l.hasContinuous) return 'signal';
  return '—';
}

function createMixerStore() {
  // Live override deltas (index → value). Absent = neutral (source baseline).
  let gains  = $state<Record<number, number>>({});
  let pans   = $state<Record<number, number>>({});
  let master = $state(GAIN_UNITY);

  const tracks = $derived.by<MixerTrack[]>(() => {
    const names = trackNames(projectStore.activeSource);
    return arrangementStore.lanes.map((l) => ({
      index: l.track,
      name: names[l.track] ?? `Track ${l.track + 1}`,
      color: laneColor(l.track),
      voice: voiceLabel(l),
      sounds: l.sounds,
      noteLo: l.noteLo,
      noteHi: l.noteHi,
      noteCount: l.noteCount,
      hasContinuous: l.hasContinuous,
      hapCount: l.haps.length,
    }));
  });

  return {
    get tracks() { return tracks; },
    byIndex(i: number): MixerTrack | null { return tracks.find((t) => t.index === i) ?? null; },

    // ── gain / pan: live ephemeral overrides (gate 2) ──
    gain(i: number) { return gains[i] ?? GAIN_UNITY; },
    pan(i: number)  { return pans[i]  ?? PAN_CENTER; },
    setGain(i: number, v: number) { gains = { ...gains, [i]: v }; void groveSetTrack('gain', i, v); },
    setPan(i: number, v: number)  { pans  = { ...pans,  [i]: v }; void groveSetTrack('pan', i, v); },

    get masterGain() { return master; },
    setMasterGain(v: number) { master = v; void groveSetTrack('master_gain', null, v); },

    // ── mute / solo: shared store (index key) + live override ──
    isMuted(i: number)  { return groveStore.isMuted(String(i)); },
    isSoloed(i: number) { return groveStore.isSoloed(String(i)); },
    toggleMute(i: number) {
      const k = String(i);
      groveStore.toggleMute(k);
      void groveSetTrack('mute', i, groveStore.isMuted(k) ? 1 : 0);
    },
    toggleSolo(i: number) {
      const k = String(i);
      groveStore.toggleSolo(k);
      void groveSetTrack('solo', i, groveStore.isSoloed(k) ? 1 : 0);
    },
    /** Solo computed over the REAL strips only — the shared store still carries
     *  stale Step-0 mock solo state which would otherwise dim every lane. */
    get anySolo() { return tracks.some((t) => groveStore.isSoloed(String(t.index))); },
    /** Whether a strip should be visually dimmed (muted, or solo-excluded). */
    isDimmed(i: number) { return this.isMuted(i) || (this.anySolo && !this.isSoloed(i)); },

    // ── selection (mixer ↔ inspector; arrangement joins once 3a wires it) ──
    get selectedIndex(): number | null {
      const id = groveStore.selectedTrackId;
      if (id == null) return null;
      const n = Number(id);
      return Number.isInteger(n) ? n : null;
    },
    select(i: number) { groveStore.selectTrack(String(i)); },

    /** Drop all gain/pan/master overrides back to neutral — call on each eval
     *  (the engine re-baselines from source, so the deltas no longer apply). */
    rebaseline() { gains = {}; pans = {}; master = GAIN_UNITY; },
  };
}

export const mixerStore = createMixerStore();
