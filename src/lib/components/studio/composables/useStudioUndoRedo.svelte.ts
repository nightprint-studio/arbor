/**
 * useStudioUndoRedo — undo / redo dispatch shared by every Studio modal.
 * Trivial wrapper around the store's `undo()` / `redo()` that also
 * reloads the tree and bumps the diff view on success.
 */

export interface UndoRedoConfig {
  /** Store-level undo. Returns true if a step was applied. */
  undo: () => Promise<boolean>;
  redo: () => Promise<boolean>;
  /** Refresh the tree view (typically `treePane?.reloadTree()`). */
  reloadTree: () => Promise<void> | void;
  /** Bump the diff refresh tick so the Diff view recomputes. */
  bumpDiffRefresh: () => void;
}

export interface UndoRedo {
  doUndo(): Promise<void>;
  doRedo(): Promise<void>;
}

export function useStudioUndoRedo(config: UndoRedoConfig): UndoRedo {
  async function doUndo(): Promise<void> {
    const ok = await config.undo();
    if (!ok) return;
    await config.reloadTree();
    config.bumpDiffRefresh();
  }
  async function doRedo(): Promise<void> {
    const ok = await config.redo();
    if (!ok) return;
    await config.reloadTree();
    config.bumpDiffRefresh();
  }
  return { doUndo, doRedo };
}
