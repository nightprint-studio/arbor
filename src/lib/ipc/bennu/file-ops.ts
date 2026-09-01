/**
 * Bennu file-operations IPC — creating directories, deleting files, and taking a delete back.
 *
 * Kept apart from `history.ts` because these are actions on the project, not questions about
 * it, and apart from `scaffold.ts`, which only *resolves* what a new file would be — nothing
 * in there touches the disk. Delete and undo only exist because the history does: the undo
 * restores from that store rather than from the operating system's trash, which on macOS has
 * no API to put a file back where it came from.
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

/** What creating a folder did. */
export interface NewFolderResult {
  /** Absolute path (forward slashes) of the **deepest** directory — the one to reveal. */
  path: string;
  /** The directories actually created, outermost first. Empty when it was all already there. */
  created: string[];
  /** True when nothing was created because the whole path existed. */
  existed: boolean;
}

/**
 * Create a directory under `dir` — or a chain of them, since `name` is a path:
 * `assets/icons` makes two, and in package territory (`asPackage`) `it.acme.web` makes three.
 *
 * Levels that already exist are stepped through, not objected to: `src/main/resources` where
 * `src/main` is there creates `resources` alone, and `created` says exactly that.
 * Wire: `bennu_new_folder`.
 */
export function newFolder(
  root: string,
  dir: string,
  name: string,
  asPackage: boolean,
): Promise<NewFolderResult> {
  return bennu('bennu_new_folder', { args: { root, dir, name, as_package: asPackage } });
}
