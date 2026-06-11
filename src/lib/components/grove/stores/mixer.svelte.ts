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
 * Room / delay are **code-first**: no track-level audio command, so their knobs
 * are not live overrides — they reflect the literal in the source and commit
 * straight back to it (`grove_set_literal` via the editor's Tree-sitter tree).
 * `room` is a single value in the mixer; `delay`'s three params (time/fb/mix)
 * live in the Inspector. gain / pan keep their live ephemeral override and gain
 * an explicit **commit** that writes the current value into the source.
 */

import { groveSetTrack } from '$lib/ipc/grove';
import { arrangementStore, noteName, type VizLane } from '../viz/arrangement.svelte';
import { projectStore } from './project.svelte';
import { groveStore } from '../grove-store.svelte';
import { controlsStore } from './controls.svelte';
import { laneColor } from '../palette';
import {
  DELAY_DEFAULT_FB, DELAY_DEFAULT_MIX, type ControlEdit, type DelayValues,
} from '../editor/grove-edit';

/** Neutral baselines — a fresh strip is unity gain, centre pan. */
export const GAIN_UNITY = 1;
export const PAN_CENTER = 0.5;
/** A fresh delay (knob seed when the source has none): a quarter-cycle echo. */
export const DELAY_DEFAULT: DelayValues = { t: 0.25, fb: DELAY_DEFAULT_FB, mix: DELAY_DEFAULT_MIX };

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

  // Code-first knob buffers (index → value). These mirror the SOURCE literal
  // (seeded from controlsStore) and commit back to it; the buffer just holds the
  // value mid-drag until the eval/re-parse round-trip catches up. Cleared on
  // rebaseline (the source becomes authoritative again).
  let roomBuf  = $state<Record<number, number>>({});
  let delayBuf = $state<Record<number, DelayValues>>({});

  // Debounced commit: a knob drag fires many onchange; commit (which re-evals)
  // once the gesture settles. Holds the latest pending edit per (index,control).
  let commitTimer: ReturnType<typeof setTimeout> | null = null;
  const pendingEdits = new Map<number, Map<string, ControlEdit>>();
  function scheduleCommit(index: number, edit: ControlEdit) {
    let m = pendingEdits.get(index);
    if (!m) { m = new Map(); pendingEdits.set(index, m); }
    m.set(edit.kind, edit);
    if (commitTimer) clearTimeout(commitTimer);
    commitTimer = setTimeout(flushCommits, 280);
  }
  function flushCommits() {
    commitTimer = null;
    for (const [index, m] of pendingEdits) groveStore.requestCommit(index, [...m.values()]);
    pendingEdits.clear();
  }

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

    // ── gain / pan: live ephemeral overrides + explicit commit ──
    gain(i: number) { return gains[i] ?? GAIN_UNITY; },
    pan(i: number)  { return pans[i]  ?? PAN_CENTER; },
    setGain(i: number, v: number) { gains = { ...gains, [i]: v }; void groveSetTrack('gain', i, v); },
    setPan(i: number, v: number)  { pans  = { ...pans,  [i]: v }; void groveSetTrack('pan', i, v); },
    /** Whether strip `i` has an uncommitted gain/pan override (commit affordance). */
    hasOverride(i: number) { return gains[i] != null || pans[i] != null; },
    /** Commit strip `i`'s current gain/pan override into the source as literals.
     *  The eval that follows re-baselines, so the override resets and the value
     *  now lives in the `.grove`. */
    commit(i: number) {
      const edits: ControlEdit[] = [];
      if (gains[i] != null) edits.push({ kind: 'gain', value: gains[i] });
      if (pans[i]  != null) edits.push({ kind: 'pan',  value: pans[i]  });
      if (edits.length) groveStore.requestCommit(i, edits);
    },
    /** Commit every overridden strip (Command Palette / shortcut). */
    commitAll() {
      const indices = new Set<number>([...Object.keys(gains), ...Object.keys(pans)].map(Number));
      for (const i of indices) this.commit(i);
    },
    /** Number of strips with a pending gain/pan override. */
    get overrideCount() {
      return new Set<number>([...Object.keys(gains), ...Object.keys(pans)].map(Number)).size;
    },

    // ── room / delay: code-first knobs (seed from source, commit to source) ──
    room(i: number) { return roomBuf[i] ?? controlsStore.room(i); },
    roomCalculated(i: number) { return controlsStore.isCalculated(i, 'room'); },
    setRoom(i: number, v: number) {
      roomBuf = { ...roomBuf, [i]: v };
      scheduleCommit(i, { kind: 'room', value: v });
    },
    delay(i: number): DelayValues { return delayBuf[i] ?? controlsStore.delay(i) ?? DELAY_DEFAULT; },
    delayActive(i: number) { return delayBuf[i] != null || controlsStore.delay(i) != null; },
    delayCalculated(i: number) { return controlsStore.isCalculated(i, 'delay'); },
    /** Set one delay parameter, merging with the current triple, and commit. */
    setDelayParam(i: number, key: keyof DelayValues, v: number) {
      const cur = this.delay(i);
      const next = { ...cur, [key]: v };
      delayBuf = { ...delayBuf, [i]: next };
      scheduleCommit(i, { kind: 'delay', ...next });
    },

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
    /** Solo computed over the REAL strips only — the shared mute/solo map can
     *  retain keys for indices no longer present after a re-eval with fewer
     *  tracks, which would otherwise dim every visible lane. */
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

    /** Drop all overrides + code-first buffers — call on each eval (the source
     *  is authoritative again: gain/pan re-baseline to neutral, room/delay reflect
     *  the freshly-parsed literals via controlsStore). */
    rebaseline() {
      gains = {}; pans = {}; master = GAIN_UNITY;
      roomBuf = {}; delayBuf = {};
    },
  };
}

export const mixerStore = createMixerStore();
