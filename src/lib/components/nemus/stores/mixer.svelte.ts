/**
 * Mixer store — the per-track live control surface (Fase 4 · Step 3b).
 *
 * Tracks are keyed by their **arrangement-strip INDEX** — the only stable
 * identity the engine exposes (`nemus_query` haps and `nemus:meters` rows are
 * both index-addressed). The track *model* (name / colour / character) is derived
 * from the shared arrangement query (`arrangementStore.lanes`) plus the active
 * source; the *editable* values are **live ephemeral overrides** (gate 2): they
 * are pushed to the running session via `nemus_set_track` and the source stays
 * authoritative, so every eval re-baselines and the overrides reset to neutral
 * (see {@link rebaseline}).
 *
 * Mute / solo live in the shared NemusShell store (keyed by `String(index)`) so
 * the arrangement headers (Step 3a) and these strips stay in sync; toggling here
 * also pushes the live audio override.
 *
 * Room / delay are **code-first**: no track-level audio command, so their knobs
 * are not live overrides — they reflect the literal in the source and commit
 * straight back to it (`nemus_set_literal` via the editor's Tree-sitter tree).
 * `room` is a single value in the mixer; `delay`'s three params (time/fb/mix)
 * live in the Inspector.
 *
 * gain / pan are **both**: the knob pushes a live ephemeral override for instant
 * audio feedback AND schedules a debounced write-through into the source (same
 * `scheduleCommit` path as room/delay) — so a drag is heard immediately and, once
 * the gesture settles (~280ms), the literal `.gain(x)` / `.pan(x)` is written to
 * the `.nemus`. No explicit commit button is needed; `commit`/`commitAll` just
 * flush the pending write early (Command Palette / the ↧ affordance).
 *
 * Mute writes `.gain(0)` into the source (immediate commit, not debounced — a
 * mute is a discrete intent, not a drag) plus the live mute override; unmute
 * restores the pre-mute gain. When a track's gain is a *calculated* argument it
 * can't be rewritten surgically, so mute stays live-only there (the source is
 * left untouched and the strip shows it isn't persistible). Solo has no DSL
 * representation, so it stays live-only — never written to the source.
 */

import { nemusSetTrack } from '$lib/ipc/nemus';
import { arrangementStore, noteName, type VizLane } from '../viz/arrangement.svelte';
import { projectStore } from './project.svelte';
import { nemusStore } from '../nemus-store.svelte';
import { controlsStore } from './controls.svelte';
import { laneColor } from '../palette';
import {
  DELAY_DEFAULT_FB, DELAY_DEFAULT_MIX, type ControlEdit, type DelayValues,
} from '../editor/nemus-edit';

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
  // once the gesture settles. Holds the latest pending edit per (index,control)
  // so gain/pan/room/delay drags all coalesce into one write per gesture.
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
    for (const [index, m] of pendingEdits) nemusStore.requestCommit(index, [...m.values()]);
    pendingEdits.clear();
  }
  /** Flush only `index`'s pending edits now (the ↧ affordance / commit). */
  function flushOne(index: number) {
    const m = pendingEdits.get(index);
    if (!m) return;
    pendingEdits.delete(index);
    nemusStore.requestCommit(index, [...m.values()]);
  }
  /** Commit `edits` straight away, bypassing the debounce (mute → `.gain(0)`).
   *  Any pending debounced edit for the same control is dropped so the two don't
   *  race (the immediate write is the newer intent). */
  function commitNow(index: number, edits: ControlEdit[]) {
    const m = pendingEdits.get(index);
    if (m) for (const e of edits) m.delete(e.kind);
    nemusStore.requestCommit(index, edits);
  }

  /** Mute/unmute strip `i`, writing the source gain (the decision the user made:
   *  mute ⇒ `.gain(0)`, unmute ⇒ restore the pre-mute gain). The live override is
   *  always pushed (instant silence/restore) regardless of whether the source can
   *  be written — when gain is *calculated* the literal can't be rewritten, so we
   *  flip mute live-only and skip the source write (the strip surfaces this). */
  function muteToSource(i: number, mute: boolean) {
    const k = String(i);
    nemusStore.toggleMute(k);
    // Live audio override: silence on mute, restore the (pre-mute or unity) gain
    // on unmute — independent of the source write so mute is instant either way.
    if (mute) void nemusSetTrack('gain', i, 0);

    const calculated = controlsStore.isCalculated(i, 'gain');
    if (mute) {
      // Snapshot the gain to restore on unmute, then write `.gain(0)` — unless gain
      // is calculated (can't be rewritten to a literal): leave the source alone,
      // mute stays live-only. Prefer an un-flushed live override (the user's latest
      // intent) over the source literal, falling back to unity.
      nemusStore.setPremuteGain(k, gains[i] ?? controlsStore.gain(i) ?? GAIN_UNITY);
      if (!calculated) commitNow(i, [{ kind: 'gain', value: 0 }]);
    } else {
      const prev = nemusStore.premuteGain(k) ?? GAIN_UNITY;
      nemusStore.clearPremuteGain(k);
      void nemusSetTrack('gain', i, gains[i] ?? prev); // live: back to pre-mute gain
      if (!calculated) commitNow(i, [{ kind: 'gain', value: prev }]);
    }
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

    // ── gain / pan: live ephemeral override + debounced write-through ──
    // The knob pushes the live audio override now (instant feedback) and schedules
    // a debounced source write — so the value ends up in the `.nemus` on its own.
    gain(i: number) { return gains[i] ?? GAIN_UNITY; },
    pan(i: number)  { return pans[i]  ?? PAN_CENTER; },
    setGain(i: number, v: number) {
      gains = { ...gains, [i]: v };
      void nemusSetTrack('gain', i, v);
      scheduleCommit(i, { kind: 'gain', value: v });
    },
    setPan(i: number, v: number)  {
      pans = { ...pans, [i]: v };
      void nemusSetTrack('pan', i, v);
      scheduleCommit(i, { kind: 'pan', value: v });
    },
    /** Whether strip `i` has a gain/pan write still pending the debounce — the ↧
     *  affordance lets the user flush it early instead of waiting. */
    hasOverride(i: number) { return pendingEdits.has(i); },
    /** Flush strip `i`'s pending gain/pan/room/delay write now (skip the debounce).
     *  The eval that follows re-baselines, so the live override resets and the
     *  value now lives in the `.nemus`. */
    commit(i: number) { flushOne(i); },
    /** Flush every pending write now (Command Palette / shortcut). */
    commitAll() { flushCommits(); },
    /** Number of strips with a pending (un-flushed) write. */
    get overrideCount() { return pendingEdits.size; },

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
    setMasterGain(v: number) { master = v; void nemusSetTrack('master_gain', null, v); },

    // ── mute / solo: shared store (index key) + live override ──
    isMuted(i: number)  { return nemusStore.isMuted(String(i)); },
    isSoloed(i: number) { return nemusStore.isSoloed(String(i)); },
    /** Whether strip `i`'s gain is a calculated argument (`gain(sine.range(…))`) —
     *  it can't be rewritten to a literal, so mute can't persist `.gain(0)` and is
     *  kept live-only (the strip shows it isn't persistible). */
    gainCalculated(i: number) { return controlsStore.isCalculated(i, 'gain'); },
    /** Toggle mute. On mute: write `.gain(0)` into the source (immediate commit)
     *  after snapshotting the current source gain so unmute can restore it; on
     *  unmute: rewrite `.gain(premute)`. Always keeps the live audio override for
     *  instant silence. When gain is calculated the source is left untouched and
     *  only the live override applies. Solo never touches the source. */
    toggleMute(i: number) {
      muteToSource(i, !nemusStore.isMuted(String(i)));
    },
    toggleSolo(i: number) {
      const k = String(i);
      nemusStore.toggleSolo(k);
      void nemusSetTrack('solo', i, nemusStore.isSoloed(k) ? 1 : 0);
    },
    /** Solo computed over the REAL strips only — the shared mute/solo map can
     *  retain keys for indices no longer present after a re-eval with fewer
     *  tracks, which would otherwise dim every visible lane. */
    get anySolo() { return tracks.some((t) => nemusStore.isSoloed(String(t.index))); },
    /** Whether a strip should be visually dimmed (muted, or solo-excluded). */
    isDimmed(i: number) { return this.isMuted(i) || (this.anySolo && !this.isSoloed(i)); },

    // ── selection (mixer ↔ inspector; arrangement joins once 3a wires it) ──
    get selectedIndex(): number | null {
      const id = nemusStore.selectedTrackId;
      if (id == null) return null;
      const n = Number(id);
      return Number.isInteger(n) ? n : null;
    },
    select(i: number) { nemusStore.selectTrack(String(i)); },

    /** Drop all overrides + code-first buffers — call on each eval (the source
     *  is authoritative again: gain/pan re-baseline to neutral, room/delay reflect
     *  the freshly-parsed literals via controlsStore).
     *
     *  The **master gain is NOT reset**: it is mixer-only (no `.nemus`
     *  representation), so the source can't re-supply it. Clearing it would snap
     *  the knob back to unity on every eval/play while the engine keeps the real
     *  value — the value persists across evals and lives only here. */
    rebaseline() {
      gains = {}; pans = {};
      roomBuf = {}; delayBuf = {};
    },
  };
}

export const mixerStore = createMixerStore();
