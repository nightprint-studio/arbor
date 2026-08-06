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
  EditorView, lineNumbers, keymap, hoverTooltip,
  highlightActiveLine, highlightActiveLineGutter, drawSelection,
  ViewPlugin, Decoration, type DecorationSet, type KeyBinding, type PluginValue, type ViewUpdate,
} from '@codemirror/view';
import { EditorState, StateField, StateEffect, Prec, type Extension, type Text } from '@codemirror/state';
import {
  history, defaultKeymap, historyKeymap, indentWithTab, deleteLine,
  moveLineUp, moveLineDown,
} from '@codemirror/commands';
import { bracketMatching, indentOnInput, foldKeymap, foldGutter, codeFolding } from '@codemirror/language';
import { lintGutter, lintKeymap } from '@codemirror/lint';
import {
  search, searchKeymap, highlightSelectionMatches, selectNextOccurrence,
} from '@codemirror/search';
import {
  autocompletion, completionKeymap, startCompletion, acceptCompletion,
  closeBrackets, closeBracketsKeymap,
} from '@codemirror/autocomplete';

import type { LanguageDescriptor, Tree, Node } from './types';
import { createHighlightPlugin } from './highlight';
import { createFoldingExtension } from './folding';
import { emmetKeymap } from './emmet';
import { duplicateSelection } from './commands';
import { rainbowBrackets } from './rainbow-brackets';
import { indentGuides } from './indent-guides';
import { stickyScroll } from './sticky-scroll';
import { scrollbarOverview } from './scrollbar-overview';
import { codeEditorTheme, codeEditorHighlightStyle } from './theme';
import {
  inlineCompletion, acceptInlineCompletion, dismissInlineCompletion,
} from './inline-completion';

/** The search keymap minus the bindings the host owns. The host routes `Ctrl+F` to the
 *  focused editor via `openSearch()` (so it removes `Mod-f`), and owns `Ctrl+G` as a
 *  Go-to-line overlay — leaving CM's `Mod-g` (find-next) / `Mod-Alt-g` (goto-line) bound
 *  here would make Ctrl+G *also* pop the search panel and fight the overlay for focus.
 *  In-panel navigation (F3 next/prev, Enter, Esc close, replace) stays, so search is
 *  fully keyboard-driven once opened. */
const HOST_OWNED_SEARCH_KEYS = new Set(['Mod-f', 'Mod-g', 'Mod-Alt-g']);
const searchKeymapNoOpen = searchKeymap.filter((b) => !b.key || !HOST_OWNED_SEARCH_KEYS.has(b.key));

/** A vertical margin guide (IntelliJ's "hard-wrap" ruler) at `column`. Rendered as a
 *  1px line inside the scroller so it scrolls with the content on both axes and paints
 *  above the active-line background. `column` is measured in default-font characters
 *  from the first glyph (the line-number gutter is excluded). */
export function editorRuler(column: number) {
  return ViewPlugin.fromClass(
    class implements PluginValue {
      readonly el: HTMLElement;
      constructor(view: EditorView) {
        this.el = document.createElement('div');
        this.el.className = 'cm-ruler';
        view.scrollDOM.appendChild(this.el);
        this.reposition(view);
      }
      update(u: ViewUpdate) {
        // Gutter width (doc length crossing 10/100/…), font metrics, resize, and the
        // content height (which our explicit height tracks so the line spans the whole
        // scrolled document).
        if (u.docChanged || u.geometryChanged || u.viewportChanged) this.reposition(u.view);
      }
      reposition(view: EditorView) {
        // Batched read→write so we never force a synchronous reflow mid-update. The
        // content sits right of the gutter; `.cm-line` adds a 12px left pad (theme). The
        // height is the full document height so the guide scrolls with the content
        // instead of sticking to the visible box.
        view.requestMeasure({
          read: (v) => ({
            left: v.contentDOM.offsetLeft + 12 + v.defaultCharacterWidth * column,
            height: v.contentHeight,
          }),
          write: ({ left, height }) => {
            this.el.style.left = `${left}px`;
            this.el.style.height = `${height}px`;
          },
        });
      }
      destroy() { this.el.remove(); }
    },
  );
}

export interface CodeEditorExtensionsOptions {
  readOnly?: boolean;
  /**
   * Show the line-number gutter. `true` by default — a buffer is navigated by line.
   *
   * Turned off for a **short input** that happens to want an editor: a structural query is two
   * or three lines of code with holes in it, and it wants the highlighting and the completion
   * without the chrome. A gutter numbering three lines is a column of noise beside a field, and
   * "line 2" is not how anyone refers to a part of a query they can see all of.
   */
  lineNumbers?: boolean;
  /** Draw a vertical margin guide at this 1-based character column (IntelliJ-style).
   *  Omitted / ≤ 0 → no ruler. */
  rulerColumn?: number;
  /** Ctrl/Cmd+Click on an identifier — the host resolves + jumps (go-to-decl). The
   *  descriptor's `resolveGoto` (when present) is tried first for a local jump; when
   *  it returns null (or is absent), the bare word is handed to `onGoto`, along with the
   *  clicked position as a **UTF-8 byte offset** so the host can classify it (a BE
   *  go-to-declaration needs the offset, not just the name). */
  onGoto?: (word: string, view: EditorView, byteOffset: number) => void;
  /** Enable Emmet abbreviation expansion on Tab (markup buffers). Off by default — the host
   *  opts in per markup file (HTML / JSP); the binding no-ops (falls through to indent) when the
   *  caret isn't on a valid abbreviation. */
  emmet?: boolean;
  /** Draw indentation guides (faint vertical lines per indent level, active block brightened). */
  indentGuides?: boolean;
  /** Pin the enclosing declaration lines (class › method › block) to the top as you scroll. */
  stickyScroll?: boolean;
  /** Replace the native vertical scrollbar with the IntelliJ-style overview strip (diagnostic
   *  marks + hover preview). A host enables this INSTEAD of the minimap. */
  scrollbarOverview?: boolean;
  /** Language-intelligence hook bag (reserved; opaque to the core today). */
  intel?: unknown;
  /**
   * Bindings the **host** owns, installed above everything CodeMirror binds.
   *
   * Without this an editor silently eats its window's shortcuts: `Mod-Enter` is
   * `insertBlankLine` in `defaultKeymap`, so a host that means "run the selection"
   * gets a blank line and no run, and the failure is invisible — the key does
   * *something*, just not the thing. Anything listed here wins outright.
   *
   * Return `true` from a binding to consume the key. Returning `false` lets it
   * fall through to CodeMirror, which is how a host claims a key conditionally.
   */
  keyBindings?: readonly KeyBinding[];
}

/** Build the full extension set for one editor bound to `lang`. Returns the
 *  extensions plus the `getTree` reader (so a host component can implement
 *  go-to-decl / structure against the same live tree the highlighter maintains). */
export function createCodeEditorExtensions(
  lang: LanguageDescriptor,
  opts: CodeEditorExtensionsOptions = {},
): { extensions: Extension; getTree: ReturnType<typeof createHighlightPlugin>['getTree'] } {
  const { plugin: highlight, getTree } = createHighlightPlugin(lang);

  // A descriptor may bring its own CodeMirror language extension (a `LanguageSupport`
  // / `StreamLanguage` for XML, YAML, JSON, …) instead of a tree-sitter grammar. When
  // it does, we install that (highlighted by the shared Lezer style) and skip the
  // tree-sitter plugin entirely; `getTree` then stays inert (the plugin never mounts),
  // so tree-driven folding/goto simply offer nothing — safe.
  const useCm = !!lang.cmExtension;

  // Client-side folding — installed only when the descriptor opts in via
  // `foldNode`. It reads the same live tree the highlighter maintains (no
  // backend), so folding is free once a grammar loads.
  const folding = createFoldingExtension(lang, getTree);

  const exts: Extension[] = [
    codeEditorTheme,
    codeEditorHighlightStyle,
    // Absent, not empty: an unwanted gutter still costs its horizontal column.
    ...(opts.lineNumbers === false ? [] : [lineNumbers()]),
    history(),
    drawSelection(),
    indentOnInput(),
    bracketMatching(),
    // Depth-tinted brackets (matching open/close share a hue) so a block reads at a glance —
    // composes with `bracketMatching`'s caret-match highlight above.
    rainbowBrackets(),
    // Auto-close paired delimiters — `(`/`[`/`{`/`"`/`'` insert their match, typing the
    // closer over an auto-inserted one skips it, and Backspace on an empty pair deletes
    // both (via `closeBracketsKeymap`). Language-aware: the pairs come from the language
    // data (`closeBrackets`), so a JSP/XML descriptor won't try to close a `'` inside text.
    closeBrackets(),
    folding,
    highlightActiveLine(),
    highlightActiveLineGutter(),
    highlightSelectionMatches(),
    search({ top: true }),
    useCm ? (lang.cmExtension as Extension) : highlight,
    lintGutter(),
  ];

  // Comment syntax for `Ctrl+/` (`toggleComment`, already in the default keymap). A
  // tree-sitter descriptor bypasses CodeMirror's `Language`, so it carries no comment
  // data — surface the descriptor's `commentTokens` via the languageData facet. A
  // `cmExtension` language brings its own, so this stays unset there.
  if (lang.commentTokens) {
    const ct = lang.commentTokens;
    exts.push(EditorState.languageData.of(() => [{ commentTokens: ct }]));
  }

  // Editing behaviours the language owns (escaped paste into a string literal, …).
  // Independent of how it highlights — a tree-sitter descriptor has no `cmExtension`
  // to compose them into.
  if (lang.editing) exts.push(lang.editing);

  // Lezer folding for a `cmExtension` language that opts in (`cmFold`) — drives the
  // fold gutter from the language's own `foldNodeProp` (e.g. `lang-html` folds tag
  // bodies, `lang-json` folds objects). Tree-sitter descriptors fold via `foldNode`
  // above; legacy StreamLanguage modes stay gutter-free (they carry no fold info).
  if (useCm && lang.cmFold) {
    exts.push(
      codeFolding(),
      foldGutter({
        markerDOM(open) {
          const el = document.createElement('span');
          el.className = open ? 'cm-foldMarker cm-foldMarker-open' : 'cm-foldMarker cm-foldMarker-closed';
          el.textContent = open ? '▾' : '▸';
          return el;
        },
      }),
    );
  }

  // Optional vertical margin guide (host opt-in via `rulerColumn`).
  if (opts.rulerColumn && opts.rulerColumn > 0) exts.push(editorRuler(opts.rulerColumn));

  // IntelliJ-flavoured chrome, each host opt-in (Bennu enables them; other products keep their
  // current look until they opt in too).
  if (opts.indentGuides) exts.push(indentGuides());
  if (opts.stickyScroll) exts.push(stickyScroll());
  if (opts.scrollbarOverview) exts.push(scrollbarOverview());

  // Language intelligence (autocomplete) — only when the descriptor supplies a
  // completion source and the editor is editable. Added before the base keymap
  // so its completion keymap (Enter / ↑↓ / Esc while the popup is open) wins.
  const completionSource = lang.intel?.completion;
  if (completionSource && !opts.readOnly) {
    exts.push(autocompletion({ override: [completionSource], defaultKeymap: false }));
    // The completion keymap MUST win over `defaultKeymap` while the popup is open, or
    // `defaultKeymap`'s Enter (insert newline) fires first and the accepted item is never
    // inserted. `Prec.highest` puts it above the base keymap regardless of push order; each
    // binding no-ops (returns false) when the popup is closed, so Enter/Tab fall through to
    // newline / indent normally. `Tab` is added to accept too (IntelliJ muscle memory).
    exts.push(
      Prec.highest(
        keymap.of([{ key: 'Tab', run: acceptCompletion }, ...completionKeymap]),
      ),
    );
    // Member-access trigger: CodeMirror's `activateOnTyping` only auto-opens the popup
    // on identifier characters, so a bare `receiver.` never queries the source. Fire
    // completion explicitly right after a `.` is typed (the source's dot branch returns
    // an empty-prefix result → the popup opens on the members).
    exts.push(EditorView.updateListener.of((u) => {
      if (!u.docChanged) return;
      let typedDot = false;
      u.changes.iterChanges((_fromA, _toA, _fromB, _toB, inserted) => {
        if (inserted.sliceString(0).endsWith('.')) typedDot = true;
      });
      if (typedDot) startCompletion(u.view);
    }));
  }

  // Ghost text — the greyed continuation at the caret, Tab to accept. Installed
  // AFTER the completion keymap above and at the same precedence, so within that
  // group `acceptCompletion` is tried first: while the popup is open it owns Tab,
  // and it returns false the rest of the time, letting the ghost text have it.
  // (The plugin also refuses to show anything while the popup is open, so the two
  // never actually contend — the ordering is belt and braces.)
  const inlineSource = lang.intel?.inlineCompletion;
  if (inlineSource && !opts.readOnly) {
    exts.push(inlineCompletion({ source: inlineSource }));
    exts.push(
      Prec.highest(
        keymap.of([
          { key: 'Tab', run: acceptInlineCompletion },
          { key: 'Escape', run: dismissInlineCompletion },
        ]),
      ),
    );
  }

  // Hover docs — a `hoverTooltip` source (e.g. a symbol signature via IPC). Installed
  // whenever the descriptor supplies one; read-only editors get it too (it's inert).
  const hoverSource = lang.intel?.hover;
  if (hoverSource) {
    // 350ms was long enough to read as lag once the source itself answers quickly — the
    // card should feel like a consequence of stopping the pointer, not a separate wait.
    exts.push(hoverTooltip(hoverSource, { hoverTime: 200 }));
  }

  // Host-owned keys. `Prec.highest`, so they beat `defaultKeymap` — which is the whole
  // point: the keys a host wants back (`Mod-Enter`, `Mod-Shift-Enter`) are ones CodeMirror
  // already binds to something plausible, so losing the race looks like a broken shortcut
  // rather than a stolen one. Registered AFTER the completion keymap above and at the same
  // precedence, so while the popup is open completion still owns Enter / Tab / Escape.
  if (opts.keyBindings?.length) exts.push(Prec.highest(keymap.of([...opts.keyBindings])));

  // Emmet Tab expansion (markup buffers). Pushed BEFORE the base keymap so its Tab binding is
  // tried first; on no abbreviation it returns false and Tab falls through to `indentWithTab`.
  if (opts.emmet) exts.push(emmetKeymap());

  exts.push(
    keymap.of([
      // IntelliJ: Ctrl+Y deletes the current line. First in the list so it wins over
      // the Windows redo binding (Mod-y) that historyKeymap also maps to Ctrl-y.
      { key: 'Ctrl-y', run: deleteLine, preventDefault: true },
      // The IDE verbs CodeMirror has no binding for. All three are keys an
      // IntelliJ-trained hand presses without looking, and finding nothing there is
      // what makes an editor feel like a text box.
      //
      // `Mod-d` duplicates: the most-used editing verb after copy and paste, and
      // the one `defaultKeymap` has no command for at all.
      { key: 'Mod-d', run: duplicateSelection, preventDefault: true },
      // `Alt-j` adds the next occurrence of the selection as a second cursor —
      // IntelliJ's own key for it. VS Code puts this on `Mod-d`; both cannot have
      // it, and duplicating is asked for far more often than multi-select.
      { key: 'Alt-j', run: selectNextOccurrence, preventDefault: true },
      // Moving a line: `Alt+↑/↓` comes from `defaultKeymap`, and these are the
      // IntelliJ spelling of the same thing. An alias, not a second feature.
      { key: 'Mod-Shift-ArrowUp', run: moveLineUp, preventDefault: true },
      { key: 'Mod-Shift-ArrowDown', run: moveLineDown, preventDefault: true },
      // Before the default keymap so Backspace deletes an empty auto-inserted pair.
      ...closeBracketsKeymap,
      ...defaultKeymap, ...historyKeymap, ...lintKeymap, ...foldKeymap,
      // (completion keymap is installed at Prec.highest above so it beats defaultKeymap's Enter)
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
        // The clicked position as a UTF-8 byte offset (what a BE classifier wants).
        const byteOffset = new TextEncoder().encode(view.state.doc.sliceString(0, pos)).length;
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
            onGoto(word, view, byteOffset);
            return true;
          }
        }
        // Otherwise, a reference-like token (a string-literal's contents or a
        // path such as `/do/Category/viewTree`) — so Ctrl/Cmd+Click on a JSP
        // action string resolves too. Generic, no product knowledge, no tree.
        const ref = refTextAt(view.state.doc, pos);
        if (ref) {
          event.preventDefault();
          onGoto(ref, view, byteOffset);
          return true;
        }
        return false;
      },
    }));
    // The Ctrl/Cmd-hover affordance: underline + pointer-cursor the token a click would
    // navigate, so the user sees WHERE go-to will land (IntelliJ / VS Code). Pushed as a
    // single nested Extension (CodeMirror flattens) rather than spread — an `Extension`
    // isn't statically iterable.
    exts.push(ctrlHoverLink());
  }

  return { extensions: exts, getTree };
}

// ── Ctrl/Cmd-hover link affordance ──────────────────────────────────────────────
//
// While Ctrl/Cmd is held, the reference-like token under the mouse is underlined and the
// cursor becomes a pointer (the mark's CSS sets `cursor: pointer`, and the mouse is over
// the marked span). Optimistic (any reference-like token, like VS Code) — the actual
// resolution still happens on click. No tree needed, so it works before the grammar loads.

const gotoLinkMark = Decoration.mark({ class: 'cm-goto-link' });
const setGotoLink = StateEffect.define<{ from: number; to: number } | null>();

const gotoLinkField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setGotoLink)) {
        deco = e.value ? Decoration.set([gotoLinkMark.range(e.value.from, e.value.to)]) : Decoration.none;
      }
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** The link range currently held in the field (to change-guard the mousemove dispatch). */
function currentGotoLink(view: EditorView): { from: number; to: number } | null {
  let found: { from: number; to: number } | null = null;
  view.state.field(gotoLinkField).between(0, view.state.doc.length, (from, to) => {
    found = { from, to };
    return false;
  });
  return found;
}

function ctrlHoverLink(): Extension {
  // A window keyup clears the link when Ctrl/Cmd is released without moving the mouse
  // (mousemove alone would leave a stale underline until the next move).
  const keyupPlugin = ViewPlugin.fromClass(
    class {
      onKeyUp: (e: KeyboardEvent) => void;
      constructor(readonly view: EditorView) {
        this.onKeyUp = (e) => {
          if ((e.key === 'Control' || e.key === 'Meta') && currentGotoLink(view)) {
            view.dispatch({ effects: setGotoLink.of(null) });
          }
        };
        window.addEventListener('keyup', this.onKeyUp);
      }
      destroy() {
        window.removeEventListener('keyup', this.onKeyUp);
      }
    },
  );

  const events = EditorView.domEventHandlers({
    mousemove(event, view) {
      const active = event.ctrlKey || event.metaKey;
      let range: { from: number; to: number } | null = null;
      if (active) {
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos != null) range = refRangeAt(view.state.doc, pos);
      }
      const cur = currentGotoLink(view);
      // Only dispatch when the highlighted token actually changes (not once per pixel).
      if ((range?.from ?? -1) === (cur?.from ?? -1) && (range?.to ?? -1) === (cur?.to ?? -1)) {
        return false;
      }
      view.dispatch({ effects: setGotoLink.of(range) });
      return false;
    },
    mouseleave(_event, view) {
      if (currentGotoLink(view)) view.dispatch({ effects: setGotoLink.of(null) });
      return false;
    },
  });

  return [gotoLinkField, keyupPlugin, events];
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

/** The document range of the reference-like token at UTF-16 `offset`, or null — the range
 *  form of {@link refTextAt}, used to underline the token under a Ctrl-hover. */
export function refRangeAt(doc: Text, offset: number): { from: number; to: number } | null {
  const clamped = Math.max(0, Math.min(offset, doc.length));
  const line = doc.lineAt(clamped);
  const text = line.text;
  const rel = clamped - line.from;

  const quoted = quotedRangeAround(text, rel);
  if (quoted) {
    return quoted.to > quoted.from ? { from: line.from + quoted.from, to: line.from + quoted.to } : null;
  }
  let start = rel;
  let end = rel;
  while (start > 0 && REF_CHAR.test(text[start - 1])) start--;
  while (end < text.length && REF_CHAR.test(text[end])) end++;
  return end > start ? { from: line.from + start, to: line.from + end } : null;
}

/** If position `rel` (a column into `text`) falls within a single-line quoted string,
 *  return the string's inner contents; else null. Scans quotes left-to-right so an
 *  even count before `rel` means "outside", odd means "inside". Handles both quote
 *  styles independently (the first-opened wins). */
function quotedStringAround(text: string, rel: number): string | null {
  const r = quotedRangeAround(text, rel);
  return r ? text.slice(r.from, r.to) : null;
}

/** The inner range (`[open+1, close)`) of the single-line quoted string covering column
 *  `rel`, or null. Shared by {@link quotedStringAround} (text) and {@link refRangeAt} (range). */
function quotedRangeAround(text: string, rel: number): { from: number; to: number } | null {
  for (const q of ['"', "'"]) {
    let i = 0;
    while (i < text.length) {
      const open = text.indexOf(q, i);
      if (open === -1) break;
      const close = text.indexOf(q, open + 1);
      if (close === -1) break;
      // Inside (or on either quote) of this pair?
      if (rel >= open && rel <= close) return { from: open + 1, to: close };
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
