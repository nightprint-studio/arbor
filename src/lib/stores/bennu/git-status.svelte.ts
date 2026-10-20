/**
 * Git status for Bennu's file tree: which rows are modified, newly added, untracked, in conflict —
 * the files that are what a push would carry.
 *
 * Corvus answers it (one call for the whole repository, by path); this store keeps the answer,
 * rolls it up so a folder knows it holds changes, and decides when a new answer is worth asking
 * for. It refreshes on its own schedule rather than per row: a request per folder is how a tree
 * of a thousand rows becomes a thousand status walks.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { getGitSummary, getGitTreeStatus, type GitSummary } from '$lib/ipc/corvus/treeStatus';
import type { GitBadge } from '$lib/ipc/fs';
import { strongerBadge } from '$lib/utils/git-badge';

/** Quiet period before a requested refresh runs, so a burst (a save, a tree reload, a focus) is one walk. */
const REFRESH_DEBOUNCE_MS = 400;

/** One path spelling for lookups: forward slashes, no trailing slash, and the drive letter —
 *  which Windows hands out in either case — lowered so `C:/x` and `c:/x` are one key. */
function key(path: string): string {
  const p = path.replace(/\\/g, '/').replace(/\/+$/, '');
  return /^[A-Za-z]:\//.test(p) ? p[0].toLowerCase() + p.slice(1) : p;
}

function parentOf(k: string): string {
  const i = k.lastIndexOf('/');
  return i <= 0 ? '' : k.slice(0, i);
}

function createGitStatusStore() {
  let root = $state('');
  let inRepo = $state(false);
  let branch = $state<string | null>(null);
  let summary = $state.raw<GitSummary | null>(null);
  // Replaced wholesale, never mutated: a big working tree must not become a deep reactive proxy.
  let files = $state.raw<Record<string, GitBadge>>({});
  /** Folder → the strongest badge beneath it. */
  let folders = $state.raw<Record<string, GitBadge>>({});

  let timer: ReturnType<typeof setTimeout> | undefined;
  let token = 0;

  async function load(forRoot: string) {
    const mine = ++token;
    try {
      // Two cheap walks in parallel; the summary's failure must not blank the tree marks.
      const [status, sum] = await Promise.all([
        getGitTreeStatus(forRoot),
        getGitSummary(forRoot).catch(() => null),
      ]);
      if (mine !== token) return;
      summary = sum?.in_repo ? sum : null;
      const direct: Record<string, GitBadge> = {};
      const rolled: Record<string, GitBadge> = {};
      const rootKey = key(forRoot);
      for (const [path, badge] of Object.entries(status.files)) {
        const k = key(path);
        direct[k] = badge;
        // Every folder between the file and the project root learns about it.
        for (let dir = parentOf(k); dir.length >= rootKey.length; dir = parentOf(dir)) {
          rolled[dir] = strongerBadge(rolled[dir], badge)!;
        }
      }
      inRepo = status.in_repo;
      branch = status.branch;
      files = direct;
      folders = rolled;
    } catch {
      // No repository, or Corvus is not available: the tree simply shows no git marks.
      if (mine !== token) return;
      inRepo = false;
      summary = null;
      files = {};
      folders = {};
    }
  }

  return {
    get inRepo() { return inRepo; },
    get branch() { return branch; },
    /** Branch, ahead/behind and per-bucket counts; `null` outside a repository. */
    get summary() { return summary; },

    /**
     * The badge for a tree row, or `undefined` when it is clean.
     *
     * A file is looked up directly, then through its ancestors — a file inside an untracked
     * folder has no entry of its own, and is untracked all the same. A folder gets the strongest
     * state beneath it.
     */
    badgeFor(path: string, isDir: boolean): GitBadge | undefined {
      if (!inRepo) return undefined;
      const k = key(path);
      if (isDir) return files[k] ?? folders[k];
      const own = files[k];
      if (own) return own;
      for (let dir = parentOf(k); dir.length > 0; dir = parentOf(dir)) {
        if (files[dir] === 'untracked') return 'untracked';
      }
      return undefined;
    },

    /** Ask for a fresh answer for the project at `projectRoot`, after a short quiet period. */
    refresh(projectRoot: string | null | undefined) {
      if (timer) clearTimeout(timer);
      if (!projectRoot) {
        token++;
        root = '';
        inRepo = false;
        summary = null;
        files = {};
        folders = {};
        return;
      }
      root = projectRoot;
      timer = setTimeout(() => void load(projectRoot), REFRESH_DEBOUNCE_MS);
    },

    get root() { return root; },
  };
}

export const bennuGitStatusStore = createGitStatusStore();
