/**
 * useStudioGlobalKeys — global keyboard handler shared by every Studio
 * modal. Builds an `onKey(e)` ready to mount on `<svelte:window
 * on:keydown={...}>` from the wrapper.
 *
 * Covers: Ctrl/Cmd+S (save), Ctrl/Cmd+Z (undo), Ctrl/Cmd+Shift+Z and
 * Ctrl/Cmd+Y (redo), F3 (next/prev hit — routes to query bar in Tree
 * view and to diff in Diff view), F2 (start edit on a tree row — mode-
 * aware), Delete (remove selection in Tree view).
 */

export interface GlobalKeysConfig<TKind extends string, TNode> {
  isOpen: () => boolean;
  doSave: () => Promise<void> | void;
  doUndo: () => Promise<void> | void;
  doRedo: () => Promise<void> | void;

  getViewMode:     () => 'tree' | 'text' | 'diff' | 'errors';
  getSelectedNode: () => TNode | null;
  getEditingPid:   () => string | null;

  /** Open the primitive editor — wrapper threads its `editPipeline.startEdit`. */
  startEdit: (node: TNode, location: 'tree' | 'detail') => void;
  /** Open the variant picker. */
  startVariantEdit: (node: TNode, location: 'tree' | 'detail') => void;

  rowEditMode:     (node: TNode) => 'primitive' | 'variant' | null;
  isPromotableNull?: (kind: TKind) => boolean;
  isRemovable:     (node: TNode | null) => boolean;
  removeSelected:  () => Promise<void> | void;

  /** Query bar controller — F3 navigates hits in Tree view. */
  getQueryBarController: () => { nav(d: number): void } | undefined;
  /** Diff pane controller — F3 navigates hunks in Diff view. */
  getDiffPaneController: () => { nav(d: number): void } | undefined;
}

function inEditableField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLInputElement)    return true;
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLSelectElement)   return true;
  const cm = target.closest('.cm-editor');
  return cm !== null && cm !== undefined;
}

export function useStudioGlobalKeys<TKind extends string, TNode extends { kind: TKind; pid: string }>(
  config: GlobalKeysConfig<TKind, TNode>,
): { onKey: (e: KeyboardEvent) => void } {
  function onKey(e: KeyboardEvent) {
    if (!config.isOpen()) return;
    const editable = inEditableField(e.target);

    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && (e.key === 's' || e.key === 'S')) {
      e.preventDefault(); e.stopPropagation();
      void config.doSave();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && (e.key === 'z' || e.key === 'Z')) {
      if (!editable) { e.preventDefault(); void config.doUndo(); }
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === 'z' || e.key === 'Z')) {
      if (!editable) { e.preventDefault(); void config.doRedo(); }
      return;
    }
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && (e.key === 'y' || e.key === 'Y')) {
      if (!editable) { e.preventDefault(); void config.doRedo(); }
      return;
    }

    const viewMode = config.getViewMode();
    if (e.key === 'F3') {
      if (viewMode === 'tree') {
        e.preventDefault();
        config.getQueryBarController()?.nav(e.shiftKey ? -1 : 1);
      } else if (viewMode === 'diff') {
        e.preventDefault();
        config.getDiffPaneController()?.nav(e.shiftKey ? -1 : 1);
      }
      return;
    }

    if (e.key === 'F2' && viewMode === 'tree' && !editable) {
      const node = config.getSelectedNode();
      if (node && !config.getEditingPid()) {
        const mode = config.rowEditMode(node);
        if (mode === 'variant') {
          e.preventDefault();
          config.startVariantEdit(node, 'tree');
        } else if (mode === 'primitive') {
          e.preventDefault();
          config.startEdit(node, 'tree');
        } else if (config.isPromotableNull?.(node.kind)) {
          e.preventDefault();
          config.startEdit(node, 'tree');
        }
      }
      return;
    }

    if (e.key === 'Delete' && viewMode === 'tree' && !editable) {
      const node = config.getSelectedNode();
      if (config.isRemovable(node)) {
        e.preventDefault();
        void config.removeSelected();
      }
      return;
    }
  }

  return { onKey };
}
