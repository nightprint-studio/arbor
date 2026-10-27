/**
 * Bennu project switcher — the Alt+Tab of the open projects, across workspaces.
 *
 * Holds the most-recently-used order of projects and the state of the switcher overlay. The overlay
 * is driven the way an OS task switcher is: Ctrl+Tab opens it on the previous project, further Tab
 * presses walk the list, releasing Ctrl commits. That makes "jump back and forth between two
 * projects" a single chord, which is the case this exists for.
 *
 * A project is identified by **workspace and root**, not root alone: the same folder may belong to
 * two workspaces, each with its own tabs, and "the previous project" is a place you were in, not a
 * folder. Picking one in another workspace switches workspace first (the caller does that).
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import type { WorkspaceProject } from './workspaces.svelte';

export type SwitcherEntry = WorkspaceProject;

/** The identity of an entry: `<workspaceId>|<root>`. */
export function entryKey(e: { workspaceId: string; root: string }): string {
  return `${e.workspaceId}|${e.root}`;
}

/** Roots compare the same however they were written: slashes, trailing slash, case. */
function normRoot(p: string): string {
  return p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

function createProjectSwitcherStore() {
  let loaded = new Set<string>();
  /** Keys ({@link entryKey}), most recently active first.
   *
   *  Deliberately NOT `$state`: nothing renders from it — it is only read when the overlay opens —
   *  and `touch` is called from an `$effect` that tracks the active project. A reactive list would
   *  be read (to filter it) and written by that same effect, which re-runs it forever and freezes
   *  the window the moment a project opens. */
  let mru: string[] = [];
  let open = $state(false);
  let index = $state(0);
  /** The list as it was when the overlay opened, so it does not reshuffle under the cursor. */
  let entries = $state<SwitcherEntry[]>([]);

  return {
    get open() { return open; },
    get index() { return index; },
    get entries() { return entries; },

    /** Record `workspaceId`'s `root` as the active project. */
    touch(workspaceId: string, root: string) {
      const key = entryKey({ workspaceId, root });
      mru = [key, ...mru.filter((k) => k !== key)];
    },

    /** The backend's answer to "which roots are in memory" (see `loadedProjects`). Plain data, for
     *  the same reason as `mru`: read at the moment the overlay opens, never rendered. */
    setLoaded(roots: string[]) {
      loaded = new Set(roots.map(normRoot));
    },

    /** Whether `entry` is open and ready — the project on screen, one you have been in this session,
     *  or one the backend holds an index / language server for — as opposed to a workspace member
     *  that was only listed (or restored in the background) and would have to start up first. */
    isOpen(entry: { workspaceId: string; root: string }): boolean {
      return mru.includes(entryKey(entry)) || loaded.has(normRoot(entry.root));
    },

    /** 0 for the most recently active, growing; `Infinity` when never active this session. For
     *  callers that order their own lists by the same recency as the overlay. */
    recencyRank(entry: { workspaceId: string; root: string }): number {
      const i = mru.indexOf(entryKey(entry));
      return i < 0 ? Number.POSITIVE_INFINITY : i;
    },

    /** Open the overlay over `projects` (dir: +1 forward, -1 backward). False when there is nothing
     *  to switch to. */
    begin(projects: SwitcherEntry[], dir: 1 | -1): boolean {
      if (projects.length < 2) return false;
      const rank = (e: SwitcherEntry) => {
        const i = mru.indexOf(entryKey(e));
        return i < 0 ? Number.MAX_SAFE_INTEGER : i;
      };
      entries = [...projects].sort((a, b) => rank(a) - rank(b));
      index = dir > 0 ? 1 : entries.length - 1;
      open = true;
      return true;
    },

    step(dir: 1 | -1) {
      if (!open || entries.length === 0) return;
      index = (index + dir + entries.length) % entries.length;
    },

    select(i: number) {
      if (open && i >= 0 && i < entries.length) index = i;
    },

    /** Close the overlay and return the chosen entry. */
    commit(): SwitcherEntry | null {
      if (!open) return null;
      const entry = entries[index] ?? null;
      open = false;
      return entry;
    },

    cancel() {
      open = false;
    },
  };
}

export const projectSwitcherStore = createProjectSwitcherStore();
