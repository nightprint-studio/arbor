/**
 * useStudioTextDiff — owns the Text-view local buffer + debounced push
 * back to the store, plus the diff-refresh tick / hunk count / tree
 * change count read by `<StudioDiffPane>`.
 *
 * Single 180ms debounce matches the existing RON / JSON / YAML modals
 * (override via `debounceMs`). When the store's `current` shifts under
 * the wrapper (undo, external mutate, save reload), the effect syncs
 * `textBuf` so the CodeMirror buffer stays in lockstep.
 */

import { untrack } from 'svelte';

export interface TextDiffConfig {
  /** Store-level reactive read of the current document text. */
  getStoreCurrent: () => string;
  /** Push the new text back to the store (debounced internally). */
  setText: (text: string) => Promise<void>;
  /** Reload the tree once the store accepted the text. */
  reloadTree: () => Promise<void> | void;
  /** Debounce in ms. Default 180 — matches existing wrappers. */
  debounceMs?: number;
}

export interface TextDiff {
  // Text-view state.
  readonly textBuf: string;
  onTextInput(next: string): void;
  /** Cancel any pending debounced push — call from `closeDoc`. */
  cancelPendingTextPush(): void;

  // Diff-view state.
  readonly diffRefreshTick: number;
  diffHunkCount: number;
  diffTreeChangeCount: number;
  bumpDiffRefresh(): void;
}

export function useStudioTextDiff(config: TextDiffConfig): TextDiff {
  const debounce = config.debounceMs ?? 180;

  let textBuf            = $state('');
  let pushTimer: ReturnType<typeof setTimeout> | null = null;

  let diffRefreshTick     = $state(0);
  let diffHunkCount       = $state(0);
  let diffTreeChangeCount = $state(0);

  function scheduleTextPush() {
    if (pushTimer) clearTimeout(pushTimer);
    pushTimer = setTimeout(() => {
      void config.setText(textBuf).then(() => {
        void config.reloadTree();
        bumpDiffRefresh();
      });
    }, debounce);
  }

  function onTextInput(next: string) {
    textBuf = next;
    scheduleTextPush();
  }

  function bumpDiffRefresh() {
    untrack(() => { diffRefreshTick++; });
  }

  function cancelPendingTextPush() {
    if (pushTimer) { clearTimeout(pushTimer); pushTimer = null; }
  }

  // Sync the local buffer when the store's text shifts under us (undo,
  // external mutate, save-reload). Untracked write so we don't loop on
  // textBuf's own reactivity.
  $effect(() => {
    const c = config.getStoreCurrent();
    untrack(() => { textBuf = c; });
  });

  return {
    get textBuf() { return textBuf; },
    onTextInput,
    cancelPendingTextPush,

    get diffRefreshTick()    { return diffRefreshTick; },
    get diffHunkCount()      { return diffHunkCount; },
    set diffHunkCount(v: number) { diffHunkCount = v; },
    get diffTreeChangeCount()    { return diffTreeChangeCount; },
    set diffTreeChangeCount(v: number) { diffTreeChangeCount = v; },
    bumpDiffRefresh,
  };
}
