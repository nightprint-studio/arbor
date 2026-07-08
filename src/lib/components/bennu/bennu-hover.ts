/**
 * Shared CodeMirror `hoverTooltip` source factory for Bennu editors.
 *
 * Both the Java descriptor (symbol signatures + `var`/`val` types via `bennu_hover`) and the JSP
 * descriptor (OGNL/form-field → action-property types via `bennu_action_property_hover`) render the
 * SAME hover card — a signature line, a muted `container · kind` meta line, and an optional Javadoc.
 * Only the async fetch differs, so the whole "find the word under the pointer, map to a byte offset,
 * build the DOM" flow lives here and each descriptor supplies just its fetch function.
 */

import type { EditorView, Tooltip } from '@codemirror/view';
import { makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import type { HoverInfo } from '$lib/ipc/bennu/nav';

/** A backend hover fetch: `(file, buffer, byteOffset) → info | null`. */
export type HoverFetch = (file: string, source: string, byteOffset: number) => Promise<HoverInfo | null>;

const WORD = /[A-Za-z0-9_$]/;

/** Build a `hoverTooltip` source that resolves the identifier under the pointer through `fetchInfo`
 *  and renders it as a `.bennu-hover` card (styled in the editor theme). Returns `null` gracefully
 *  when there's no active file, no word under the pointer, or the backend has nothing to say. */
export function makeHoverSource(fetchInfo: HoverFetch) {
  return async function hoverSource(view: EditorView, pos: number, _side: -1 | 1): Promise<Tooltip | null> {
    const path = projectStore.activeFilePath;
    if (!path) return null;

    // Expand the identifier word around `pos`.
    const line = view.state.doc.lineAt(pos);
    const text = line.text;
    const rel = pos - line.from;
    let s = rel;
    let e = rel;
    while (s > 0 && WORD.test(text[s - 1])) s--;
    while (e < text.length && WORD.test(text[e])) e++;
    if (s === e) return null;
    const from = line.from + s;
    const to = line.from + e;

    // Map the middle of the word to a UTF-8 byte offset (the BE classifier biases left by one).
    const src = view.state.doc.toString();
    const u2b = makeU16ToByte(src);
    const byteOffset = u2b(from + Math.floor((e - s) / 2));

    let info: HoverInfo | null;
    try {
      info = await fetchInfo(path, src, byteOffset);
    } catch {
      return null;
    }
    if (!info) return null;
    const resolved = info;

    return {
      pos: from,
      end: to,
      above: true,
      create() {
        const dom = document.createElement('div');
        dom.className = 'bennu-hover';
        const sig = document.createElement('div');
        sig.className = 'bh-sig';
        sig.textContent = resolved.signature;
        dom.appendChild(sig);
        const meta: string[] = [];
        if (resolved.container) meta.push(resolved.container);
        if (resolved.kind) meta.push(resolved.kind);
        if (meta.length) {
          const m = document.createElement('div');
          m.className = 'bh-meta';
          m.textContent = meta.join('  ·  ');
          dom.appendChild(m);
        }
        if (resolved.doc) {
          const d = document.createElement('div');
          d.className = 'bh-doc';
          d.textContent = resolved.doc;
          dom.appendChild(d);
        }
        return { dom };
      },
    };
  };
}
