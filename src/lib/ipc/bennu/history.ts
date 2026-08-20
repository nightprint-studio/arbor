/**
 * Bennu local-history IPC — what every project file used to be.
 *
 * Routes through the generic `bennu(...)` bridge, wrapping each call's fields under
 * `{ args: … }` like every other bennu handler. Types only + wrappers; the store owns
 * the state and the modal owns the UI.
 *
 * The store behind this is `arbor-history`, a foundation crate that speaks paths and
 * bytes — so the shapes here carry no editor concepts either. What Bennu adds on top is
 * the two things a content store deliberately refuses to know: which files are worth
 * recording (not the gitignored ones), and what encoding their bytes are in.
 */

import { bennu } from '../rpc';

/** Why a revision was recorded. What it says changes what you do about it: `saved` is you,
 *  `refactored` is a tool that may have touched a dozen files, `external` is something that
 *  happened behind the editor's back. */
export type RevisionKind =
  | 'created' | 'saved' | 'external' | 'refactored' | 'renamed' | 'deleted';

/** One recorded state of one file. */
export interface Revision {
  id: string;
  /** Unix milliseconds. */
  at: number;
  kind: RevisionKind;
  /** Content hash. Absent on a deletion, which stores no bytes. */
  blob?: string;
  size: number;
  /** A name the user pinned here. A labelled revision never expires. */
  label?: string;
  /** What the operation was, when a tool did it. */
  title?: string;
  /** The change set this belongs to — shared by every file one operation touched. */
  change?: string;
  /** For a rename, where the file came from (project-relative). */
  from?: string;
}

/** One file's whole history, newest first. */
export interface FileHistory {
  /** Project-relative, forward slashes. */
  path: string;
  /** The file is currently gone. */
  deleted: boolean;
  revisions: Revision[];
}

/** A file the history knows and the project no longer has. */
export interface DeletedEntry {
  path: string;
  name: string;
  at: number;
  kind: RevisionKind;
  title?: string;
  blob?: string;
  size: number;
  revisions: number;
}

/** One entry of a directory, as the history knows it. Merged by the UI with the live
 *  tree, which is the only side that knows about files nobody ever edited. */
export interface FolderEntry {
  path: string;
  name: string;
  is_dir: boolean;
  deleted: boolean;
  at: number;
  revisions: number;
}

/** One file inside a change set. */
export interface ChangeFile {
  path: string;
  revision: string;
  kind: RevisionKind;
}

/** One operation: a save is a change set of one file, a refactor is one of six. */
export interface ChangeGroup {
  id: string;
  at: number;
  kind: RevisionKind;
  title?: string;
  label?: string;
  files: ChangeFile[];
}

/** A directory's history: what it held, and what has happened in it. */
export interface FolderHistory {
  entries: FolderEntry[];
  timeline: ChangeGroup[];
}

export type DiffLineKind = 'context' | 'add' | 'del';

export interface DiffLine {
  kind: DiffLineKind;
  /** 1-based line number on the old side; absent for an added line. */
  old?: number;
  /** 1-based line number on the new side; absent for a removed line. */
  new?: number;
  text: string;
}

export interface DiffHunk {
  old_start: number;
  new_start: number;
  lines: DiffLine[];
}

/** The comparison of two revisions. `identical` is said out loud rather than left to an
 *  empty hunk list, which reads as a failure to load. */
export interface TextDelta {
  hunks: DiffHunk[];
  added: number;
  removed: number;
  identical: boolean;
}

/** A revision's content, decoded in the project's own encoding. */
export interface RevisionContent {
  text: string;
  encoding: string;
  /** `false` for bytes that are not text — an image has a history too, and the viewer
   *  must be told rather than shown mojibake. */
  is_text: boolean;
}

/** What the history is costing. */
export interface HistoryUsage {
  files: number;
  revisions: number;
  bytes: number;
}

/** One file's revisions, newest first. Wire: `bennu_history_file`. */
export function fileHistory(root: string, file: string): Promise<FileHistory> {
  return bennu('bennu_history_file', { args: { root, file } });
}

/** A directory's entries + timeline, as of `at` (unix ms) or now. Pass an empty `dir`
 *  for the whole project. Wire: `bennu_history_folder`. */
export function folderHistory(root: string, dir: string, at?: number): Promise<FolderHistory> {
  return bennu('bennu_history_folder', { args: { root, dir, at: at ?? null } });
}

/** Every file the history knows and the project no longer has. Wire: `bennu_history_deleted`. */
export function deletedFiles(root: string): Promise<DeletedEntry[]> {
  return bennu('bennu_history_deleted', { args: { root } });
}

/** One revision's content as text. Omit `revision` for the newest content a file ever
 *  had — what a deleted file is restored from. Wire: `bennu_history_content`. */
export function revisionContent(
  root: string, file: string, revision?: string,
): Promise<RevisionContent> {
  return bennu('bennu_history_content', { args: { root, file, revision: revision ?? null } });
}

/** What changed between `revision` and `against` — or between it and what is on disk now,
 *  which is the comparison that answers "what would restoring this change?".
 *  Wire: `bennu_history_diff`. */
export function revisionDiff(
  root: string, file: string, revision: string, against?: string,
): Promise<TextDelta> {
  return bennu('bennu_history_diff', { args: { root, file, revision, against: against ?? null } });
}

/** Put a revision back on disk. Omit `revision` to restore a deleted file's last content;
 *  pass `to` to put it somewhere else. Wire: `bennu_history_restore`. */
export function restoreRevision(
  root: string, file: string, revision?: string, to?: string,
): Promise<{ file: string }> {
  return bennu('bennu_history_restore', {
    args: { root, file, revision: revision ?? null, to: to ?? null },
  });
}

/** Pin a name on a revision (empty clears it). Wire: `bennu_history_label`. */
export function labelRevision(
  root: string, file: string, revision: string, label: string,
): Promise<boolean> {
  return bennu('bennu_history_label', { args: { root, file, revision, label } });
}

/** Record files that changed (or vanished) outside Bennu — the one trigger the editor
 *  cannot infer from its own actions. Wire: `bennu_history_external`. */
export function noteExternal(root: string, files: string[]): Promise<number> {
  return bennu('bennu_history_external', { args: { root, files } });
}

/** What the history is costing, for the settings page. Wire: `bennu_history_usage`. */
export function historyUsage(root: string): Promise<HistoryUsage> {
  return bennu('bennu_history_usage', { args: { root } });
}

/** Throw this project's whole history away. Wire: `bennu_history_clear`. */
export function clearHistory(root: string): Promise<boolean> {
  return bennu('bennu_history_clear', { args: { root } });
}
