/**
 * Which files are open in which of the editor's (at most two) panes — a pure model.
 *
 * The editor can be split into two panes side by side, each with its own tab strip and its own
 * active tab. This is the whole of that state, as immutable values and functions over them: the
 * project store holds one {@link Panes} and replaces it, and everything that used to be "the open
 * tabs" and "the active tab" is derived from it (`allTabs`, `activeOf`), so the rest of the app keeps
 * asking the questions it always asked and gets the focused pane's answer.
 *
 * ## The invariants, kept in one place
 *
 * * A file is open in **at most one pane**. Two editors on one buffer would be two CodeMirror states
 *   for one text, each with its own undo history, and an edit in one is a foreign change to the other.
 *   Opening a file that is already open in the other pane focuses it there instead.
 * * The second pane exists only while it has tabs. Emptying it (closing its last tab, moving it away)
 *   collapses the split; emptying the first pane promotes the second into its place — so pane 0 is
 *   only ever empty when nothing is open at all.
 * * A pane's active tab is one of its own tabs (or `null` when it has none), and `focused` always names
 *   a pane that exists.
 *
 * Every operation ends in {@link normalize}, which is what makes those hold however the operation got
 * there — rather than each one remembering to.
 */

export type PaneIndex = 0 | 1;

export interface Panes {
  /** The open tabs of each pane, in strip order. */
  readonly tabs: readonly [readonly string[], readonly string[]];
  readonly active: readonly [string | null, string | null];
  /** The pane that has the focus — whose active tab is "the active file". */
  readonly focused: PaneIndex;
}

export const emptyPanes: Panes = { tabs: [[], []], active: [null, null], focused: 0 };

/** Every open file, pane 0's tabs first. */
export function allTabs(p: Panes): string[] {
  return [...p.tabs[0], ...p.tabs[1]];
}

/** The focused pane's active file — what the rest of the app calls "the active file". */
export function activeOf(p: Panes): string | null {
  return p.active[p.focused];
}

export function isSplit(p: Panes): boolean {
  return p.tabs[1].length > 0;
}

/** The pane `path` is open in, or `null` when it is not open. */
export function paneOf(p: Panes, path: string): PaneIndex | null {
  if (p.tabs[0].includes(path)) return 0;
  if (p.tabs[1].includes(path)) return 1;
  return null;
}

/** The invariants (see the module notes), applied to whatever an operation produced. */
export function normalize(p: Panes): Panes {
  let tabs: [string[], string[]] = [[...new Set(p.tabs[0])], [...new Set(p.tabs[1])]];
  // A file in both panes stays in the first.
  tabs = [tabs[0], tabs[1].filter((t) => !tabs[0].includes(t))];
  let active: [string | null, string | null] = [p.active[0], p.active[1]];
  let focused = p.focused;

  // The first pane is empty only when everything is.
  if (tabs[0].length === 0 && tabs[1].length > 0) {
    tabs = [tabs[1], []];
    active = [active[1], null];
    focused = 0;
  }
  if (tabs[1].length === 0) {
    active[1] = null;
    focused = 0;
  }
  for (const pane of [0, 1] as const) {
    if (tabs[pane].length === 0) active[pane] = null;
    else if (active[pane] === null || !tabs[pane].includes(active[pane] as string)) active[pane] = tabs[pane][0];
  }
  return { tabs, active, focused };
}

/** Make `pane` the focused one. A pane that does not exist cannot be. */
export function focus(p: Panes, pane: PaneIndex): Panes {
  return normalize({ ...p, focused: pane });
}

/**
 * Open `path` and make it the active tab: where it already is if it is open, otherwise at the end of
 * the focused pane.
 */
export function open(p: Panes, path: string): Panes {
  const at = paneOf(p, path);
  if (at !== null) return activate(p, path);
  const pane = p.focused;
  const tabs: [string[], string[]] = [[...p.tabs[0]], [...p.tabs[1]]];
  tabs[pane].push(path);
  const active: [string | null, string | null] = [p.active[0], p.active[1]];
  active[pane] = path;
  return normalize({ tabs, active, focused: pane });
}

/** Make `path` the active tab of its pane and focus that pane. A path that is not open changes nothing. */
export function activate(p: Panes, path: string): Panes {
  const at = paneOf(p, path);
  if (at === null) return p;
  const active: [string | null, string | null] = [p.active[0], p.active[1]];
  active[at] = path;
  return normalize({ tabs: p.tabs, active, focused: at });
}

/**
 * Close `path`. When it was its pane's active tab, the neighbour at the same position takes over — the
 * one that slides into its place, or the one before it at the end — within the same pane.
 */
export function close(p: Panes, path: string): Panes {
  const at = paneOf(p, path);
  if (at === null) return p;
  const list = p.tabs[at];
  const index = list.indexOf(path);
  const rest = list.filter((t) => t !== path);
  const tabs: [string[], string[]] = [[...p.tabs[0]], [...p.tabs[1]]];
  tabs[at] = rest;
  const active: [string | null, string | null] = [p.active[0], p.active[1]];
  if (active[at] === path) active[at] = rest.length ? rest[Math.min(index, rest.length - 1)] : null;
  return normalize({ tabs, active, focused: p.focused });
}

/** Close everything but `path`, which becomes the only tab. */
export function closeOthers(p: Panes, path: string): Panes {
  if (paneOf(p, path) === null) return p;
  return normalize({ tabs: [[path], []], active: [path, null], focused: 0 });
}

export function closeAll(): Panes {
  return emptyPanes;
}

/** Close the tabs to the right of `path` in its own pane. The pane's active tab falls back to `path`
 *  when it was among those closed. */
export function closeToRight(p: Panes, path: string): Panes {
  const at = paneOf(p, path);
  if (at === null) return p;
  const list = p.tabs[at];
  const kept = list.slice(0, list.indexOf(path) + 1);
  if (kept.length === list.length) return p;
  const tabs: [string[], string[]] = [[...p.tabs[0]], [...p.tabs[1]]];
  tabs[at] = kept;
  const active: [string | null, string | null] = [p.active[0], p.active[1]];
  if (active[at] !== null && !kept.includes(active[at] as string)) active[at] = path;
  return normalize({ tabs, active, focused: p.focused });
}

/**
 * Move `path` to `pane`, at `index` (the end by default), and make it the active tab there with the
 * focus. Moving to the second pane when there is none creates the split. Moving the only tab of a pane
 * away collapses the split ({@link normalize}). Moving within a pane is a reorder.
 */
export function moveTo(p: Panes, path: string, pane: PaneIndex, index?: number): Panes {
  const from = paneOf(p, path);
  if (from === null) return p;
  const tabs: [string[], string[]] = [[...p.tabs[0]], [...p.tabs[1]]];
  tabs[from] = tabs[from].filter((t) => t !== path);
  const target = tabs[pane];
  const at = index === undefined ? target.length : Math.max(0, Math.min(index, target.length));
  target.splice(at, 0, path);
  const active: [string | null, string | null] = [p.active[0], p.active[1]];
  if (active[from] === path) active[from] = tabs[from][0] ?? null;
  active[pane] = path;
  return normalize({ tabs, active, focused: pane });
}

/** Reorder within one pane: the tab at `from` ends up at `to`. */
export function reorder(p: Panes, pane: PaneIndex, from: number, to: number): Panes {
  const list = [...p.tabs[pane]];
  if (from < 0 || from >= list.length || from === to) return p;
  const [moved] = list.splice(from, 1);
  list.splice(Math.max(0, Math.min(to, list.length)), 0, moved);
  const tabs: [string[], string[]] = [[...p.tabs[0]], [...p.tabs[1]]];
  tabs[pane] = list;
  return normalize({ tabs, active: p.active, focused: p.focused });
}

/** Merge the second pane back into the first — its tabs after the first's — keeping the file that was
 *  being edited active. */
export function merge(p: Panes): Panes {
  if (!isSplit(p)) return p;
  return normalize({ tabs: [[...p.tabs[0], ...p.tabs[1]], []], active: [activeOf(p), null], focused: 0 });
}

/** `from` renamed to `to` everywhere it appears (a file rename or move), keeping its pane and place. */
export function replacePath(p: Panes, from: string, to: string): Panes {
  const swap = (list: readonly string[]) => list.map((t) => (t === from ? to : t));
  return normalize({
    tabs: [swap(p.tabs[0]), swap(p.tabs[1])],
    active: [p.active[0] === from ? to : p.active[0], p.active[1] === from ? to : p.active[1]],
    focused: p.focused,
  });
}

/** The split as a stored session keeps it: the whole tab list stays whole (so a build that knows no
 *  split still opens every tab), and the second pane is named by what it holds. */
export interface StoredPanes {
  open_files: string[];
  /** The focused pane's active tab. */
  active_file: string;
  split_files: string[];
  first_active: string;
  split_active: string;
}

/** The panes as the fields a stored session carries. A session with no split writes the extra
 *  fields empty, which the stored form leaves out. */
export function toStored(p: Panes): StoredPanes {
  const split = isSplit(p);
  return {
    open_files: allTabs(p),
    active_file: activeOf(p) ?? '',
    split_files: [...p.tabs[1]],
    first_active: split ? (p.active[0] ?? '') : '',
    split_active: split ? (p.active[1] ?? '') : '',
  };
}

/**
 * The panes a stored session describes. Tolerant of every shape a file can have: no split fields (a
 * session from before the split existed) is one pane; a split tab that is not in the list, or a pane
 * left with nothing, is dropped by {@link normalize}; and the focus is wherever the stored active
 * file is.
 */
export function fromStored(s: {
  open_files: readonly string[];
  active_file?: string;
  split_files?: readonly string[];
  first_active?: string;
  split_active?: string;
}): Panes {
  const all = [...new Set(s.open_files)];
  const split = (s.split_files ?? []).filter((t) => all.includes(t));
  const first = all.filter((t) => !split.includes(t));
  const active = s.active_file || null;
  const focused: PaneIndex = active !== null && split.includes(active) ? 1 : 0;
  return normalize({
    tabs: [first, split],
    active: [s.first_active || (focused === 0 ? active : null), s.split_active || (focused === 1 ? active : null)],
    focused,
  });
}

/** A single pane from a plain list of tabs — what a stored session, which knows no split, restores to. */
export function fromTabs(tabs: readonly string[], active: string | null): Panes {
  return normalize({ tabs: [[...tabs], []], active: [active, null], focused: 0 });
}
