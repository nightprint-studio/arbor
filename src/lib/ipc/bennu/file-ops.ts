/**
 * Bennu file-operations IPC — deleting project files, and taking it back.
 *
 * Kept apart from `history.ts` because these two are actions on the project, not
 * questions about it — but they only exist because the history does: the undo restores
 * from that store rather than from the operating system's trash, which on macOS has no
 * API to put a file back where it came from.
 */

import { bennu } from '../rpc';

/** One path a delete could not remove, and why. */
export interface DeleteFailure {
  path: string;
  error: string;
}

/** What a delete did. */
export interface DeleteResult {
  /** The files that are gone, absolute with forward slashes. A deleted directory is
   *  reported through the files that were inside it — tabs and tree rows are keyed by
   *  file, and a directory name closes nothing. */
  deleted: string[];
  /** How many of them the history kept — how many the undo can bring back. */
  recorded: number;
  /** The change set the whole delete shares. Hand it back to {@link undelete}. */
  change: string;
  failed: DeleteFailure[];
}

/** What an undo did. */
export interface UndeleteResult {
  restored: string[];
  /** Left alone because something is there now, or because nothing was kept for it. */
  skipped: string[];
}

/** Delete files and directories, keeping what they held so it can be undone. The whole
 *  call shares one change set. Wire: `bennu_delete_paths`. */
export function deletePaths(root: string, paths: string[]): Promise<DeleteResult> {
  return bennu('bennu_delete_paths', { args: { root, paths } });
}

/** Put back everything a delete removed. Wire: `bennu_undelete`. */
export function undelete(root: string, change: string): Promise<UndeleteResult> {
  return bennu('bennu_undelete', { args: { root, change } });
}
