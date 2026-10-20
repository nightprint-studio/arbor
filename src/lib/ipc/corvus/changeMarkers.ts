/**
 * Which lines of an editor buffer differ from HEAD — the data behind Bennu's change gutter.
 * Tab-less like `treeStatus.ts`: the repository is discovered from the file path.
 */
import { corvus, ensureBackend } from '../rpc';
import type { ChangeHunk } from '$lib/components/shared/ui/code-editor/change-gutter';

export interface ChangeMarkers {
  /** The file exists in HEAD. False → nothing to mark (untracked, new repo, no repository). */
  tracked: boolean;
  /** The diff was not run: the file is over the size limit. */
  skipped: boolean;
  hunks: ChangeHunk[];
}

export async function getChangeMarkers(file: string, text: string): Promise<ChangeMarkers> {
  await ensureBackend('corvus');
  return corvus<ChangeMarkers>('change_markers_by_path', { file, text });
}
