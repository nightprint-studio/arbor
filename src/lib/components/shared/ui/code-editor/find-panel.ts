/**
 * The bridge between CodeMirror's panel mechanism and the Svelte find widget.
 *
 * `search({ createPanel })` lets a host replace the stock search panel with its own DOM; this
 * returns that factory. The panel is only a mount point — the widget floats over the top-right of
 * the editor (see the `.cm-panels-top:has(.fw)` rule in `theme.ts`), so it does not push the code
 * down when it opens, and CodeMirror's scroll margins still keep the caret out from under it.
 */

import { flushSync, mount, unmount } from 'svelte';
import type { EditorView, Panel } from '@codemirror/view';
import FindWidget from './FindWidget.svelte';

export interface FindPanelOptions {
  /** Hand the query to a project-wide search. Omitted → no "Find in files" button. */
  onFindInFiles?: (query: string) => void;
}

/** The `createPanel` function for `search()`. */
export function createFindPanel(opts: FindPanelOptions = {}): (view: EditorView) => Panel {
  return (view) => {
    const dom = document.createElement('div');
    dom.className = 'arbor-find-host';
    let widget: ReturnType<typeof mount> | null = null;
    return {
      dom,
      top: true,
      mount() {
        widget = mount(FindWidget, {
          target: dom,
          props: { view, onFindInFiles: opts.onFindInFiles },
        });
        // `bind:element` is applied by an effect, so without a flush the field is not bound yet
        // and the first Ctrl+F opened the widget with the focus still in the code. The panel's
        // DOM is also attached only after `mount()` returns, so focus once more on the next tick.
        flushSync();
        const focus = () => (widget as { focusFind?: () => void } | null)?.focusFind?.();
        focus();
        queueMicrotask(focus);
      },
      update() {
        (widget as { refresh?: () => void } | null)?.refresh?.();
      },
      destroy() {
        if (widget) unmount(widget);
        widget = null;
      },
    };
  };
}
