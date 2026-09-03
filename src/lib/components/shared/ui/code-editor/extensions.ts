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
  EditorView, lineNumbers, keymap, hoverTooltip, highlightWhitespace,
  highlightActiveLine, highlightActiveLineGutter, drawSelection,
  ViewPlugin, Decoration, type DecorationSet, type KeyBinding, type PluginValue, type ViewUpdate,
} from '@codemirror/view';
import {
  Compartment, EditorState, StateField, StateEffect, Prec, type Extension, type Text,
} from '@codemirror/state';
import { history, defaultKeymap, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, indentOnInput, foldKeymap, foldGutter, codeFolding } from '@codemirror/language';
import { lintGutter, lintKeymap } from '@codemirror/lint';
import { search, searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import {
  autocompletion, completionKeymap, startCompletion, acceptCompletion,
  closeBrackets, closeBracketsKeymap, type CompletionSource,
} from '@codemirror/autocomplete';

import { documentHighlights, serverFolding } from './server-layers';
import { inlayHints } from './inlay-hints';
import { signatureHints } from './signature-hint';
import { codeLensLayer } from './code-lens';
import { snippetStops } from './snippet-stops';
import type { LanguageDescriptor, Tree, Node } from './types';
import { createHighlightPlugin } from './highlight';
import { createFoldingExtension, foldBlockCommentsOnLoad } from './folding';
import { emmetKeymap } from './emmet';
import { intellijEditingKeymap } from './intellij-keymap';
import { rainbowBrackets } from './rainbow-brackets';
import { indentGuides } from './indent-guides';
import { stickyScroll } from './sticky-scroll';
import { scrollbarOverview } from './scrollbar-overview';
import { semanticHighlight } from './semantic-tokens';
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

/**
 * The undo history, in a compartment so it can be **emptied**.
 *
 * CodeMirror has no "clear the history" command — reconfiguring the extension is how it is done,
 * and there is one thing that needs it: a controlled `value` swap puts a different file in the
 * same view. Undo would then replay the previous file's edits, at the positions they had in a
 * document that is no longer there. `historyReset()` is the effect that prevents it.
 */
export const historyCompartment = new Compartment();

/** The effect that empties the undo history — dispatch it with a whole-document replacement. */
export function historyReset() {
  return historyCompartment.reconfigure(history());
}

/**
 * The parts of the editing surface a **user setting** turns on and off.
 *
 * Separated from the rest of the options because they share one property none of the others
 * have: the answer can change while a file is open. They live in {@link preferencesCompartment}
 * and are rebuilt by the closure `createCodeEditorExtensions` hands back, so flipping "word
 * wrap" reconfigures the buffer in front of you instead of the next one you open.
 */
export interface CodeEditorPreferences {
  /**
   * Show the line-number gutter. `true` by default — a buffer is navigated by line.
   *
   * Turned off for a **short input** that happens to want an editor: a structural query is two
   * or three lines of code with holes in it, and it wants the highlighting and the completion
   * without the chrome. A gutter numbering three lines is a column of noise beside a field, and
   * "line 2" is not how anyone refers to a part of a query they can see all of.
   */
  lineNumbers?: boolean;
  /** Tint the line the caret is on (and its gutter number). `true` by default. */
  highlightActiveLine?: boolean;
  /** Render spaces and tabs as visible glyphs. Off by default — it is a mode you turn on to
   *  answer a question about indentation, not a way to read code. */
  showWhitespace?: boolean;
  /** Wrap long lines to the viewport instead of scrolling sideways. Off for a document (a
   *  source file has a column budget and the horizontal scrollbar is how you notice you blew
   *  it); on for the editors that are a *field* in a narrow box. */
  wrap?: boolean;
  /** Install folding at all — the gutter arrows and the fold commands. `true` by default;
   *  off leaves the gutter column back to the text. */
  folding?: boolean;
  /** Collapse the block comments of a file when it opens. Needs `folding`, and a language
   *  that both folds and declares its block-comment tokens. */
  foldBlockComments?: boolean;
  /** How the completion popup behaves. Ignored by an editor whose language brings no
   *  completion source, and by a read-only one. */
  completion?: CompletionPreferences;
}

/** The completion-popup half of {@link CodeEditorPreferences}. */
export interface CompletionPreferences {
  /** Open the popup on its own while an identifier is being typed. `true` by default; off
   *  leaves completion to the explicit chord. */
  autoPopup?: boolean;
  /** How long the typing must pause before the auto-popup opens, in milliseconds. */
  delayMs?: number;
  /** Require the candidate to start with the typed prefix, matching case. Off by default,
   *  which is CodeMirror's fuzzy, case-insensitive matching. */
  caseSensitive?: boolean;
}

/**
 * The compartment holding whatever {@link CodeEditorPreferences} currently say.
 *
 * One module-level compartment is enough for every editor: a `Compartment` is an identity used
 * as a key inside a state, not state itself, so two views can hold different contents under the
 * same key (`historyCompartment` above works the same way).
 */
export const preferencesCompartment = new Compartment();

export interface CodeEditorExtensionsOptions extends CodeEditorPreferences {
  readOnly?: boolean;
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
  /**
   * A code lens was pressed — `key` is the identifier the host issued with the lens.
   *
   * The layer is installed only when this is given, because a lens is a *control*: a host that
   * cannot answer a press should not be drawing something that invites one. The host owns what
   * pressing it means (show a list of implementations, run a test), which is why the core hands back
   * an opaque key rather than trying to interpret a command itself.
   */
  onLensPress?: (key: number) => void;
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

/** Build the full extension set for one editor bound to `lang`. Returns the extensions, the
 *  `getTree` reader (so a host component can implement go-to-decl / structure against the same
 *  live tree the highlighter maintains), and `preferences` — the builder for
 *  {@link preferencesCompartment}, so the host can reconfigure a live buffer when a setting
 *  changes instead of waiting for the next mount. */
/** The fold gutter's arrow. Shared by the Lezer and the provider-driven folding, so the two look the
 *  same — a fold is a fold, whoever found it. */
function foldMarkerDOM(open: boolean): HTMLElement {
  const el = document.createElement('span');
  el.className = open ? 'cm-foldMarker cm-foldMarker-open' : 'cm-foldMarker cm-foldMarker-closed';
  el.textContent = open ? '▾' : '▸';
  return el;
}

/**
 * The completion popup, configured by the user's {@link CompletionPreferences}.
 *
 * `autoPopup` and its delay map straight onto CodeMirror's own options. **Case sensitivity does
 * not**: the library ranks a case match above a case-insensitive one but still offers both, and
 * there is no option to stop it. So it is enforced where the candidates are: the source is
 * wrapped, and when the setting is on, an option whose label does not start with the typed
 * prefix — same letters, same case — is dropped before the list is ever scored.
 */
function completionExtension(
  source: CompletionSource,
  prefs: CompletionPreferences | undefined,
): Extension {
  const autoPopup = prefs?.autoPopup !== false;
  return [
    autocompletion({
      override: [prefs?.caseSensitive ? caseSensitiveSource(source) : source],
      defaultKeymap: false,
      activateOnTyping: autoPopup,
      ...(prefs?.delayMs !== undefined ? { activateOnTypingDelay: Math.max(0, prefs.delayMs) } : {}),
    }),
    // Member-access trigger: CodeMirror's `activateOnTyping` only auto-opens the popup on
    // identifier characters, so a bare `receiver.` never queries the source. Fire completion
    // explicitly right after a `.` is typed (the source's dot branch returns an empty-prefix
    // result → the popup opens on the members). Gated on the same preference: an editor told
    // not to open the popup by itself must not open it on a dot either.
    autoPopup
      ? EditorView.updateListener.of((u) => {
          if (!u.docChanged) return;
          let typedDot = false;
          u.changes.iterChanges((_fromA, _toA, _fromB, _toB, inserted) => {
            if (inserted.sliceString(0).endsWith('.')) typedDot = true;
          });
          if (typedDot) startCompletion(u.view);
        })
      : [],
  ];
}

/** Wrap a completion source so only candidates matching the typed prefix's CASE survive. */
function caseSensitiveSource(source: CompletionSource): CompletionSource {
  return async (context) => {
    const result = await source(context);
    if (!result) return result;
    const typed = context.state.sliceDoc(result.from, context.pos);
    if (!typed) return result;
    const options = result.options.filter((o) => o.label.startsWith(typed));
    return { ...result, options };
  };
}

export function createCodeEditorExtensions(
  lang: LanguageDescriptor,
  opts: CodeEditorExtensionsOptions = {},
): {
  extensions: Extension;
  getTree: ReturnType<typeof createHighlightPlugin>['getTree'];
  preferences: (prefs: CodeEditorPreferences) => Extension;
} {
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
  //
  // Collected rather than pushed: folding is one of the preferences (see
  // `CodeEditorPreferences.folding`), so it lives in the compartment with them and the whole
  // set — gutter, service, fold state — comes and goes together when the setting flips.
  const foldingExts: Extension[] = [createFoldingExtension(lang, getTree)];

  // Lezer folding for a `cmExtension` language that opts in (`cmFold`) — drives the
  // fold gutter from the language's own `foldNodeProp` (e.g. `lang-html` folds tag
  // bodies, `lang-json` folds objects). Tree-sitter descriptors fold via `foldNode`
  // above; legacy StreamLanguage modes stay gutter-free (they carry no fold info).
  if (useCm && lang.cmFold) {
    foldingExts.push(codeFolding(), foldGutter({ markerDOM: foldMarkerDOM }));
  }

  // Folding from a PROVIDER's ranges, for a language whose descriptor says its folds come from
  // outside the buffer. A legacy stream mode carries no fold information at all — which is why a
  // `.rs` file had no fold gutter — and brace matching would find the function bodies and nothing
  // else, where a server folds by item. The ranges are pushed by the host; this installs the
  // machinery that uses them.
  if (lang.serverFold) {
    foldingExts.push(codeFolding(), foldGutter({ markerDOM: foldMarkerDOM }), serverFolding());
  }

  // The completion source a language brings, if it brings one. Read here because the popup's
  // BEHAVIOUR is a preference (auto-popup, delay, case) while its CONTENT is the language's.
  const completionSource = lang.intel?.completion;

  // Each toggleable piece is built ONCE and composed by value below. Reconfiguring a
  // compartment with the same extension value keeps the state field / view plugin behind it
  // alive, so flipping word wrap doesn't re-run the one-shot comment fold or rebuild the gutter.
  const lineNumberGutter = lineNumbers();
  const activeLineHighlight = [highlightActiveLine(), highlightActiveLineGutter()];
  const whitespaceGlyphs = highlightWhitespace();
  const commentFoldOnLoad = foldBlockCommentsOnLoad(lang, getTree);

  /**
   * Build the contents of {@link preferencesCompartment} for one set of preferences.
   *
   * Everything a setting can turn on or off is assembled here and nowhere else, so a live
   * reconfigure and a fresh mount cannot end up with different surfaces.
   */
  const buildPreferences = (prefs: CodeEditorPreferences): Extension => [
    // Absent, not empty: an unwanted gutter still costs its horizontal column.
    prefs.lineNumbers === false ? [] : lineNumberGutter,
    prefs.highlightActiveLine === false ? [] : activeLineHighlight,
    prefs.showWhitespace ? whitespaceGlyphs : [],
    prefs.wrap ? EditorView.lineWrapping : [],
    prefs.folding === false ? [] : foldingExts,
    // Folding the comments needs the folding: `foldEffect` on a state with no fold field is
    // dropped silently, which would look like a setting that works some days.
    prefs.folding !== false && prefs.foldBlockComments ? commentFoldOnLoad : [],
    completionSource && !opts.readOnly
      ? completionExtension(completionSource, prefs.completion)
      : [],
  ];

  const exts: Extension[] = [
    codeEditorTheme,
    codeEditorHighlightStyle,
    // The settings-driven half of the surface (line numbers, active line, whitespace glyphs,
    // wrap, folding, completion behaviour). Here rather than anywhere else because it holds the
    // line-number gutter, and gutter order follows extension order — earlier is further left,
    // which is what keeps the host's breakpoint column outside the numbers.
    preferencesCompartment.of(buildPreferences(opts)),
    // In a compartment so the host can RESET it. A controlled `value` swap replaces the whole
    // document without remounting the view, and the edits of the file that was there a moment
    // ago are still undoable — expressed as positions in a text that is gone. See
    // `historyCompartment`.
    historyCompartment.of(history()),
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
    highlightSelectionMatches(),
    search({ top: true }),
    useCm ? (lang.cmExtension as Extension) : highlight,
    // Text the grammar never handed to `classify` — see `LanguageDescriptor.extraHighlight`.
    // Absent for every language whose grammar covers its own document.
    ...(lang.extraHighlight ? [lang.extraHighlight] : []),
    // Semantic highlighting, LAYERED over whichever highlighter ran above. Installed
    // unconditionally: with no tokens pushed it is one state field holding an empty decoration
    // set, and a language with no server never pushes any. Layering (rather than replacing) is
    // what keeps a file coloured instantly on open and coloured while it is edited — the
    // server's refinement arrives a round-trip later and lands on top.
    semanticHighlight(),
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

  // Occurrence highlighting — where else the symbol under the caret appears. Installed whenever the
  // descriptor is provider-backed, because that is exactly when there is something to push: it costs
  // one state field holding an empty decoration set until the host pushes anything.
  if (lang.intel) exts.push(documentHighlights());

  // Inlay hints and the parameter strip, on the same terms and for the same reason: both are
  // things only a provider can know, both cost one idle state field until something is pushed, and
  // both are wanted by every provider-backed language rather than by any particular one. Where the
  // content comes from — Bennu's own resolver for Java, a language server for the rest — is the
  // host's business and invisible from here.
  if (lang.intel) exts.push(inlayHints(), signatureHints());

  // Code lenses — the counts a provider draws above an item. Gated on the HOST's press handler
  // rather than on the descriptor: a lens is a control, and only the host knows what pressing one
  // means.
  if (opts.onLensPress) exts.push(codeLensLayer(opts.onLensPress));

  // Optional vertical margin guide (host opt-in via `rulerColumn`).
  if (opts.rulerColumn && opts.rulerColumn > 0) exts.push(editorRuler(opts.rulerColumn));

  // IntelliJ-flavoured chrome, each host opt-in (Bennu enables them; other products keep their
  // current look until they opt in too).
  if (opts.indentGuides) exts.push(indentGuides());
  if (opts.stickyScroll) exts.push(stickyScroll());
  if (opts.scrollbarOverview) exts.push(scrollbarOverview());

  // Language intelligence (autocomplete) — only when the descriptor supplies a completion source
  // and the editor is editable. The extension itself is a PREFERENCE (auto-popup, delay, case —
  // see `completionExtension`) and lives in the compartment; only its keymap is installed here,
  // before the base keymap, so Enter / ↑↓ / Esc while the popup is open win.
  if (completionSource && !opts.readOnly) {
    // The completion keymap MUST win over `defaultKeymap` while the popup is open, or
    // `defaultKeymap`'s Enter (insert newline) fires first and the accepted item is never
    // inserted. `Prec.highest` puts it above the base keymap regardless of push order; each
    // binding no-ops (returns false) when the popup is closed, so Enter/Tab fall through to
    // newline / indent normally. `Tab` is added to accept too (IntelliJ muscle memory).
    //
    // Three chords ask for completions, and the third exists because the first two are unreachable
    // on one platform.
    //
    // `Ctrl+Space` is IntelliJ's basic completion and arrives with `completionKeymap` (whose entry
    // is a literal `Ctrl-Space`, not `Mod-Space`); `Ctrl+Shift+Space` is its sibling. **On macOS
    // neither produces a keydown at all** — the whole Control+Space family is taken by the system
    // for switching input source, and it is taken above the application: a capture-phase listener
    // on `window` never sees the event, which is how this was finally established rather than
    // guessed. No binding, in any layer, can answer a key the process is not given.
    //
    // So on a Mac the chord is `Cmd+Shift+Space`, which does arrive. Not `Cmd+Space` — Spotlight —
    // and the Shift is what keeps the two apart.
    exts.push(
      Prec.highest(
        keymap.of([
          { key: 'Tab', run: acceptCompletion },
          { key: 'Ctrl-Shift-Space', run: startCompletion },
          { mac: 'Cmd-Shift-Space', run: startCompletion },
          ...completionKeymap,
        ]),
      ),
    );
    // Tab stops of an accepted snippet. AFTER the completion keymap and at the same precedence, so
    // within that group `acceptCompletion` is tried first: while the popup is open Tab accepts, and
    // only once it has closed does Tab walk the stops. Every binding returns false with no run
    // active, so Tab keeps its ordinary meaning the rest of the time.
    exts.push(snippetStops());
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
      // The IntelliJ keys, shared with merula's editor (see `./intellij-keymap`). First in
      // the list because two of them are keys `defaultKeymap` / `historyKeymap` already
      // bind — delete-line has to beat redo on Windows and delete-to-line-start on a Mac.
      ...intellijEditingKeymap(),
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

  return { extensions: exts, getTree, preferences: buildPreferences };
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
  // The range decides; this only reads it. They must agree — the underline shown under the
  // pointer is a promise about what a click will act on, and two implementations of "the
  // token here" is how that promise gets broken.
  const range = refRangeAt(doc, offset);
  return range ? doc.sliceString(range.from, range.to) : null;
}

/** The document range of the reference-like token at UTF-16 `offset`, or null — the range
 *  form of {@link refTextAt}, used to underline the token under a Ctrl-hover. */
export function refRangeAt(doc: Text, offset: number): { from: number; to: number } | null {
  const clamped = Math.max(0, Math.min(offset, doc.length));
  const line = doc.lineAt(clamped);
  const text = line.text;
  const rel = clamped - line.from;

  // Inside an interpolation, one dotted chain is SEVERAL references. `a.b.c` is three names,
  // each declared somewhere else, and treating the chain as one token both underlines the
  // whole thing — telling the user nothing about where a click would land — and hands the
  // resolver a string that names nothing.
  //
  // Only inside `${…}` / `%{…}` / `#{…}`, because outside one a dotted run is usually a single
  // name: `com.acme.OrderDao` in a `class="…"`, `struts-default.xml`, `view.action`.
  if (inInterpolation(text, rel)) {
    const seg = dotSegmentAround(text, rel);
    return seg && seg.to > seg.from ? { from: line.from + seg.from, to: line.from + seg.to } : null;
  }

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

/** Is column `rel` inside a `${…}` / `%{…}` / `#{…}` on this line?
 *
 *  An unterminated one (the closing brace is on a later line, or is still being typed) counts
 *  from its opener to the end of the line — a half-written expression is still an expression. */
function inInterpolation(text: string, rel: number): boolean {
  const open = Math.max(
    text.lastIndexOf('${', rel),
    text.lastIndexOf('%{', rel),
    text.lastIndexOf('#{', rel),
  );
  if (open < 0) return false;
  const close = text.indexOf('}', open);
  return close < 0 || rel <= close;
}

/** The identifier segment around column `rel` — the run of name characters bounded by the
 *  dots, brackets and operators on either side. `null` when there is none. */
function dotSegmentAround(text: string, rel: number): { from: number; to: number } | null {
  const NAME = /[A-Za-z0-9_$]/;
  let from = Math.min(rel, text.length);
  let to = from;
  while (from > 0 && NAME.test(text[from - 1])) from--;
  while (to < text.length && NAME.test(text[to])) to++;
  return to > from ? { from, to } : null;
}

/** The inner range (`[open+1, close)`) of the single-line quoted string covering column
 *  `rel`, or null. Scans quotes left-to-right so an even count before `rel` means "outside",
 *  odd means "inside". Handles both quote styles independently (the first-opened wins). */
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
