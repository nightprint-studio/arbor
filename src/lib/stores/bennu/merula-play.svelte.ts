/**
 * Play the open `.merula` file from Bennu.
 *
 * No audio logic lives here: this drives the very handlers the Merula window does (`merula_eval`,
 * `merula_transport`) on `merula-be`, which Bennu *borrows* — `ensureBackend('merula')` starts it on
 * the first press of play and the shell releases it when Bennu closes. What Bennu adds is the
 * console: one tab per play, carrying the backend's log lines, the evaluation errors (as
 * `file:line:col` links) and how it ended.
 *
 * One file plays at a time, because the engine has one arrangement. Pressing play on another file
 * replaces the first, exactly as it does in the Merula window.
 */

import { ensureBackend } from '$lib/ipc/rpc';
import {
  merulaEval, merulaPlay, merulaStop, onMerulaLog, onMerulaAudioError,
  type MerulaLogLine,
} from '$lib/ipc/merula/merula';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { bennuRunStore } from './run.svelte';
import { projectStore } from './project.svelte';
import { consoleDiagnostic } from '$lib/components/bennu/merula-diagnostics';

const RUNNER = 'merula';

export const isMerulaPath = (path: string | null | undefined): boolean => !!path && /\.merula$/i.test(path);

/** The engine's log level onto the console's streams. */
export function streamForLevel(level: string): 'out' | 'err' | 'meta' {
  switch (level) {
    case 'error': return 'err';
    case 'warn': return 'err';
    case 'trace':
    case 'debug': return 'meta';
    default: return 'out';
  }
}

function createMerulaPlayStore() {
  /** The file being played, or null. */
  let playing = $state<string | null>(null);
  let tabId: string | null = null;
  let unlisten: UnlistenFn[] = [];
  /** Serialises play/stop: a second press while the first is still evaluating must not interleave. */
  let busy = false;

  const projectDir = (path: string): string => {
    const root = projectStore.project?.root;
    if (root && path.replace(/\\/g, '/').startsWith(root.replace(/\\/g, '/'))) return root;
    return path.replace(/[\\/][^\\/]*$/, '');
  };

  async function listen() {
    unlisten = await Promise.all([
      onMerulaLog((l: MerulaLogLine) => {
        if (tabId) bennuRunStore.externalLine(tabId, `[${l.level}] ${l.message}`, streamForLevel(l.level));
      }),
      onMerulaAudioError((e) => {
        if (!tabId) return;
        bennuRunStore.externalLine(tabId, `Audio error: ${e.message}`, 'err');
        finish({ failed: true });
      }),
    ]);
  }

  function unlistenAll() {
    for (const u of unlisten) u();
    unlisten = [];
  }

  function finish(opts: { stopped?: boolean; failed?: boolean }) {
    if (tabId) bennuRunStore.endExternal(tabId, opts);
    tabId = null;
    playing = null;
    unlistenAll();
  }

  /** Evaluate the file's buffer and, if it is clean, start the transport. */
  async function play(path: string): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      if (playing) await halt();
      const source = projectStore.sourceOf(path);
      const name = path.split(/[\\/]/).pop() ?? path;
      tabId = bennuRunStore.beginExternal(RUNNER, path, name, 'Merula');
      playing = path;
      bennuRunStore.externalLine(tabId, `Playing ${path}`, 'meta');
      try {
        await ensureBackend('merula');
        await listen();
        const result = await merulaEval(source, projectDir(path));
        for (const err of result.errors) {
          const d = consoleDiagnostic(path, source, err);
          bennuRunStore.externalLine(tabId, d.text, err.severity === 'error' ? 'err' : 'out', d.pieces ? { pieces: d.pieces } : undefined);
        }
        const hasErrors = result.errors.some((e) => e.severity === 'error');
        // An evaluation that staged nothing has nothing to play. One that did — its errors are findings
        // such as an instrument the sound registry lacks — plays, as it does in the Merula window.
        if (hasErrors && !result.staged) {
          finish({ failed: true });
          return;
        }
        if (hasErrors) {
          bennuRunStore.externalLine(tabId, 'Playing anyway — the engine falls back to its synth for what it cannot find.', 'meta');
        }
        await merulaPlay();
        bennuRunStore.externalLine(tabId, 'Playing — press Stop to end.', 'meta');
      } catch (e) {
        if (tabId) bennuRunStore.externalLine(tabId, `Could not play: ${e instanceof Error ? e.message : String(e)}`, 'err');
        finish({ failed: true });
      }
    } finally {
      busy = false;
    }
  }

  async function halt(): Promise<void> {
    try { await merulaStop(); } catch { /* the backend may already be gone — the tab ends either way */ }
    finish({ stopped: true });
  }

  async function stop(): Promise<void> {
    if (!playing || busy) return;
    busy = true;
    try { await halt(); } finally { busy = false; }
  }

  bennuRunStore.registerExternalRunner(RUNNER, { rerun: play, stop });

  // Closing Bennu must not leave music playing out of a backend another window still holds.
  if (typeof window !== 'undefined') {
    window.addEventListener('pagehide', () => { if (playing) void merulaStop().catch(() => {}); });
  }

  return {
    /** The file currently playing, or null. */
    get playing() { return playing; },
    isPlaying(path: string | null | undefined) { return !!path && playing === path; },
    play,
    stop,
    /** Play the file if it is stopped, stop it if it is the one playing — what the shortcut does. */
    async toggle(path: string) { if (playing === path) await stop(); else await play(path); },
  };
}

export const merulaPlayStore = createMerulaPlayStore();
