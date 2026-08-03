/**
 * Shared CodeMirror hover machinery for Bennu editors.
 *
 * {@link makeHoverSource} carries the flow the **backend-resolved** languages share: find
 * the word under the pointer, map it to a UTF-8 byte offset, ask the BE. The Java
 * descriptor (`bennu_hover`) and the JSP one (`bennu_action_property_hover`) differ only
 * in which call they make.
 *
 * The card itself is `shared/ui/code-editor/hover-card` — one card for every editor in
 * Arbor, Picus's included — and is re-exported here so a Bennu language that resolves
 * *locally* (`.dig`, whose whole vocabulary is a table) builds its tooltip from the same
 * thing without reaching across the tree for it.
 */

import type { EditorView, Tooltip } from '@codemirror/view';
import { hoverCardDom, makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import type { HoverInfo } from '$lib/ipc/bennu/nav';

/** A backend hover fetch: `(file, buffer, byteOffset) → info | null`. */
export type HoverFetch = (file: string, source: string, byteOffset: number) => Promise<HoverInfo | null>;

const WORD = /[A-Za-z0-9_$]/;

// The card lives in `shared/ui/code-editor`; re-exported so a locally-resolved Bennu
// language (`.dig`) keeps importing it from one place. `HoverInfo` satisfies `HoverCard`.
export { hoverCardDom, type HoverCard } from '$lib/components/shared/ui/code-editor';

/** Build a `hoverTooltip` source that resolves the identifier under the pointer through `fetchInfo`
 *  and renders it as a `.cm-hover-card` (the shared card styled in the editor theme). Returns `null` gracefully
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
        return { dom: hoverCardDom(resolved) };
      },
    };
  };
}
