/**
 * Deleting project files, and the stack that takes it back.
 *
 * ## Why the undo lives here and not in the editor
 *
 * There are two undo stacks in this window and they must stay two, because they answer
 * two questions: <kbd>⌘Z</kbd> in the buffer means "un-type that", <kbd>⌘Z</kbd> in the
 * project tree means "un-delete that". Merging them is how you end up undoing the wrong
 * thing — you press it expecting your last keystroke back and get a file instead, or the
 * reverse. So each surface owns its own, and which one answers is decided by where the
 * focus is.
 *
 * ## What makes the undo possible
 *
 * Not the operating system's trash. macOS has no API to put a file back where it came
 * from, so an undo built on it would have to find the file by name in `~/.Trash` and
 * hope — and two `mod.rs` from two folders, or a collision macOS renamed to `mod 2.rs`,
 * turn that hope into "restored the wrong file, silently". The backend writes every file
 * into the local history before unlinking it, and the undo reads it back from there.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { deletePaths as ipcDelete, undelete as ipcUndelete } from '$lib/ipc/bennu/file-ops';
import { projectStore } from './project.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

/** One delete, as the undo stack remembers it. */
interface UndoEntry {
  /** The project it happened in — an undo must not reach into another one. */
  root: string;
  /** The backend's change set: the one handle that puts all of it back at once. */
  change: string;
  /** How many files, for the label. */
  count: number;
  /** What was deleted, for the tabs that need closing again on a redo-less second undo. */
  label: string;
}

/** How many deletes stay undoable. Deep enough for a session's worth of mistakes, shallow
 *  enough that the stack never becomes a second, worse history browser — that is what the
 *  Local History dialog is, and it goes back a week. */
const DEPTH = 20;

function createFileOpsStore() {
  let stack = $state<UndoEntry[]>([]);
  let busy = $state(false);

  /** The label a confirmation and a toast both use, so the two never disagree about what
   *  is being deleted. */
  function describe(paths: string[]): string {
    if (paths.length === 1) return paths[0].split(/[\\/]/).pop() ?? paths[0];
    return `${paths.length} items`;
  }


  /**
   * Put the most recent delete back.
   *
   * Popped whatever the outcome: a delete that could not be restored will not restore on
   * a second try either, and leaving it on the stack would make <kbd>⌘Z</kbd> answer the
   * same failure forever instead of moving on to the one before it.
   */
  async function undo(): Promise<void> {
    const entry = stack.at(-1);
    if (!entry || busy) return;
    busy = true;
    stack = stack.slice(0, -1);
    try {
      const res = await ipcUndelete(entry.root, entry.change);
      projectStore.refreshTree();
      if (res.restored.length === 0) {
        toastStore.show(`Nothing to restore for ${entry.label}`, 'warning');
        return;
      }
      toastStore.show(
        res.restored.length === 1
          ? `Restored ${res.restored[0].split(/[\\/]/).pop()}`
          : `Restored ${res.restored.length} files`,
        'success',
      );
      if (res.skipped.length) {
        toastStore.show(
          `${res.skipped.length} left alone — something is already there`,
          'warning',
        );
      }
    } catch (e) {
      toastStore.show(e instanceof Error ? e.message : String(e), 'error');
    } finally {
      busy = false;
    }
  }

  return {
    get busy() { return busy; },
    /** What the next undo would put back, or `null` when there is nothing to undo. */
    get undoable() { return stack.at(-1) ?? null; },
    describe,

    /**
     * Delete `paths`, close whatever tabs they had open, and push the operation onto the
     * undo stack. The caller is responsible for having asked first.
     */
    async delete(root: string, paths: string[]): Promise<void> {
      if (busy || paths.length === 0) return;
      busy = true;
      try {
        const res = await ipcDelete(root, paths);
        for (const f of res.deleted) projectStore.closeFile(f);
        projectStore.refreshTree();

        if (res.failed.length) {
          toastStore.show(
            `${res.failed[0].path.split(/[\\/]/).pop()}: ${res.failed[0].error}`,
            'error',
          );
        }
        if (res.deleted.length === 0) return;

        const undoable = res.recorded > 0;
        if (undoable) {
          stack = [...stack, {
            root,
            change: res.change,
            count: res.deleted.length,
            label: describe(paths),
          }].slice(-DEPTH);
        }
        toastStore.show(
          res.deleted.length === 1
            ? `Deleted ${describe(paths)}`
            : `Deleted ${res.deleted.length} files`,
          'success',
          undoable ? 7000 : 5000,
          // The toast is where the undo is discovered, in the one moment it is wanted.
          // Absent when there is nothing kept to restore — offering an undo that cannot
          // work is worse than not offering one.
          undoable ? { label: 'Undo', onClick: () => void undo() } : undefined,
        );
        if (!undoable) {
          toastStore.show('Too many files to keep — this delete cannot be undone.', 'warning');
        }
      } catch (e) {
        toastStore.show(e instanceof Error ? e.message : String(e), 'error');
      } finally {
        busy = false;
      }
    },

    undo,
  };
}

export const bennuFileOpsStore = createFileOpsStore();
