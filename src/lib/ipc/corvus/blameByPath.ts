/**
 * Git blame for a file named by absolute path — the tab-less door into Corvus.
 *
 * Kept apart from `diff.ts` on purpose: that module reads Corvus's tab and diff stores, and a
 * window that only wants a blame (Bennu's gutter) should not pull a product's stores in to get
 * one. The backend discovers the repository from the path and keeps it registered for an idle
 * TTL, so repeated calls on the same project are cheap.
 */
import { corvus, ensureBackend } from '../rpc';
import type { BlameLine } from '../../types/corvus/git';

/**
 * Blame `file` as of HEAD. Rejects with the backend's sentence when the file is not in a git
 * repository, has no history yet, or is untracked — callers show that as "no blame", not as
 * an error.
 */
export async function getFileBlameByPath(file: string): Promise<BlameLine[]> {
  await ensureBackend('corvus');
  return corvus<BlameLine[]>('get_file_blame_by_path', { file });
}
