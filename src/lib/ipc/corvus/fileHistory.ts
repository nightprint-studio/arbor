/**
 * Git history for a file named by absolute path — the tab-less door into Corvus, like
 * `blameByPath.ts` and for the same reason: Bennu has a file, not a Corvus `tab_id`.
 */
import { corvus, ensureBackend } from '../rpc';

/** A commit that touched the file. `date` is `YYYY-MM-DD` (UTC). */
export interface FileHistoryEntry {
  commit: string;
  short: string;
  author: string;
  date: string;
  summary: string;
}

/** A commit that touched a line range, with the diff of just those lines. */
export interface LineHistoryEntry extends FileHistoryEntry {
  patch: string;
}

/** Commits that touched `file`, newest first. */
export async function getFileHistoryByPath(file: string, limit?: number): Promise<FileHistoryEntry[]> {
  await ensureBackend('corvus');
  return corvus<FileHistoryEntry[]>('file_history_by_path', { file, limit });
}

/** Commits that touched lines `startLine..endLine` (1-based) of `file`, each with its diff. */
export async function getLineHistoryByPath(
  file: string,
  startLine: number,
  endLine: number,
  limit?: number,
): Promise<LineHistoryEntry[]> {
  await ensureBackend('corvus');
  return corvus<LineHistoryEntry[]>('line_history_by_path', {
    file, start_line: startLine, end_line: endLine, limit,
  });
}

/** What `commit` changed in `file`, as unified-diff text. */
export async function getFileCommitPatchByPath(file: string, commit: string): Promise<string> {
  await ensureBackend('corvus');
  return corvus<string>('file_commit_patch_by_path', { file, commit });
}
