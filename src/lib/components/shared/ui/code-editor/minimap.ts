/**
 * A scrollable **minimap** in the editor's right gutter (VS Code / IntelliJ style), over the
 * framework-agnostic `@replit/codemirror-minimap` facet.
 *
 * The package renders the whole document as a tiny overview with a viewport overlay you can drag /
 * click to navigate. It owns its own DOM inside the container we hand it via `create`; we only pick
 * the render style. Kept app-agnostic (no Arbor imports) so it belongs in the shared editor core.
 */

import type { Extension } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { showMinimap } from '@replit/codemirror-minimap';

/** The minimap extension — a block-rendered overview with an always-visible viewport overlay. */
export function minimapExtension(): Extension {
  const create = (_view: EditorView) => {
    const dom = document.createElement('div');
    return { dom };
  };
  return showMinimap.of({
    create,
    displayText: 'blocks',
    showOverlay: 'always',
  });
}
