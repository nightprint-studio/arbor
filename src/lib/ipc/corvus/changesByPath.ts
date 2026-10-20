/**
 * What Bennu's Changes panel needs on top of Corvus's tab-keyed staging and diff calls (see
 * `stage.ts`, `diff.ts`): turn a project path into a headless-repo tab id. Same tab-less door
 * as `treeStatus.ts`.
 */
import { corvus, ensureBackend } from '../rpc';

/** The `tab_id` that names the repository containing `path` — pass it to `stage.ts`'s calls. */
export async function repoTabForPath(path: string): Promise<string> {
  await ensureBackend('corvus');
  return corvus<string>('repo_tab_for_path', { path });
}

