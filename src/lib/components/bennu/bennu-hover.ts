/**
 * Shared CodeMirror hover machinery for Bennu editors.
 *
 * Every Bennu language renders the SAME hover card — a signature line, a muted
 * `container · kind` meta line, and an optional doc body — so the card DOM lives here
 * once ({@link hoverCardDom}).
 *
 * On top of it, {@link makeHoverSource} carries the flow the **backend-resolved**
 * languages share: find the word under the pointer, map it to a UTF-8 byte offset, ask
 * the BE. The Java descriptor (`bennu_hover`) and the JSP one
 * (`bennu_action_property_hover`) differ only in which call they make.
 *
 * A language whose vocabulary is *closed* resolves locally instead and has no fetch to
 * supply (`.dig`, whose whole vocabulary is a table) — it builds its own tooltip and
 * reuses {@link hoverCardDom}, so the card stays one thing.
 */

import type { EditorView, Tooltip } from '@codemirror/view';
import { makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import type { HoverInfo } from '$lib/ipc/bennu/nav';

/** A backend hover fetch: `(file, buffer, byteOffset) → info | null`. */
export type HoverFetch = (file: string, source: string, byteOffset: number) => Promise<HoverInfo | null>;

const WORD = /[A-Za-z0-9_$]/;

/** What the hover card renders. A view type, deliberately not {@link HoverInfo}: the card
 *  is also built from a locally-resolved lookup (`.dig`), and tying it to the Java wire
 *  shape would force that caller to fake wire fields. `HoverInfo` satisfies it. */
export interface HoverCard {
  /** The signature line — the card's title. */
  signature: string;
  /** Owning type / namespace, for the muted meta line. */
  container?: string | null;
  /** What kind of thing it is, for the muted meta line. */
  kind?: string | null;
  /** The explanation body, below the meta line. */
  doc?: string | null;
}

/** Build the shared `.cm-hover-card` DOM (styled in the editor theme): the signature, an
 *  optional muted `container · kind` line, and an optional doc body. `textContent`
 *  throughout — a doc string is data from a project (or a Javadoc), never markup to run. */
export function hoverCardDom(info: HoverCard): HTMLElement {
  const dom = document.createElement('div');
  dom.className = 'cm-hover-card';

  const sig = document.createElement('div');
  sig.className = 'cm-hc-title';
  sig.textContent = info.signature;
  dom.appendChild(sig);

  const meta = [info.container, info.kind].filter((s): s is string => !!s);
  if (meta.length) {
    const m = document.createElement('div');
    m.className = 'cm-hc-meta';
    m.textContent = meta.join('  ·  ');
    dom.appendChild(m);
  }

  if (info.doc) {
    const d = document.createElement('div');
    d.className = 'cm-hc-doc';
    d.textContent = info.doc;
    dom.appendChild(d);
  }
  return dom;
}

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
