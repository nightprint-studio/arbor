/**
 * The generic CodeMirror 6 extension set for the shared code editor.
 *
 * `createCodeEditorExtensions(lang, opts)` assembles the standard editing surface
 * (theme + gutters + history + selection + bracket-matching + active-line +
 * selection-match highlight + search + the Tree-sitter highlight plugin + lint
 * gutter + a keymap) around a {@link LanguageDescriptor}. It is app-agnostic: no
 * Arbor domain imports, only `@codemirror/*`, `web-tree-sitter`, and the sibling
 * core files.
 *
 * As in merula, CodeMirror's search panel IS wired (so `Ctrl+F` searches the buffer
 * when the editor is focused), but its *open* binding is removed from the keymap so
 * the host owns `Ctrl+F` routing and calls `openSearch()` imperatively. The
 * in-panel navigation (next / previous / replace / close) stays, so search is fully
 * keyboard-driven once opened.
 */

import {
  EditorView, lineNumbers, keymap,
  highlightActiveLine, highlightActiveLineGutter, drawSelection,
} from '@codemirror/view';
import { EditorState, type Extension, type Text } from '@codemirror/state';
import { history, defaultKeymap, historyKeymap, indentWithTab, deleteLine } from '@codemirror/commands';
import { bracketMatching, indentOnInput, foldKeymap } from '@codemirror/language';
import { lintGutter, lintKeymap } from '@codemirror/lint';
import { search, searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import { autocompletion, completionKeymap } from '@codemirror/autocomplete';

import type { LanguageDescriptor, Tree, Node } from './types';
import { createHighlightPlugin } from './highlight';
import { createFoldingExtension } from './folding';
import { codeEditorTheme } from './theme';

/** The search keymap minus its open binding: the host owns `Ctrl+F` so it can route
 *  it to the editor (when the pane is focused) or elsewhere, and calls `openSearch()`
 *  imperatively. The in-panel navigation stays so search is keyboard-complete. */
const searchKeymapNoOpen = searchKeymap.filter((b) => b.key !== 'Mod-f');

export interface CodeEditorExtensionsOptions {
  readOnly?: boolean;
  /** Ctrl/Cmd+Click on an identifier — the host resolves + jumps (go-to-decl). The
   *  descriptor's `resolveGoto` (when present) is tried first for a local jump; when
   *  it returns null (or is absent), the bare word is handed to `onGoto`. */
  onGoto?: (word: string, view: EditorView) => void;
  /** Language-intelligence hook bag (reserved; opaque to the core today). */
  intel?: unknown;
}

/** Build the full extension set for one editor bound to `lang`. Returns the
 *  extensions plus the `getTree` reader (so a host component can implement
 *  go-to-decl / structure against the same live tree the highlighter maintains). */
export function createCodeEditorExtensions(
  lang: LanguageDescriptor,
  opts: CodeEditorExtensionsOptions = {},
): { extensions: Extension; getTree: ReturnType<typeof createHighlightPlugin>['getTree'] } {
  const { plugin: highlight, getTree } = createHighlightPlugin(lang);

  // Client-side folding — installed only when the descriptor opts in via
  // `foldNode`. It reads the same live tree the highlighter maintains (no
  // backend), so folding is free once a grammar loads.
  const folding = createFoldingExtension(lang, getTree);

  const exts: Extension[] = [
    codeEditorTheme,
    lineNumbers(),
    history(),
    drawSelection(),
    indentOnInput(),
    bracketMatching(),
    folding,
    highlightActiveLine(),
    highlightActiveLineGutter(),
    highlightSelectionMatches(),
    search({ top: true }),
    highlight,
    lintGutter(),
  ];

  // Language intelligence (autocomplete) — only when the descriptor supplies a
  // completion source and the editor is editable. Added before the base keymap
  // so its completion keymap (Enter / ↑↓ / Esc while the popup is open) wins.
  const completionSource = lang.intel?.completion;
  if (completionSource && !opts.readOnly) {
    exts.push(autocompletion({ override: [completionSource], defaultKeymap: false }));
  }

  exts.push(
    keymap.of([
      // IntelliJ: Ctrl+Y deletes the current line. First in the list so it wins over
      // the Windows redo binding (Mod-y) that historyKeymap also maps to Ctrl-y.
      { key: 'Ctrl-y', run: deleteLine, preventDefault: true },
      ...defaultKeymap, ...historyKeymap, ...lintKeymap, ...foldKeymap,
      ...(completionSource && !opts.readOnly ? completionKeymap : []),
      ...searchKeymapNoOpen, indentWithTab,
    ]),
    EditorState.readOnly.of(!!opts.readOnly),
  );

  // Go-to-declaration (Ctrl/Cmd+Click). Try the descriptor's local resolver first;
  // otherwise report the identifier under the cursor to the host.
  if (opts.onGoto) {
    const { onGoto } = opts;
    exts.push(EditorView.domEventHandlers({
      mousedown(event, view) {
        if (!(event.ctrlKey || event.metaKey)) return false;
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos == null) return false;
        // Tree-driven paths (local jump + identifier) need the live tree; the
        // string/path fallback below works on line text, so it stays available
        // even before the grammar wasm has loaded.
        const tree = getTree(view);
        if (tree) {
          // Local resolver wins when it can jump within the buffer.
          if (lang.resolveGoto) {
            const target = lang.resolveGoto(tree, pos);
            if (target) {
              event.preventDefault();
              view.dispatch({
                selection: { anchor: target.offset },
                effects: EditorView.scrollIntoView(target.offset, { y: 'center' }),
              });
              view.focus();
              return true;
            }
          }
          // An identifier under the cursor (a class/method name) wins.
          const word = identifierTextAt(tree, pos);
          if (word) {
            event.preventDefault();
            onGoto(word, view);
            return true;
          }
        }
        // Otherwise, a reference-like token (a string-literal's contents or a
        // path such as `/do/Category/viewTree`) — so Ctrl/Cmd+Click on a JSP
        // action string resolves too. Generic, no product knowledge, no tree.
        const ref = refTextAt(view.state.doc, pos);
        if (ref) {
          event.preventDefault();
          onGoto(ref, view);
          return true;
        }
        return false;
      },
    }));
  }

  return { extensions: exts, getTree };
}

/** Characters that make up a "reference-like" token (identifiers, plus the path /
 *  segment punctuation that appears in action refs like `/do/Category/viewTree` and
 *  `bando-search`). Deliberately generic — not Java/JSP-specific. */
const REF_CHAR = /[A-Za-z0-9_$/.\-]/;

/** The reference-like token at UTF-16 `offset`, or null. If the offset sits inside a
 *  quoted string (`'…'` or `"…"`) on its line, returns the string's inner contents
 *  (a JSP `action="…"` value); otherwise the maximal run of {@link REF_CHAR} around
 *  the offset. Language-agnostic (operates on line text, no CST), so the core stays
 *  app-neutral while still surfacing path/string references to the host's `onGoto`. */
export function refTextAt(doc: Text, offset: number): string | null {
  const clamped = Math.max(0, Math.min(offset, doc.length));
  const line = doc.lineAt(clamped);
  const text = line.text;
  const rel = clamped - line.from;

  // If the offset is inside a quoted string on this line, prefer its contents.
  const quoted = quotedStringAround(text, rel);
  if (quoted !== null) return quoted.length ? quoted : null;

  // Otherwise expand a REF_CHAR run around the offset (tolerant of the caret sitting
  // at the token's right edge, matching `wordAtCaret`).
  let start = rel;
  let end = rel;
  while (start > 0 && REF_CHAR.test(text[start - 1])) start--;
  while (end < text.length && REF_CHAR.test(text[end])) end++;
  const tok = text.slice(start, end).trim();
  return tok.length ? tok : null;
}

/** If position `rel` (a column into `text`) falls within a single-line quoted string,
 *  return the string's inner contents; else null. Scans quotes left-to-right so an
 *  even count before `rel` means "outside", odd means "inside". Handles both quote
 *  styles independently (the first-opened wins). */
function quotedStringAround(text: string, rel: number): string | null {
  for (const q of ['"', "'"]) {
    let i = 0;
    while (i < text.length) {
      const open = text.indexOf(q, i);
      if (open === -1) break;
      const close = text.indexOf(q, open + 1);
      if (close === -1) break;
      // Inside (or on either quote) of this pair?
      if (rel >= open && rel <= close) return text.slice(open + 1, close);
      i = close + 1;
    }
  }
  return null;
}

/** The smallest `identifier`-ish leaf covering `offset` (UTF-16), or null — the
 *  Ctrl+Click word. Generic: matches any leaf whose type contains `identifier`
 *  (covers `identifier`, `type_identifier`, `field_identifier`, …). */
function identifierTextAt(tree: Tree, offset: number): string | null {
  const node = tree.rootNode.descendantForIndex(offset);
  if (!node) return null;
  let cur: Node | null = node;
  while (cur) {
    if (cur.type.includes('identifier') && cur.childCount === 0) return cur.text;
    cur = cur.parent;
  }
  return null;
}
