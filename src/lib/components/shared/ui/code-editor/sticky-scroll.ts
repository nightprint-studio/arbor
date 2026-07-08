/**
 * Sticky scroll — pins the enclosing declaration lines (class › method › block) to the top of
 * the editor as you scroll into a body, so the class signature (and the method you're in) stays
 * visible. The IntelliJ / VS Code "sticky scroll" affordance.
 *
 * Language-agnostic: the enclosing scopes are found by walking UP from the top visible line and
 * collecting each line whose indentation is strictly shallower than the running minimum (skipping
 * blank lines and pure closing-brace lines). No syntax tree needed, so it works in every language
 * the editor hosts. Each pinned row is clickable to jump to that line.
 */

import { EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import { highlightToHtml } from './mini-highlight';

/** Max pinned rows (outermost type › … › innermost block). Keeps the header from eating the view. */
const MAX_ROWS = 4;

/** Leading-whitespace width of `text` in columns (a tab jumps to the next `tabSize` stop). */
function indentCols(text: string, tabSize: number): number {
  let col = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === ' ') col++;
    else if (c === '\t') col += tabSize - (col % tabSize);
    else break;
  }
  return col;
}

/** The enclosing "header" line numbers for `topLine`, outermost first. A header is a non-blank,
 *  non-closing line strictly shallower than every header collected so far — i.e. the scope chain
 *  (`class` › `method` › `if`/`for`) that contains the top of the viewport. */
function enclosingHeaders(view: EditorView, topLine: number, tabSize: number): number[] {
  const doc = view.state.doc;
  const headers: number[] = [];
  let minIndent = indentCols(doc.line(topLine).text, tabSize);
  for (let n = topLine - 1; n >= 1 && headers.length < MAX_ROWS; n--) {
    const text = doc.line(n).text;
    const trimmed = text.trim();
    if (!trimmed) continue; // blank
    const ind = indentCols(text, tabSize);
    if (ind >= minIndent) continue; // sibling / deeper — not an enclosing scope
    // A line that only CLOSES scope (`}`, `};`, `)`) is never a meaningful header even when it
    // happens to sit at a shallower indent.
    if (trimmed[0] === '}' || trimmed[0] === ')') {
      minIndent = ind;
      continue;
    }
    headers.push(n);
    minIndent = ind;
  }
  return headers.reverse();
}

export function stickyScroll(): Extension {
  const plugin = ViewPlugin.fromClass(
    class {
      readonly el: HTMLElement;
      lastKey = '';

      readonly onScroll: () => void;

      constructor(readonly view: EditorView) {
        this.el = document.createElement('div');
        this.el.className = 'cm-sticky';
        view.dom.appendChild(this.el);
        // Small scrolls within CodeMirror's rendered margin don't fire a `ViewUpdate`, so the pinned
        // set is refreshed straight off the scroll event (rAF-coalesced) for a smooth follow.
        let raf = 0;
        this.onScroll = () => {
          if (raf) return;
          raf = requestAnimationFrame(() => { raf = 0; this.render(); });
        };
        view.scrollDOM.addEventListener('scroll', this.onScroll, { passive: true });
        this.render();
      }

      update(u: ViewUpdate) {
        // Scroll is driven by the (rAF-coalesced) scroll listener; here we only react to edits and
        // layout changes, so a scroll doesn't render twice per frame (a source of the flicker).
        if (u.docChanged || u.geometryChanged) this.render();
      }

      render() {
        const view = this.view;
        const doc = view.state.doc;
        const tabSize = view.state.tabSize;
        // The line at the very top of the visible area (accounting for the current scroll).
        const topPos = view.lineBlockAtHeight(view.scrollDOM.scrollTop).from;
        const topLine = doc.lineAt(topPos).number;
        const headers = enclosingHeaders(view, topLine, tabSize).filter((n) => n < topLine);

        // Change-guard: only touch the DOM when the pinned set actually changes.
        const key = headers.join(',');
        if (key === this.lastKey) return;
        this.lastKey = key;

        this.el.textContent = '';
        if (headers.length === 0) {
          this.el.style.display = 'none';
          return;
        }
        this.el.style.display = 'block';
        // Align the pinned text with the code below it: offset past the line-number gutter (the row's
        // own 12px pad then lands on the first glyph, matching a `.cm-line`).
        const gutter = view.contentDOM.getBoundingClientRect().left - view.dom.getBoundingClientRect().left;
        this.el.style.paddingLeft = `${Math.max(0, gutter)}px`;
        for (const n of headers) {
          const row = document.createElement('div');
          row.className = 'cm-sticky-row';
          // Highlighted like the buffer (the header line is scrolled off-screen, so its DOM isn't
          // rendered to clone — we re-highlight the text with the shared token classes).
          row.innerHTML = highlightToHtml(doc.line(n).text.replace(/\s+$/, '')) || '&nbsp;';
          row.addEventListener('mousedown', (e) => {
            e.preventDefault();
            const pos = doc.line(n).from;
            view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: 'start' }) });
            view.focus();
          });
          this.el.appendChild(row);
        }
      }

      destroy() {
        this.view.scrollDOM.removeEventListener('scroll', this.onScroll);
        this.el.remove();
      }
    },
  );
  return [plugin, stickyTheme];
}

const stickyTheme = EditorView.baseTheme({
  '.cm-sticky': {
    position: 'absolute',
    top: '0',
    left: '0',
    right: '0',
    zIndex: '6',
    background: 'var(--bg-base, #1e1e22)',
    borderBottom: '1px solid var(--border, #3a3a40)',
    boxShadow: '0 2px 6px rgba(0,0,0,0.25)',
    // Match the editor's own font/size so the pinned lines read exactly like the code below them.
    fontFamily: 'var(--font-code)',
    fontSize: '12.5px',
    lineHeight: '1.55',
  },
  '.cm-sticky-row': {
    padding: '0 12px',
    whiteSpace: 'pre',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    cursor: 'pointer',
    color: 'var(--text-primary)',
  },
  '.cm-sticky-row:hover': { background: 'color-mix(in srgb, var(--accent, #4a9eff) 12%, transparent)' },
});
