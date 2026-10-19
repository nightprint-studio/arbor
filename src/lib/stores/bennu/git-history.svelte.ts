/**
 * Bennu's Git history dialog — the commits that touched a file, or a selection of its lines.
 *
 * Distinct from Local History on purpose: that is Bennu's own record of every save, underneath
 * git; this is what git itself remembers, and Corvus answers it (by file path, no tab involved).
 * Two entry points feed the one dialog — *Show History* (the file) and *Show History for
 * Selection* (a line range, tracked through moves by `git log -L`) — because they share the
 * list, the diff and the keyboard handling and differ only in the question.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import {
  getFileCommitPatchByPath,
  getFileHistoryByPath,
  getLineHistoryByPath,
  type FileHistoryEntry,
  type LineHistoryEntry,
} from '$lib/ipc/corvus/fileHistory';
import type { DiffHunkModel } from '$lib/components/shared/ui/TextDiff.svelte';
import { parseUnifiedPatch } from '$lib/utils/unified-patch';

export type GitHistoryMode = 'file' | 'selection';

function createGitHistoryStore() {
  let open = $state(false);
  let mode = $state<GitHistoryMode>('file');
  let file = $state('');
  let startLine = $state(0);
  let endLine = $state(0);

  // Replaced wholesale, never mutated — a long history must not become a deep reactive proxy.
  let entries = $state.raw<(FileHistoryEntry | LineHistoryEntry)[]>([]);
  let selected = $state<string | null>(null);
  let hunks = $state.raw<DiffHunkModel[]>([]);
  let loading = $state(false);
  let diffing = $state(false);
  let error = $state('');

  /** Out-of-order guards: the list and the diff are separate requests, each of which a newer
   *  one must be able to supersede. Two counters for the reason `history.svelte.ts` documents:
   *  loading the list *selects* a commit, so one shared counter would cancel its own load. */
  let loadToken = 0;
  let diffToken = 0;

  function reset(nextMode: GitHistoryMode, path: string, start = 0, end = 0) {
    open = true;
    mode = nextMode;
    file = path;
    startLine = start;
    endLine = end;
    entries = [];
    selected = null;
    hunks = [];
    error = '';
  }

  async function load() {
    const mine = ++loadToken;
    loading = true;
    try {
      const list = mode === 'selection'
        ? await getLineHistoryByPath(file, startLine, endLine)
        : await getFileHistoryByPath(file);
      if (mine !== loadToken) return;
      entries = list;
      if (list[0]) void select(list[0].commit);
    } catch (e) {
      if (mine === loadToken) error = e instanceof Error ? e.message : String(e);
    } finally {
      if (mine === loadToken) loading = false;
    }
  }

  async function select(commit: string) {
    selected = commit;
    const mine = ++diffToken;
    const entry = entries.find((e) => e.commit === commit);
    if (!entry) return;
    // A selection's history already carries its patch; a file's is asked for on demand.
    if ('patch' in entry) {
      hunks = parseUnifiedPatch(entry.patch);
      return;
    }
    hunks = [];
    diffing = true;
    try {
      const patch = await getFileCommitPatchByPath(file, commit);
      if (mine === diffToken) hunks = parseUnifiedPatch(patch);
    } catch (e) {
      if (mine === diffToken) error = e instanceof Error ? e.message : String(e);
    } finally {
      if (mine === diffToken) diffing = false;
    }
  }

  return {
    get open() { return open; },
    get mode() { return mode; },
    get file() { return file; },
    get startLine() { return startLine; },
    get endLine() { return endLine; },
    get entries() { return entries; },
    get selected() { return selected; },
    get hunks() { return hunks; },
    get loading() { return loading; },
    get diffing() { return diffing; },
    get error() { return error; },

    /** Open the dialog on a whole file. */
    showFile(path: string) {
      reset('file', path);
      void load();
    },

    /** Open the dialog on lines `start..end` (1-based) of a file. */
    showSelection(path: string, start: number, end: number) {
      reset('selection', path, start, end);
      void load();
    },

    select,

    close() {
      open = false;
      loadToken++;
      diffToken++;
    },
  };
}

export const bennuGitHistoryStore = createGitHistoryStore();
