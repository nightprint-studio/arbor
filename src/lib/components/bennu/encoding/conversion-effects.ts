/**
 * What the rest of the window has to do after a conversion (or its undo) changed files on disk:
 * re-read the open ones, re-read the config the backend moved the encoding rule in, and refresh the
 * project's declared encoding. Shared by the dialog and the palette's "Undo last conversion".
 */
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { encodingConvertUndo, type UndoResult } from '$lib/ipc/bennu/encoding-convert';

/** Re-sync the window with `files` having been rewritten behind its back. */
export async function afterConversionChange(files: string[], root: string): Promise<void> {
  for (const f of files) void projectStore.reload(f);
  await bennuConfigStore.reload();
  if (root) void projectStore.refreshProjectInfo(root);
}

/** Say what an undo did — one toast, warning when some files had been edited since and were left alone. */
export function reportUndo(res: UndoResult): void {
  const n = res.restored.length;
  const left = res.skipped.length;
  toastStore.show(
    `Restored ${n} file${n === 1 ? '' : 's'}${left ? `; ${left} changed since and ${left === 1 ? 'was' : 'were'} left alone` : ''}`,
    left ? 'warning' : 'success',
  );
}

/** Undo the latest encoding conversion of the open project (the palette command). */
export async function undoLastEncodingConversion(): Promise<void> {
  const root = projectStore.project?.root;
  if (!root) return;
  try {
    const res = await encodingConvertUndo(root);
    await afterConversionChange(res.restored, root);
    reportUndo(res);
  } catch (e) {
    toastStore.show(String(e).replace(/^Error:\s*/, ''), 'warning');
  }
}
