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

/** Payload of `arbor://bennu/tree-changed`. */
export interface TreeChanged {
  /** The project root the changes are under. */
  root: string;
  /** Root-relative paths, forward slashes. Capped — see `truncated`. */
  paths: string[];
  /** More changed than are listed. The tree is reloaded wholesale either way; this is for a
   *  caller that wants to say so rather than for one that acts on it. */
  truncated: boolean;
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
