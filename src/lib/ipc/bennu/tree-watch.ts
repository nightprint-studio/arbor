/**
 * The Project tree's filesystem watcher.
 *
 * One call and one event. The call says which roots to watch; the event says a burst of changes
 * has settled and names the paths — so a `git checkout` touching four hundred files is one reload
 * rather than four hundred.
 *
 * What is deliberately **not** here: any notion of "reload the tree". The store decides that, from
 * the root the event names — a change can land in a workspace member that is not on screen, and
 * reloading the active tree for it would be a reload that fixes nothing.
 */

import { bennu } from '../rpc';

/**
 * One change to what the tree lists, net of everything the burst did to the same path: an editor's
 * write-to-temp-then-rename save nets out to nothing, a rename's two halves arrive as one entry,
 * and what happened *inside* an added, removed or renamed directory is folded into it.
 * Paths are root-relative, forward slashes. `module` — the directory holds a `pom.xml`.
 */
export type TreeChange =
  | { kind: 'added'; path: string; is_dir: boolean; module: boolean }
  | { kind: 'removed'; path: string; is_dir: boolean }
  | { kind: 'renamed'; from: string; path: string; is_dir: boolean; module: boolean };

/** Payload of `arbor://bennu/tree-changed`. */
export interface TreeChanged {
  /** The project root the changes are under. */
  root: string;
  /** Root-relative paths, forward slashes. Capped — see `truncated`. Every structural change, plus
   *  content changes to the few files whose content matters here (`pom.xml`, `Cargo.toml`,
   *  `.gitignore`); an ordinary file edit is not reported at all. */
  paths: string[];
  /** More changed than are listed. The tree is reloaded wholesale either way; this is for a
   *  caller that wants to say so rather than for one that acts on it. */
  truncated: boolean;
  /** Something was created, removed or renamed — the listing changed, not just a file's bytes. */
  structural: boolean;
  /** Events may have been lost (a watcher error, a bulk burst, a watched directory that moved):
   *  trust nothing but a fresh read. */
  rescan: boolean;
  /** The net structural changes. Capped like `paths`. */
  changes: TreeChange[];
}

/** The topic. Exported so the subscriber and the backend cannot drift apart silently. */
export const TREE_CHANGED = 'arbor://bennu/tree-changed';

/**
 * Watch these roots, replacing whatever was watched.
 *
 * Resolves `false` when no watcher could be started — an unreadable root, a platform limit. The
 * tree then has to be refreshed by hand, which is why this is worth knowing and not worth
 * throwing: a project that opened fine should not report an error for something nobody asked for.
 *
 * Wire: `bennu_watch_roots`.
 */
export function watchRoots(roots: string[]): Promise<boolean> {
  return bennu('bennu_watch_roots', { args: { roots } });
}
