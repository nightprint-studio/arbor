/**
 * The rules that keep a Bennu session from being overwritten by something that is not one.
 *
 * Kept out of the rune stores (which cannot run under the Node test runner) because every one of
 * them is a decision about ORDER and STATE — when may a snapshot be written, which writes may race,
 * which caret reports are the reader's — and those are exactly what is impossible to see in the UI
 * and cannot be reproduced by hand.
 */

import type { ProjectSession } from '$lib/ipc/bennu/config';

export interface SessionSnapshot { active_project: string; projects: ProjectSession[] }

export interface SnapshotInput {
  /** A restore is running: the flat state is half built. */
  restoring: boolean;
  isDemo: boolean;
  /** The root on screen, or undefined when nothing is. */
  activeRoot: string | undefined;
  /** Sessions of the members that are open, the active one included. */
  live: ProjectSession[];
  /** Stored members that did not open — kept as they were. */
  unopened: ProjectSession[];
  /** The project the stored workspace wanted on screen (canonical). */
  wantedRoot: string;
  canon: (path: string) => string;
}

/**
 * The session to store, or `null` when the live state is not one right now.
 *
 * - While restoring: `null`. A snapshot of a half-built workspace is the one that used to replace a real
 *   session when the window closed, or the user switched away, during boot.
 * - Demo project: nothing to remember.
 * - Nothing on screen: the stored members that never opened, still — an empty list only when the user
 *   really closed everything.
 * - Otherwise the open members, then the ones that did not open (dropped silently, they cost a project its tabs).
 */
export function composeSnapshot(i: SnapshotInput): SessionSnapshot | null {
  if (i.restoring) return null;
  if (i.isDemo) return { active_project: '', projects: [] };
  if (!i.activeRoot) {
    return {
      active_project: i.unopened.some((u) => i.canon(u.root) === i.wantedRoot) ? i.wantedRoot : '',
      projects: [...i.unopened],
    };
  }
  return { active_project: i.activeRoot, projects: [...i.live, ...i.unopened] };
}

/**
 * Serialise async writes: each one starts after the previous settled, and takes its snapshot when it
 * STARTS. Two writes in flight can be served out of order by a backend that handles every request on
 * its own thread, and then the older one is what remains on disk.
 */
export function createWriteQueue(send: () => Promise<void>): () => Promise<void> {
  let chain: Promise<void> = Promise.resolve();
  return () => {
    chain = chain.then(send, send);
    return chain;
  };
}

/**
 * Whether a caret report from the editor is worth remembering across restarts.
 *
 * A restored tab mounts before its text arrives and loading it reports 1:1; remembered, that replaces
 * the position the restore is about to read. So: a user's own move, or a tab whose restore has
 * already run, only.
 */
export function shouldRememberCaret(user: boolean | undefined, restoredFor: string | null, path: string): boolean {
  return user === true || restoredFor === path;
}

/** Stored members in the order the workspace was saved in; unknown roots go last, order kept. */
export function inStoredOrder(roots: string[], stored: string[]): string[] {
  const rank = (r: string) => { const i = stored.indexOf(r); return i < 0 ? Number.MAX_SAFE_INTEGER : i; };
  return roots.map((r, i) => ({ r, i })).sort((a, b) => rank(a.r) - rank(b.r) || a.i - b.i).map((x) => x.r);
}
