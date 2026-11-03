/**
 * A log file followed in the console, instead of the program's own output.
 *
 * The backend reads the file the way it reads a run — every line comes back interpreted (level, links to
 * the source, SQL, stack frames) — so the console draws it with everything it already does and this store
 * only holds the lines and which file they are from.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { logFiles, logTailStart, logTailStop, type LogFileInfo } from '$lib/ipc/bennu/log-view';
import type { RunLogLine } from '$lib/stores/bennu/run.svelte';
import type { LogLevel, LogPiece } from '$lib/types/log';

/** Lines kept: the end of a log is what is read, and an hour of a busy one is more than a console can hold. */
const MAX_LINES = 20_000;

interface TailEvent {
  id: string;
  reset: boolean;
  lines: { text: string; level?: LogLevel | null; pieces?: LogPiece[] }[];
}

function createLogTailStore() {
  let files = $state<LogFileInfo[] | null>(null);
  let source = $state<{ path: string; name: string } | null>(null);
  let lines = $state<RunLogLine[]>([]);
  let error = $state<string | null>(null);
  let id = '';
  let unlisten: UnlistenFn | null = null;
  /** True between asking the backend to follow and learning the id it chose: the first lines can arrive
   *  before the answer does, and are kept here until the id says whose they are. */
  let starting = false;
  let early: TailEvent[] = [];

  function apply(e: TailEvent) {
    if (e.id !== id) return;
    if (e.reset) lines = [];
    if (!e.lines.length) return;
    const added = e.lines.map((l): RunLogLine => ({ text: l.text, stream: 'out', level: l.level, pieces: l.pieces }));
    const all = lines.concat(added);
    lines = all.length > MAX_LINES ? all.slice(all.length - MAX_LINES) : all;
  }

  async function listening() {
    unlisten ??= await listen<TailEvent>('arbor://bennu/log-tail', (e) => {
      if (starting) early.push(e.payload);
      else apply(e.payload);
    });
  }

  async function stop() {
    const old = id;
    id = '';
    if (old) await logTailStop(old).catch(() => {});
  }

  return {
    /** The log files found in the project, or `null` before they were looked for. */
    get files() { return files; },
    /** The file being followed, if any. */
    get source() { return source; },
    get active() { return source !== null; },
    get lines() { return lines; },
    get error() { return error; },

    async refresh(root: string | undefined) {
      if (!root) { files = []; return; }
      try { files = await logFiles(root); } catch { files = []; }
    },

    /** Follow `path`, replacing whatever was followed before. */
    async open(root: string, path: string, name: string) {
      await stop();
      await listening();
      lines = [];
      error = null;
      source = { path, name };
      early = [];
      starting = true;
      try {
        id = await logTailStart(root, path);
      } catch (e) {
        error = String(e);
        source = null;
      } finally {
        starting = false;
        for (const e of early) apply(e);
        early = [];
      }
    },

    /** Back to the program's own output. */
    async close() {
      await stop();
      source = null;
      lines = [];
      error = null;
    },
  };
}

export const bennuLogTailStore = createLogTailStore();
