/**
 * The two verbs Bennu's Changes panel needs on top of Corvus's tab-keyed staging calls (see
 * `stage.ts`): turn a project path into a headless-repo tab id, and read one file's working-tree
 * change as a patch. Same tab-less door as `treeStatus.ts`.
 */
import { corvus, ensureBackend } from '../rpc';

/** The `tab_id` that names the repository containing `path` — pass it to `stage.ts`'s calls. */
export async function repoTabForPath(path: string): Promise<string> {
  await ensureBackend('corvus');
  return corvus<string>('repo_tab_for_path', { path });
}

/** Unified patch of `file`'s change: index vs HEAD when `staged`, working tree vs index otherwise.
 *  An untracked file comes back as entirely added. */
export async function getWorkdirFilePatch(file: string, staged: boolean): Promise<string> {
  await ensureBackend('corvus');
  return corvus<string>('workdir_file_patch_by_path', { file, staged });
}
