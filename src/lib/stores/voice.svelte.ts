/**
 * Voice store — the one place that knows whether the microphone is open.
 *
 * Product-agnostic: Bennu's composer is the first consumer, anything else can subscribe with
 * {@link onTranscript} / {@link onPartial}. Everything that wants the user's speech — a button, a
 * shortcut, the wake word — goes through {@link startListening}; the trigger is deliberately abstract.
 *
 * Settings live in `voice-be`'s config file (profile directory), never in localStorage; the
 * store only mirrors them, and `voice://config` keeps the mirror current when the backend changes a
 * setting itself (a model finishing its install becomes the selected one). The backend is started
 * on first use, so nothing here talks to it until something is asked of it.
 *
 * It can never stay "transcribing" forever: the backend bounds every run, and the store adds its
 * own deadline on top (see {@link transcribeDeadlineMs}) that resets the state and cancels.
 *
 * Rune-store pattern: private `$state`, returned getters + methods.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  voiceApi, type VoiceConfig, type VoiceReason, type VoiceStatus, type VoiceTranscript,
} from '$lib/ipc/voice';
import { createDownloadMeter, type DownloadStats } from '$lib/utils/download-progress';
import { transcribeDeadlineMs } from '$lib/utils/voice-format';

/** Where a finished transcript goes: the composer's caret, or the voice overlay's request. */
export type VoiceTarget = 'composer' | 'overlay';

type TranscriptListener = (t: VoiceTranscript, target: VoiceTarget) => void;
/** A piece of live text: `text` is everything heard so far in this dictation. */
type PartialListener = (p: { id: number; text: string }, target: VoiceTarget) => void;

/** A model's download as the page shows it: the stats the meter measured (percentage, left, speed, ETA), or the error. */
type InstallProgress = (DownloadStats | { done: number; total: null; percent: null; remaining: null; speed: null; etaSeconds: null }) & { error?: string };

const failed = (message: string | undefined): InstallProgress =>
  ({ done: 0, total: null, percent: null, remaining: null, speed: null, etaSeconds: null, error: message });

/** "Test it" on one model: recording, then transcribing, then a result. */
export interface ModelTest {
  modelId: string;
  phase: 'recording' | 'transcribing';
  startedAt: number;
  seconds: number;
}

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));
const CANCELLED = 'cancelled';

function createVoiceStore() {
  let config = $state<VoiceConfig | null>(null);
  let status = $state<VoiceStatus | null>(null);
  let level = $state(0);
  let listening = $state(false);
  let reason = $state<VoiceReason | null>(null);
  let target = $state<VoiceTarget>('composer');
  let transcribing = $state(false);
  let armed = $state(false);
  let lastTranscript = $state<VoiceTranscript | null>(null);
  let error = $state<string | null>(null);
  let installs = $state<Record<string, InstallProgress>>({});
  let installedTick = $state(0);
  /** Everything heard so far in the current dictation (live text), "" before the first piece. */
  let liveText = $state('');
  /** The current dictation shows live text; otherwise `liveNote` says why not. */
  let live = $state(false);
  let liveNote = $state<string | null>(null);
  let test = $state<ModelTest | null>(null);
  let testResult = $state<{ modelId: string; transcript: VoiceTranscript } | null>(null);

  // One meter per model being downloaded: the speed needs the readings that came before this one.
  const meters = new Map<string, ReturnType<typeof createDownloadMeter>>();
  const listeners = new Set<TranscriptListener>();
  const partialListeners = new Set<PartialListener>();
  const wakeListeners = new Set<() => void>();
  let unlisten: UnlistenFn[] = [];
  let wired = false;
  let recordedSecs = 0;
  let recordStart = 0;
  let deadline: ReturnType<typeof setTimeout> | null = null;

  function clearDeadline() { if (deadline) { clearTimeout(deadline); deadline = null; } }

  /** Arm the safety net for a transcription that should finish within `secs` of audio's budget. */
  function armDeadline(audioSecs: number) {
    clearDeadline();
    deadline = setTimeout(() => {
      deadline = null;
      if (!transcribing) return;
      transcribing = false;
      test = null;
      error = 'The transcription took too long and was stopped.';
      void voiceApi.cancel().catch(() => {});
    }, transcribeDeadlineMs(audioSecs));
  }

  function applyState(s: { state: string; armed: boolean; streaming?: boolean; note?: string | null }) {
    const wasTranscribing = transcribing;
    listening = s.state === 'listening';
    transcribing = s.state === 'transcribing';
    armed = s.armed;
    if (listening) { live = !!s.streaming; liveNote = s.note ?? null; }
    if (!listening) level = 0;
    if (transcribing && !wasTranscribing) armDeadline(recordedSecs || 10);
    if (!transcribing) clearDeadline();
    if (test && transcribing) test = { ...test, phase: 'transcribing' };
  }

  async function wire() {
    if (wired) return;
    wired = true;
    unlisten = await Promise.all([
      listen<{ state: string; armed: boolean; streaming?: boolean; note?: string | null }>('voice://state', (e) => applyState(e.payload)),
      listen<{ level: number }>('voice://level', (e) => { level = e.payload.level; }),
      listen<{ id: number; text: string }>('voice://partial', (e) => {
        liveText = e.payload.text;
        for (const l of partialListeners) l(e.payload, target);
      }),
      listen<VoiceTranscript>('voice://transcript', (e) => {
        lastTranscript = e.payload;
        transcribing = false;
        clearDeadline();
        if (e.payload.error && e.payload.error !== CANCELLED) error = e.payload.error;
        for (const l of listeners) l(e.payload, target);
      }),
      // A setting changed behind the page's back (a model finished installing and became the
      // selected one): mirror it, and drop the notices that described the old state.
      listen<VoiceConfig>('voice://config', (e) => {
        config = e.payload;
        error = null;
        void refreshStatus();
        installedTick += 1;
      }),
      // The wake word heard its phrase: the request that follows belongs to the overlay.
      listen('voice://wake', () => { target = 'overlay'; reason = 'wake'; for (const l of wakeListeners) l(); }),
      listen<{ message: string }>('voice://error', (e) => { error = e.payload.message; }),
      listen<{ why: string }>('voice://disarmed', () => { armed = false; }),
      listen<{ id: string; kind: string; done?: number; total?: number | null; error?: string }>('voice://install', (e) => {
        const p = e.payload;
        if (p.kind === 'progress') {
          const meter = meters.get(p.id) ?? createDownloadMeter();
          meters.set(p.id, meter);
          installs = { ...installs, [p.id]: meter.push(p.done ?? 0, p.total ?? null) };
        } else {
          meters.delete(p.id);
          const { [p.id]: _gone, ...rest } = installs;
          installs = p.kind === 'error' ? { ...rest, [p.id]: failed(p.error) } : rest;
          if (p.kind === 'done') error = null;
          installedTick += 1;
          void refreshStatus();
        }
      }),
      // voice-be exits when idle: forget everything it knew.
      listen('arbor://voice-be-down', () => {
        listening = false; transcribing = false; armed = false; level = 0; status = null; test = null;
        clearDeadline();
      }),
    ]);
  }

  async function refreshStatus() {
    try { status = await voiceApi.status(); } catch { /* backend down: the next call restarts it */ }
  }

  return {
    get config() { return config; },
    get status() { return status; },
    get level() { return level; },
    get listening() { return listening; },
    get reason() { return reason; },
    get target() { return target; },
    get transcribing() { return transcribing; },
    get armed() { return armed; },
    get busy() { return listening || transcribing; },
    get lastTranscript() { return lastTranscript; },
    get error() { return error; },
    get installs() { return installs; },
    /** Bumps whenever a model finished installing / was selected / failed, so lists know to refetch. */
    get installedTick() { return installedTick; },
    /** Live text of the current dictation (everything heard so far). */
    get liveText() { return liveText; },
    /** The current dictation shows text while the user speaks. */
    get live() { return live; },
    /** Why live text is off although it is enabled (the model is too slow for this PC). */
    get liveNote() { return liveNote; },
    get test() { return test; },
    get testResult() { return testResult; },
    clearError() { error = null; },

    /** Subscribe to the wake word being heard (open the overlay here). */
    onWake(fn: () => void): () => void {
      wakeListeners.add(fn);
      return () => wakeListeners.delete(fn);
    },

    /** Subscribe to every finished transcript. Returns the unsubscribe function. */
    onTranscript(fn: TranscriptListener): () => void {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },

    /** Subscribe to live text pieces. Returns the unsubscribe function. */
    onPartial(fn: PartialListener): () => void {
      partialListeners.add(fn);
      return () => partialListeners.delete(fn);
    },

    /** Read the settings (starts voice-be — call it when a voice surface opens, not at boot). */
    async loadConfig() {
      await wire();
      try {
        config = await voiceApi.getConfig();
        status = await voiceApi.status();
        armed = status.armed;
        // A notice about a missing model is stale the moment a model is in use.
        if (status.model_ready) error = null;
      } catch (e) { error = errorText(e); }
    },

    /** Change some settings; only the named fields are sent, so nothing newer is overwritten. */
    async saveConfig(patch: Partial<VoiceConfig>) {
      try { config = await voiceApi.setConfig(patch); } catch (e) { error = errorText(e); }
    },

    /** Make an installed model the one in use — applied at once, no restart. */
    async selectModel(id: string): Promise<boolean> {
      try {
        config = await voiceApi.selectModel(id);
        error = null;
        installedTick += 1;
        await refreshStatus();
        return true;
      } catch (e) { error = errorText(e); return false; }
    },

    /** The abstract trigger: a button, a shortcut or the wake word all call this. */
    async startListening(why: VoiceReason, to: VoiceTarget = 'composer'): Promise<boolean> {
      await wire();
      error = null;
      target = to;
      liveText = '';
      // Re-ask the backend: a model may have been installed or selected since the last answer.
      await refreshStatus();
      if (status && !status.engine) { error = 'This build has no speech engine.'; return false; }
      if (status && !status.model_ready) { error = 'No speech model is selected. Choose one in Settings › Voice.'; return false; }
      try {
        await voiceApi.start(why);
        reason = why;
        listening = true;
        recordStart = performance.now();
        return true;
      } catch (e) {
        error = errorText(e);
        return false;
      }
    },

    /** End the recording and wait for the text. `null` when nothing was recording or it failed. */
    async stopListening(): Promise<VoiceTranscript | null> {
      recordedSecs = (performance.now() - recordStart) / 1000;
      try {
        transcribing = true;
        listening = false;
        armDeadline(recordedSecs);
        return await voiceApi.stop();
      } catch (e) {
        error = errorText(e);
        return null;
      } finally {
        transcribing = false;
        clearDeadline();
        reason = null;
      }
    },

    /** Stop everything: a recording is dropped, a transcription in progress gives up. */
    async cancel() {
      listening = false;
      transcribing = false;
      test = null;
      reason = null;
      clearDeadline();
      try { await voiceApi.cancel(); } catch { /* nothing was running */ }
    },

    async toggle(to: VoiceTarget = 'composer'): Promise<VoiceTranscript | null> {
      if (listening) return this.stopListening();
      await this.startListening('toggle', to);
      return null;
    },

    /** "Test it": record a few seconds with `modelId`, transcribe, remember the measured speed. */
    async runTest(modelId: string, seconds = 5) {
      if (test || listening || transcribing) return;
      await wire();
      error = null;
      testResult = null;
      test = { modelId, phase: 'recording', startedAt: Date.now(), seconds };
      recordedSecs = seconds;
      try {
        const t = await voiceApi.test(modelId, seconds);
        if (t.error !== CANCELLED) testResult = { modelId, transcript: t };
      } catch (e) { error = errorText(e); }
      finally { test = null; transcribing = false; clearDeadline(); installedTick += 1; }
    },

    /** The Stop button of a running test. */
    async stopTest() { await this.cancel(); },

    async arm() {
      await wire();
      error = null;
      try { await voiceApi.arm(); armed = true; } catch (e) { error = errorText(e); }
    },
    async disarm() {
      try { await voiceApi.disarm(); } catch { /* backend already gone */ }
      armed = false;
    },
    async toggleArm() { if (armed) await this.disarm(); else await this.arm(); },

    destroy() { unlisten.forEach((u) => u()); unlisten = []; wired = false; clearDeadline(); },
  };
}

export const voiceStore = createVoiceStore();
