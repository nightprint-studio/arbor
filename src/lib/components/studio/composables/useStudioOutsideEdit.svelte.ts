/**
 * useStudioOutsideEdit — pointer-down-outside-active-inline-edit watcher.
 *
 * When the user is inline-editing a tree row and clicks anywhere outside
 * the input/select element, treat it as an implicit "commit". Mirrors the
 * UX of file renames in IntelliJ: typing + click-away = saved; typing +
 * Escape = cancel.
 *
 * The composable owns the listener lifecycle (capturing pointerdown so
 * we see the click before any nested handler can stop it) and bails out
 * when:
 *   · no inline edit is active,
 *   · the active edit has a validation error (pending error pill — let
 *     the user fix it before we commit),
 *   · the active edit lives in the detail / inspector pane (i.e.
 *     `editLocation !== 'tree'`),
 *   · the click landed on (or inside) the active input/select itself.
 *
 * Wrappers wire it as a single $effect:
 *
 *   useStudioOutsideEdit({
 *     editPipeline,
 *     getSelectedNode: () => selectedNode,
 *   });
 */

import type { EditLocation } from './useStudioEditPipeline.svelte';

interface EditPipelineLike<TNode> {
  readonly editingPid:        string | null;
  readonly editError:         string | null;
  /** Where the active inline edit lives. Matches the canonical
   *  `EditLocation` from `useStudioEditPipeline` (`'tree' | 'detail'`);
   *  `null` is kept for "no edit active" so the composable can read the
   *  field uniformly before checking `editingPid`. */
  readonly editLocation:      EditLocation | null;
  readonly editInlineEl:      HTMLInputElement  | undefined;
  readonly editInlineSelectEl: HTMLSelectElement | undefined;
  maybeCommitActiveEdit(node: TNode | null): Promise<void>;
}

export interface OutsideEditConfig<TNode> {
  editPipeline:    EditPipelineLike<TNode>;
  getSelectedNode: () => TNode | null;
}

export function useStudioOutsideEdit<TNode>(config: OutsideEditConfig<TNode>): void {
  $effect(() => {
    function onDown(e: PointerEvent) {
      const ep = config.editPipeline;
      if (!ep.editingPid)              return;
      if (ep.editError)                return;
      if (ep.editLocation !== 'tree')  return;
      const el = ep.editInlineEl ?? ep.editInlineSelectEl;
      if (!el)                         return;
      const target = e.target as Node | null;
      if (target && (el === target || el.contains(target))) return;
      void ep.maybeCommitActiveEdit(config.getSelectedNode());
    }
    window.addEventListener('pointerdown', onDown, true);
    return () => window.removeEventListener('pointerdown', onDown, true);
  });
}
