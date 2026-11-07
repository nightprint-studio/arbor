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
    // Nothing on screen, but members may still be attached in the background (a project moved into an
    // empty workspace): they belong to the session as much as the ones that did not open.
    const wanted = [...i.live, ...i.unopened].some((u) => i.canon(u.root) === i.wantedRoot);
    return {
      active_project: wanted ? i.wantedRoot : '',
      projects: [...i.live, ...i.unopened],
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

/**
 * Run async operations strictly one after the other, each starting only once the previous settled
 * (whether it resolved or threw). A failure does not poison the chain, and is still seen by the
 * caller of that operation.
 *
 * What it is for: every workspace mutation reads the whole list, edits it and writes it back, with
 * an `await` in the middle. Two of them interleaved — three quick drags in the manager — each edit a
 * copy that the other has already changed.
 */
export function createSerial(): <T>(op: () => Promise<T>) => Promise<T> {
  let chain: Promise<unknown> = Promise.resolve();
  return <T>(op: () => Promise<T>) => {
    const run = chain.then(op, op);
    chain = run.catch(() => {});
    return run;
  };
}

/** The part of a workspace a move touches. */
export interface MovableWorkspace {
  id: string;
  active_project: string;
  projects: ProjectSession[];
}

/**
 * Move project `root` from workspace `fromId` into `toId` at `toIndex` (a gap in the destination as it
 * is now), as ONE step on `list`: the result has the project in the target or, when anything is
 * missing, `list` unchanged — never in neither. Pure; returns new workspaces and leaves `list` alone.
 *
 * - same workspace: a reorder;
 * - the target already holds the root: the source simply lets it go (no duplicate);
 * - the source's `active_project` falls back to its first remaining project.
 *
 * `session` overrides the stored session being carried (the live one, fresher than what is stored).
 */
export function planMove<W extends MovableWorkspace>(
  list: W[], fromId: string, root: string, toId: string, toIndex: number, session?: ProjectSession,
): W[] {
  const source = list.find((w) => w.id === fromId);
  const target = list.find((w) => w.id === toId);
  const carried = source?.projects.find((p) => p.root === root);
  if (!source || !target || !carried) return list;
  const moving = session ?? carried;
  const clamp = (n: number, len: number) => Math.max(0, Math.min(len, n));

  if (fromId === toId) {
    const projects = [...source.projects];
    const from = projects.findIndex((p) => p.root === root);
    projects.splice(from, 1);
    projects.splice(clamp(toIndex > from ? toIndex - 1 : toIndex, projects.length), 0, moving);
    return list.map((w) => (w.id === fromId ? { ...w, projects } : w));
  }

  return list.map((w) => {
    if (w.id === fromId) {
      const projects = w.projects.filter((p) => p.root !== root);
      return { ...w, projects, active_project: w.active_project === root ? (projects[0]?.root ?? '') : w.active_project };
    }
    if (w.id === toId && !w.projects.some((p) => p.root === root)) {
      const projects = [...w.projects];
      projects.splice(clamp(toIndex, projects.length), 0, moving);
      return { ...w, projects };
    }
    return w;
  });
}
