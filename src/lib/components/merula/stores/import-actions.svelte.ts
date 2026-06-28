/**
 * merula import-action launcher — the single owner of the audio/MIDI import flow
 * (D3/D4/D5). Mirrors `projectActions`: the store holds which picker/dialog is
 * open and runs the IPC; `MerulaImportActions.svelte` renders the modals once for
 * the window. Both the waveform toolbar's Import button and the command palette
 * drive the SAME logic here.
 *
 * Progress: every run is surfaced as a live entry in the shared **Downloads &
 * Exports** overlay (`transfersStore`) — exactly like the WAV export. The FE
 * generates an `opId`, hands it to the backend, and tracks `arbor://job-progress`
 * / `job-done` for that id, so the bar moves in real time (the DSP transcriber
 * streams frame-loop progress) and the terminal state fires a notification.
 *
 * Flow:
 *   start()  → pick a file. MIDI → import as .merula directly (D5). Audio → ask
 *              "Convert to .mid file" (D4) or "Import as .merula" (D3).
 *   startConvert() → pick a WAV, then a .mid output (D4 only) — the palette verb.
 *
 * Results that produce `.merula` text (D3/D5) open in a new tab via the project
 * store's source cache (no disk round-trip until the user saves). The transient
 * MIDI of D3 never touches disk — it lives only inside the backend command.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { projectStore } from './project.svelte';
import { transfersStore } from '$lib/feedback/stores/transfers.svelte';
import {
  merulaImportAudioAsMerula, merulaImportMidiAsMerula, merulaConvertWavToMidi,
} from '$lib/ipc/merula';

/** Extensions the input picker accepts. */
export const AUDIO_EXTS = ['wav', 'wave', 'mp3', 'ogg', 'flac', 'aiff', 'aif'];
export const MIDI_EXTS = ['mid', 'midi'];

interface JobProgress { job_id: string; pct: number; }
interface JobDone { job_id: string; success: boolean; error: string | null; }

/** Lowercase extension (no dot), or '' when there is none. */
function extOf(path: string): string {
  const m = /\.([^.\\/]+)$/.exec(path);
  return m ? m[1].toLowerCase() : '';
}
/** Filename without directory (kept with extension for the transfer label). */
function filename(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
/** Filename without directory or extension. */
function stem(path: string): string {
  return filename(path).replace(/\.[^.]+$/, '');
}
function newOpId(): string {
  return `import-${crypto.randomUUID()}`;
}

type Picker = 'input' | 'midi-out' | null;

/**
 * Track an import op as a transfer-overlay entry keyed by `opId`. Listeners are
 * armed BEFORE `run` so nothing is missed. A *foreground* op (`run` resolves when
 * the work is done — D3/D5) settles success on resolve if `job-done` hasn't
 * already; a *background* op (D4 — `run` returns a job id immediately) waits for
 * `job-done`.
 */
async function track(
  opId: string,
  label: string,
  sublabel: string,
  path: string | undefined,
  run: () => Promise<void>,
  foreground: boolean,
): Promise<void> {
  transfersStore.start({ id: opId, kind: 'import', label, sublabel, progress: 0, path });

  let settled = false;
  let offP: UnlistenFn | null = null;
  let offD: UnlistenFn | null = null;
  const disarm = () => { offP?.(); offD?.(); offP = offD = null; };
  const settle = (ok: boolean, err?: string) => {
    if (settled) return;
    settled = true;
    if (ok) transfersStore.finish(opId);
    else transfersStore.fail(opId, err);
    disarm();
  };

  offP = await listen<JobProgress>('arbor://job-progress', (e) => {
    if (e.payload.job_id === opId) transfersStore.update(opId, { progress: e.payload.pct });
  });
  offD = await listen<JobDone>('arbor://job-done', (e) => {
    if (e.payload.job_id === opId) settle(e.payload.success, e.payload.error ?? undefined);
  });

  try {
    await run();
    if (foreground) settle(true);
  } catch (e) {
    settle(false, e instanceof Error ? e.message : String(e));
    throw e;
  }
}

function createImportActions() {
  let picker     = $state<Picker>(null);
  let choiceFor  = $state<string | null>(null); // audio file awaiting the user's choice
  let pendingWav = $state<string | null>(null); // WAV carried into the .mid save picker
  let convertOnly = false;                        // palette "Convert WAV to MIDI" entry
  let busy       = $state(false);                 // an import (D3/D5) is resolving

  /** Open generated `.merula` text as a new tab (cached, not yet on disk). */
  async function openResult(srcPath: string, text: string) {
    const base = `imported-${stem(srcPath)}.merula`;
    const dir = projectStore.project?.path;
    const path = dir ? `${dir}/${base}` : base;
    projectStore.setSource(path, text);
    await projectStore.openFile(path);
  }

  /** D3 (audio) / D5 (MIDI): transcribe/convert and open the result in a tab. */
  async function runImport(path: string, audio: boolean) {
    busy = true;
    const opId = newOpId();
    try {
      let text = '';
      await track(opId, filename(path), audio ? 'Transcribing…' : 'Reading MIDI…', undefined, async () => {
        text = audio ? await merulaImportAudioAsMerula(path, opId) : await merulaImportMidiAsMerula(path);
      }, true);
      await openResult(path, text);
    } catch {
      /* the transfer entry already reflects (and notifies) the failure */
    } finally {
      busy = false;
    }
  }

  /** D4: transcribe a WAV and write a .mid (background job; reveal on finish). */
  function runConvert(wav: string, out: string) {
    const opId = newOpId();
    void track(opId, filename(out), 'Transcribing…', out, async () => {
      await merulaConvertWavToMidi(wav, out, opId);
    }, false).catch(() => { /* surfaced by the transfer entry */ });
  }

  return {
    get picker()    { return picker; },
    get choiceFor() { return choiceFor; },
    get busy()      { return busy; },

    /** Toolbar Import: pick any audio/MIDI file (D3/D4/D5). */
    start() { convertOnly = false; picker = 'input'; },
    /** Palette "Convert WAV to MIDI": pick a WAV, then a .mid output (D4). */
    startConvert() { convertOnly = true; picker = 'input'; },

    /** Confirm of the input picker. */
    onInput(path: string) {
      picker = null;
      const isMidi = MIDI_EXTS.includes(extOf(path));
      const wantConvert = convertOnly;
      convertOnly = false;
      if (isMidi) {
        // A .mid never needs transcription, even via the "convert" verb.
        void runImport(path, false);
      } else if (wantConvert) {
        pendingWav = path;
        picker = 'midi-out';
      } else {
        choiceFor = path; // ask: .mid file or .merula
      }
    },

    /** Audio choice → D4 (write a .mid file). */
    chooseConvert() { pendingWav = choiceFor; choiceFor = null; picker = 'midi-out'; },
    /** Audio choice → D3 (import as .merula). */
    chooseImport()  { const wav = choiceFor; choiceFor = null; if (wav) void runImport(wav, true); },

    /** Confirm of the .mid save picker (D4). */
    onMidiOut(out: string) {
      picker = null;
      const wav = pendingWav;
      pendingWav = null;
      if (wav) runConvert(wav, out);
    },

    /** Suggested .mid filename for the save picker. */
    midiOutName(): string { return pendingWav ? `${stem(pendingWav)}.mid` : 'transcription.mid'; },

    cancel() { picker = null; choiceFor = null; pendingWav = null; convertOnly = false; },
  };
}

export const importActions = createImportActions();
