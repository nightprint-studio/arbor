/**
 * Git status of a whole project, by path — what Bennu's file tree colours its rows from.
 *
 * The tab-less door into Corvus again (see `blameByPath.ts`): Bennu has a project folder, not a
 * Corvus tab. One call returns every changed file of the repository containing `root`.
 */
import { corvus, ensureBackend } from '../rpc';
import type { GitBadge } from '../fs';

export interface TreeGitStatus {
  /** `root` is inside a git repository. */
  in_repo: boolean;
  branch: string | null;
  /** Absolute path (forward slashes) → its badge; clean paths are absent. An untracked folder
   *  appears once, as the folder — its files have no entry of their own. */
  files: Record<string, GitBadge>;
}

export async function getGitTreeStatus(root: string): Promise<TreeGitStatus> {
  await ensureBackend('corvus');
  return corvus<TreeGitStatus>('git_tree_status_by_path', { root });
}
