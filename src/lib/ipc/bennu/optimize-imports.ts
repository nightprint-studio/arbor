/**
 * Optimize imports over a whole folder — the project, a module, or any directory in the tree.
 *
 * A plan is built first and written second, the way structural replace does it: the preview reads
 * the disk and says what each file would become, the apply re-reads every file and refuses by name
 * any that changed in between. The per-file rule is the one Ctrl+Alt+O applies to the open buffer.
 *
 * Same conventions as the rest of `ipc/bennu`: fields under `{ args: … }`.
 */

import { bennu } from '../rpc';
import type { SsrApplied } from './ssr';

/** One file the optimization would change. */
export interface PlannedImportFile {
  file: string;
  /** Path relative to the project root, forward slashes. */
  rel: string;
  /** Import lines that go. Zero means the file is only being reordered. */
  removed: number;
  /** What the file was when the plan was built. Handed back to {@link applyOptimizeImports}. */
  digest: number;
  after: string;
}

export interface ImportsPlan {
  files: PlannedImportFile[];
  /** Java files looked at. */
  scanned: number;
  /** Files that would change but have unsaved edits open, so were left out. */
  skipped: string[];
}

/**
 * What optimizing every Java file under `scope` would do. Nothing is written.
 * `skip` lists the files with unsaved edits in the editor.
 *
 * Wire: `bennu_optimize_imports_preview`.
 */
export function previewOptimizeImports(root: string, scope: string, skip: string[]): Promise<ImportsPlan> {
  return bennu('bennu_optimize_imports_preview', { args: { root, scope, skip } });
}

/** Write a plan. Wire: `bennu_optimize_imports_apply`. */
export function applyOptimizeImports(
  root: string,
  files: { file: string; digest: number; after: string }[],
): Promise<SsrApplied> {
  return bennu('bennu_optimize_imports_apply', { args: { root, files } });
}
