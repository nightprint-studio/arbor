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

import { nemusSetTrack, nemusSetReverb, getNemusProjectMix, setNemusProjectMix } from '$lib/ipc/nemus';
import { arrangementStore, noteName, type VizLane } from '../viz/arrangement.svelte';
import { projectStore } from './project.svelte';
import { nemusStore } from '../nemus-store.svelte';
import { controlsStore } from './controls.svelte';
import { laneColor } from '../palette';
import {
  DELAY_DEFAULT_FB, DELAY_DEFAULT_MIX, EQ_DEFAULT_BAND, COMP_DEFAULTS,
  type ControlEdit, type DelayValues, type EqBandValue, type CompValues,
} from '../editor/nemus-edit';

/** Neutral baselines — a fresh strip is unity gain, centre pan. */
export const GAIN_UNITY = 1;
export const PAN_CENTER = 0.5;
/** A fresh delay (knob seed when the source has none): a quarter-cycle echo. */
export const DELAY_DEFAULT: DelayValues = { t: 0.25, fb: DELAY_DEFAULT_FB, mix: DELAY_DEFAULT_MIX };
/** Default shared reverb-return decay in seconds — mirrors the renderer's
 *  `DEFAULT_REVERB_SECS`, so the knob reads true before the first change. */
export const REVERB_DECAY_DEFAULT = 0.5;

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
  /** Peak simultaneous voices (static polyphony) — the track's voice cost. */
  polyphony: number;
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
  // Shared reverb-return decay (seconds). Like `master`, this has no `.nemus`
  // source representation, so it's NOT cleared on rebaseline — but it IS persisted
  // per-project (`.nemus/mix.json`) so the master mix survives a reopen.
  let reverbDecay = $state(REVERB_DECAY_DEFAULT);
  // Debounce for the (allocating) reverb-IR rebuild — see `setReverbDecay`.
  let reverbTimer: ReturnType<typeof setTimeout> | null = null;
  // Debounced persist of the master mix (master gain + reverb decay) to the open
  // project's `.nemus/mix.json`. Suppressed while loading a project's saved mix.
  let mixPersistTimer: ReturnType<typeof setTimeout> | null = null;
  let loadingMix = false;
  function persistMix() {
    if (loadingMix) return;
    const path = projectStore.project?.path;
    if (!path) return;
    if (mixPersistTimer) clearTimeout(mixPersistTimer);
    mixPersistTimer = setTimeout(() => {
      mixPersistTimer = null;
      void setNemusProjectMix(path, { master_gain: master, reverb_decay: reverbDecay }).catch(() => {});
    }, 300);
  }

  // Code-first knob buffers (index → value). These mirror the SOURCE literal
  // (seeded from controlsStore) and commit back to it; the buffer just holds the
  // value mid-drag until the eval/re-parse round-trip catches up. Cleared on
  // rebaseline (the source becomes authoritative again).
  let roomBuf  = $state<Record<number, number>>({});
  let delayBuf = $state<Record<number, DelayValues>>({});
  // EQ / compressor (strip inserts) — code-first like room/delay. `eqBuf` holds the
  // full band list mid-edit; `compBuf` the six compressor params.
  let eqBuf    = $state<Record<number, EqBandValue[]>>({});
  let compBuf  = $state<Record<number, CompValues>>({});

  // Debounced commit: a knob drag fires many onchange; commit (which re-evals)
  // once the gesture settles. Holds the latest pending edit per (index,control)
  // so gain/pan/room/delay drags all coalesce into one write per gesture.
  let commitTimer: ReturnType<typeof setTimeout> | null = null;
  const pendingEdits = new Map<number, Map<string, ControlEdit>>();
  /** Coalesce key: per-band for EQ (so different bands don't clobber each other),
   *  else the control kind. */
  function commitKey(e: ControlEdit): string {
    return e.kind === 'eqBand' ? `eqBand:${e.band}` : e.kind;
  }
  function scheduleCommit(index: number, edit: ControlEdit) {
    let m = pendingEdits.get(index);
    if (!m) { m = new Map(); pendingEdits.set(index, m); }
    m.set(commitKey(edit), edit);
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
    if (m) for (const e of edits) m.delete(commitKey(e));
    nemusStore.requestCommit(index, edits);
  }
  /** Drop any pending (debounced) EQ-band edits for a track. Called before a
   *  structural EQ change (add / remove) re-indexes the bands, so a queued
   *  per-band rewrite can't land on the wrong band after the re-parse. */
  function dropPendingEq(index: number) {
    const m = pendingEdits.get(index);
    if (!m) return;
    for (const k of [...m.keys()]) if (k.startsWith('eqBand:')) m.delete(k);
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
      polyphony: l.polyphony,
    }));
  });

  // The track carrying the most simultaneous voices — the first place to look when
  // the voice/CPU budget runs hot. Null until an arrangement is loaded; ties keep
  // the lowest index (stable).
  const heaviestTrack = $derived.by<MixerTrack | null>(() => {
    let best: MixerTrack | null = null;
    for (const t of tracks) if (t.polyphony > 0 && (!best || t.polyphony > best.polyphony)) best = t;
    return best;
  });

  return {
    get tracks() { return tracks; },
    /** The track with the highest peak polyphony (the heaviest voice load), or null. */
    get heaviestTrack() { return heaviestTrack; },
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

    // ── EQ / compressor: code-first strip inserts (seed from source) ──
    /** Parametric-EQ bands for track `i` (mid-edit buffer, else source). */
    eq(i: number): EqBandValue[] { return eqBuf[i] ?? controlsStore.eq(i); },
    eqActive(i: number) { return this.eq(i).length > 0; },
    /** Edit one parameter of band `b` (merging the band), committing in place. */
    setEqBand(i: number, b: number, patch: Partial<EqBandValue>) {
      const cur = this.eq(i).map((band) => ({ ...band }));
      if (!cur[b]) return;
      cur[b] = { ...cur[b], ...patch };
      eqBuf = { ...eqBuf, [i]: cur };
      scheduleCommit(i, { kind: 'eqBand', band: b, value: cur[b] });
    },
    /** Append a fresh band at the chain tail (immediate — a structural change). */
    addEqBand(i: number) {
      dropPendingEq(i);
      const band = { ...EQ_DEFAULT_BAND };
      eqBuf = { ...eqBuf, [i]: [...this.eq(i), band] };
      commitNow(i, [{ kind: 'eqAdd', value: band }]);
    },
    /** Remove band `b` (immediate — a structural change). */
    removeEqBand(i: number, b: number) {
      dropPendingEq(i);
      eqBuf = { ...eqBuf, [i]: this.eq(i).filter((_, k) => k !== b) };
      commitNow(i, [{ kind: 'eqRemove', band: b }]);
    },

    /** Compressor settings for track `i` (mid-edit buffer, else source). */
    comp(i: number): CompValues | null { return compBuf[i] ?? controlsStore.comp(i); },
    compActive(i: number) { return compBuf[i] != null || controlsStore.comp(i) != null; },
    compCalculated(i: number) { return controlsStore.isCalculated(i, 'comp'); },
    /** Set one compressor parameter, merging with the current settings, and commit. */
    setCompParam(i: number, key: keyof CompValues, v: number) {
      const next = { ...(this.comp(i) ?? COMP_DEFAULTS), [key]: v };
      compBuf = { ...compBuf, [i]: next };
      scheduleCommit(i, { kind: 'comp', value: next });
    },
    /** Add a compressor with defaults (when the track has none). */
    addComp(i: number) {
      const next = { ...COMP_DEFAULTS };
      compBuf = { ...compBuf, [i]: next };
      scheduleCommit(i, { kind: 'comp', value: next });
    },
    /** Remove the compressor (immediate — a structural change). */
    removeComp(i: number) {
      const m = pendingEdits.get(i);
      if (m) m.delete('comp');
      const next = { ...compBuf };
      delete next[i];
      compBuf = next;
      commitNow(i, [{ kind: 'compRemove' }]);
    },

    get masterGain() { return master; },
    setMasterGain(v: number) { master = v; void nemusSetTrack('master_gain', null, v); persistMix(); },

    // ── Master mix persistence (per-project; master gain + reverb have no source) ──
    /** Load the project's saved master mix and push it to the (possibly not-yet-open)
     *  session. Call on project open. A missing file yields the defaults. */
    async loadMix(projectPath: string) {
      loadingMix = true;
      try {
        const mix = await getNemusProjectMix(projectPath);
        master = mix.master_gain;
        reverbDecay = mix.reverb_decay;
      } catch {
        master = GAIN_UNITY;
        reverbDecay = REVERB_DECAY_DEFAULT;
      } finally {
        loadingMix = false;
      }
      this.syncMaster();
    },
    /** Re-push the master gain + reverb decay to the running session. Called after a
     *  load and on play-start (both are session-only on the BE, so a value set while
     *  stopped / before the device opened must be re-sent). No-op audio when stopped. */
    syncMaster() {
      void nemusSetTrack('master_gain', null, master);
      void nemusSetReverb(reverbDecay);
    },

    // ── reverb return: shared bus decay (global, session-only, like master gain) ──
    get reverbDecay() { return reverbDecay; },
    /** Set the reverb decay. The knob updates instantly, but the audio command is
     *  DEBOUNCED: changing the procedural IR rebuilds the reverb (an allocation on
     *  the audio thread), so a drag must not fire it per frame — only on settle. */
    setReverbDecay(v: number) {
      reverbDecay = v;
      if (reverbTimer) clearTimeout(reverbTimer);
      reverbTimer = setTimeout(() => { reverbTimer = null; void nemusSetReverb(v); }, 140);
      persistMix();
    },
    /** Each track's reverb send (its `room` value), for the return-bus visual. */
    roomSend(i: number) { return this.room(i); },

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
      eqBuf = {}; compBuf = {};
    },
  };
}

export const mixerStore = createMixerStore();
