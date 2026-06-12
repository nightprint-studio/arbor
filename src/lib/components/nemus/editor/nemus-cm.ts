/**
 * nemus ↔ CodeMirror 6 glue. Turns the Tree-sitter bridge (`nemus-lang.ts`)
 * into a set of editor extensions:
 *
 *   - **Syntax highlight** — a ViewPlugin that owns the parser + live tree,
 *     re-parses incrementally on every edit, and walks the tree into per-token
 *     decorations (replacing the Step-0 regex tokenizer).
 *   - **Active-hap highlight** — a StateField fed live from `activeHapsStore`;
 *     each sounding source range is tinted with its track colour.
 *   - **Diagnostics** — a helper that maps the backend's UTF-8 byte spans onto
 *     CodeMirror's UTF-16 offsets, for the lint gutter + underlines.
 *   - **Go-to-declaration** — a Ctrl/Cmd+Click handler that reads the word under
 *     the cursor from the tree (`nemus-lang`) and hands it to the host.
 *
 * Theme: every colour is an Arbor CSS variable, so a theme overlay re-skins the
 * editor for free — and the pane sits on `--bg-base` like the Step-0 editor.
 */

import { EditorView, ViewPlugin, Decoration, lineNumbers } from '@codemirror/view';
import type { DecorationSet, ViewUpdate } from '@codemirror/view';
import {
  EditorState, StateField, StateEffect, RangeSetBuilder, type Extension,
} from '@codemirror/state';
import {
  highlightActiveLine, highlightActiveLineGutter, drawSelection, keymap,
} from '@codemirror/view';
import { history, defaultKeymap, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, indentOnInput } from '@codemirror/language';
import { lintGutter, lintKeymap, type Diagnostic as CmDiagnostic } from '@codemirror/lint';
import { search, searchKeymap, highlightSelectionMatches } from '@codemirror/search';

import { laneColor } from '../palette';
import type { NemusDiagnostic } from '$lib/ipc/nemus';
import {
  classifyToken, makeByteToU16, identifierAt, createNemusParser,
  type NemusTokenClass, type Tree, type Node, type Parser,
} from './nemus-lang';
import { nemusLanguageIntel, type NemusIntelSource } from './nemus-intel';
import { nemusEditingExtensions } from './nemus-ergonomics';

// ── Syntax-highlight ViewPlugin ────────────────────────────────────────────────

/** Fired once the (async) grammar load completes, to kick the first full parse. */
export const nemusParserReady = StateEffect.define<void>();

/** One cached mark decoration per token class (`cm-grv-<class>`). */
const TOKEN_MARKS = new Map<NemusTokenClass, Decoration>();
function tokenMark(cls: NemusTokenClass): Decoration {
  let m = TOKEN_MARKS.get(cls);
  if (!m) { m = Decoration.mark({ class: `cm-grv-${cls}` }); TOKEN_MARKS.set(cls, m); }
  return m;
}

function point(doc: EditorState['doc'], pos: number) {
  const line = doc.lineAt(pos);
  return { row: line.number - 1, column: pos - line.from };
}

/**
 * Owns the parser + the live syntax tree for one editor. Re-parses incrementally
 * (Tree-sitter `edit` + reparse with the previous tree) so typing stays cheap,
 * and rebuilds decorations from a single tree walk. The parser loads
 * asynchronously; until then the document renders plain (no crash, no flash).
 */
class NemusHighlighter {
  decorations: DecorationSet = Decoration.none;
  tree: Tree | null = null;
  private parser: Parser | null = null;
  private destroyed = false;

  constructor(view: EditorView) {
    // Async grammar load; once ready, do the first full parse via a self-effect.
    createNemusParser()
      .then((parser) => {
        if (this.destroyed) return; // editor torn down before the wasm loaded
        this.parser = parser;
        view.dispatch({ effects: nemusParserReady.of() });
      })
      .catch(() => { /* grammar wasm missing — stays plain text */ });
  }

  destroy() { this.destroyed = true; }

  update(u: ViewUpdate) {
    if (!this.parser) return;
    const forced = u.transactions.some((tr) =>
      tr.effects.some((e) => e.is(nemusParserReady)));

    if (u.docChanged) {
      if (this.tree) {
        u.changes.iterChanges((fromA, toA, fromB, toB) => {
          this.tree!.edit({
            startIndex: fromA, oldEndIndex: toA, newEndIndex: toB,
            startPosition: point(u.startState.doc, fromA),
            oldEndPosition: point(u.startState.doc, toA),
            newEndPosition: point(u.state.doc, toB),
          });
        });
      }
      this.reparse(u.state, true);
    } else if (forced || (!this.tree && this.parser)) {
      this.reparse(u.state, false);
    }
  }

  private reparse(state: EditorState, incremental: boolean) {
    const text = state.doc.toString();
    try {
      const old = incremental ? (this.tree ?? undefined) : undefined;
      const next = this.parser!.parse(text, old);
      if (next) this.tree = next;
    } catch {
      // Incremental bookkeeping went inconsistent — recover with a fresh parse.
      const next = this.parser!.parse(text);
      if (next) this.tree = next;
    }
    this.decorations = this.buildDecorations();
  }

  private buildDecorations(): DecorationSet {
    if (!this.tree) return Decoration.none;
    const builder = new RangeSetBuilder<Decoration>();
    const visit = (node: Node, parentType: string | null, field: string | null) => {
      if (node.childCount === 0) {
        const cls = classifyToken(node.type, node.isNamed, field, parentType);
        if (cls && node.endIndex > node.startIndex) {
          builder.add(node.startIndex, node.endIndex, tokenMark(cls));
        }
        return;
      }
      for (let i = 0; i < node.childCount; i++) {
        const child = node.child(i);
        if (child) visit(child, node.type, node.fieldNameForChild(i));
      }
    };
    visit(this.tree.rootNode, null, null);
    return builder.finish();
  }
}

const nemusHighlight = ViewPlugin.fromClass(NemusHighlighter, {
  decorations: (v) => v.decorations,
});

/** Read the live syntax tree of an editor (null until the grammar has loaded
 *  and the first parse has run). Used by go-to-decl + the outline derivation. */
export function getNemusTree(view: EditorView): Tree | null {
  return view.plugin(nemusHighlight)?.tree ?? null;
}

// ── Active-hap decorations (live, per-track colour) ────────────────────────────

/** One sounding source range in CodeMirror coordinates (UTF-16, doc-clamped). */
export interface ActiveHapMark { from: number; to: number; track: number; }

/** Push the current active-hap set (already converted + clamped) to the editor. */
export const setActiveHaps = StateEffect.define<ActiveHapMark[]>();

const activeHapsField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (!e.is(setActiveHaps)) continue;
      const builder = new RangeSetBuilder<Decoration>();
      const sorted = [...e.value].sort((a, b) => a.from - b.from || a.to - b.to);
      for (const h of sorted) {
        if (h.to <= h.from) continue;
        builder.add(h.from, h.to, Decoration.mark({
          class: 'cm-grv-hap',
          attributes: { style: `--grv-hap: ${laneColor(h.track)}` },
        }));
      }
      deco = builder.finish();
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** Convert backend active-haps (UTF-8 byte spans) into doc-clamped CM marks for
 *  `src`. Spans past the current document (a different / edited file is showing)
 *  are dropped — the highlight only paints ranges that exist in this buffer. */
export function toActiveHapMarks(
  haps: { start: number; end: number; track: number }[],
  src: string,
): ActiveHapMark[] {
  const b2u = makeByteToU16(src);
  const len = src.length;
  const out: ActiveHapMark[] = [];
  for (const h of haps) {
    const from = b2u(h.start);
    const to = b2u(h.end);
    if (to > from && to <= len) out.push({ from, to, track: h.track });
  }
  return out;
}

// ── Diagnostics (byte spans → CM lint) ─────────────────────────────────────────

/** Map the diagnostics store (UTF-8 byte offsets) onto CodeMirror `Diagnostic`s
 *  for `src`. A null span (whole-file error) lands at the document start. */
export function toCmDiagnostics(errors: NemusDiagnostic[], src: string): CmDiagnostic[] {
  const b2u = makeByteToU16(src);
  const len = src.length;
  const out: CmDiagnostic[] = [];
  for (const e of errors) {
    let from = e.start != null ? b2u(e.start) : 0;
    let to = e.end != null ? b2u(e.end) : from;
    from = Math.max(0, Math.min(from, len));
    to = Math.max(from, Math.min(to, len));
    if (to === from) to = Math.min(len, from + 1); // give the marker some width
    out.push({ from, to, severity: e.severity, message: e.message, source: 'nemus' });
  }
  return out;
}

// ── Theme ──────────────────────────────────────────────────────────────────────

export const nemusTheme = EditorView.theme(
  {
    '&': {
      height: '100%',
      backgroundColor: 'var(--bg-base)',
      color: 'var(--text-primary)',
      fontFamily: 'var(--font-code)',
      fontSize: '12.5px',
    },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': { fontFamily: 'var(--font-code)', lineHeight: '1.55', overflow: 'auto' },
    '.cm-content': { padding: '6px 0', caretColor: 'var(--text-primary)' },
    '.cm-line': { padding: '0 12px' },
    '.cm-gutters': {
      backgroundColor: 'var(--bg-base)',
      color: 'var(--text-disabled)',
      border: 'none',
      fontFamily: 'var(--font-code)',
    },
    '.cm-lineNumbers .cm-gutterElement': { padding: '0 8px 0 14px', minWidth: '34px' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--text-secondary)' },
    // Fold gutter (collapse arrows) + the inline placeholder for a folded block.
    '.cm-foldGutter .cm-gutterElement': { padding: '0 2px', color: 'var(--text-disabled)', cursor: 'pointer' },
    '.cm-foldGutter .cm-gutterElement:hover': { color: 'var(--text-primary)' },
    '.cm-foldPlaceholder': {
      backgroundColor: 'var(--bg-hover)', color: 'var(--text-muted)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-sm)',
      margin: '0 2px', padding: '0 4px',
    },
    '.cm-activeLine': { backgroundColor: 'color-mix(in srgb, var(--bg-hover) 45%, transparent)' },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--text-primary)' },
    '.cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: 'var(--accent-subtle) !important',
    },
    '&.cm-focused .cm-selectionBackground': { backgroundColor: 'var(--accent-subtle) !important' },
    '.cm-matchingBracket': {
      outline: '1px solid var(--accent-strong, var(--accent))', borderRadius: '2px',
    },
    '.cm-tooltip': {
      backgroundColor: 'var(--bg-elevated)', color: 'var(--text-primary)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-md)',
    },
    '.cm-tooltip.cm-tooltip-lint': { padding: '2px 6px' },

    // ── Search panel (Ctrl+F) — themed to match Arbor's inputs/buttons ──
    '.cm-panels': { backgroundColor: 'var(--bg-elevated)', color: 'var(--text-primary)' },
    '.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--border-subtle)' },
    '.cm-panel.cm-search': {
      padding: '6px 8px', fontFamily: 'var(--font-ui-sans)', fontSize: '12px',
      display: 'flex', alignItems: 'center', flexWrap: 'wrap', gap: '6px',
    },
    '.cm-panel.cm-search label': { display: 'inline-flex', alignItems: 'center', gap: '3px', fontSize: '11px', color: 'var(--text-muted)' },
    '.cm-panel.cm-search input, .cm-panel.cm-search input[type=text]': {
      backgroundColor: 'var(--bg-input)', color: 'var(--text-primary)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-sm)',
      padding: '3px 6px', fontFamily: 'var(--font-code)', fontSize: '12px', outline: 'none',
    },
    '.cm-panel.cm-search input:focus': { borderColor: 'var(--border-focus, var(--accent))' },
    '.cm-panel.cm-search button, .cm-panel.cm-search .cm-button': {
      backgroundColor: 'var(--bg-input)', color: 'var(--text-secondary)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-sm)',
      backgroundImage: 'none', padding: '3px 8px', cursor: 'pointer', fontSize: '11px',
    },
    '.cm-panel.cm-search button:hover, .cm-panel.cm-search .cm-button:hover': {
      backgroundColor: 'var(--bg-hover)', color: 'var(--text-primary)',
    },
    '.cm-panel.cm-search .cm-button:active': { backgroundImage: 'none' },
    '.cm-panel.cm-search [name=close]': { color: 'var(--text-muted)', fontSize: '16px' },
    '.cm-panel.cm-search [name=close]:hover': { color: 'var(--text-primary)' },
    '.cm-searchMatch': { backgroundColor: 'color-mix(in srgb, var(--warning) 28%, transparent)', borderRadius: '2px' },
    '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'color-mix(in srgb, var(--accent) 45%, transparent)' },
    '.cm-selectionMatch': { backgroundColor: 'color-mix(in srgb, var(--accent) 18%, transparent)' },

    // ── Token palette ──
    //
    // Deliberate, hierarchical colouring rather than one-accent-per-token:
    //   • Music content (notes / chords / sounds) is the bright, eye-catching
    //     layer — it's what you read while composing.
    //   • Structure (island heads, combinators-via-fn, splice) uses the accent /
    //     teal family so the "shape" of a phrase stands out from its content.
    //   • Host scaffolding (keywords, defs, numbers, strings, comments) uses the
    //     standard Arbor syntax vars, quieter than the music.
    //   • Punctuation / mini-notation operators stay muted so the rhythm reads
    //     without visual noise.

    // Host scaffolding (quiet, conventional — same `--syntax-*` vars + fallbacks
    // the rest of Arbor's highlighting uses, so a theme overlay re-skins it too).
    '.cm-grv-comment': { color: 'var(--syntax-comment, #7a7d85)', fontStyle: 'italic' },
    '.cm-grv-string': { color: 'var(--syntax-string, #6a9956)' },
    '.cm-grv-number': { color: 'var(--syntax-number, #9876aa)' },
    '.cm-grv-keyword': { color: 'var(--syntax-keyword, #cc7832)', fontWeight: '600' },
    '.cm-grv-def': { color: 'var(--syntax-type, #4d78cc)' },
    '.cm-grv-ident': { color: 'var(--text-primary)' },

    // Structure: function/method calls (combinators, transforms) + island heads.
    '.cm-grv-fn': { color: 'var(--syntax-function, #ffc66d)' },
    '.cm-grv-island': { color: 'var(--accent, #56b6c2)', fontWeight: '700' },
    '.cm-grv-splice': { color: 'var(--accent, #56b6c2)', fontWeight: '600' },

    // Music content (the bright layer): notes warm, chords italic, sounds cool
    // teal. Nemus-local vars (default to the music palette) keep them distinct
    // from host scaffolding while still theme-overridable.
    '.cm-grv-note': { color: 'var(--grv-syntax-note, #e5c07b)' },
    '.cm-grv-chord': { color: 'var(--grv-syntax-note, #e5c07b)', fontStyle: 'italic', fontWeight: '600' },
    '.cm-grv-sound': { color: 'var(--grv-syntax-sound, #56b6c2)' },

    // Mini-notation operators (~ _ * / ! @ …) — muted, so rhythm reads clean.
    '.cm-grv-mininote': { color: 'var(--text-muted)' },
    '.cm-grv-op': { color: 'var(--text-secondary)' },

    // ── Autocomplete + hover docs ──
    '.cm-tooltip-autocomplete': {
      backgroundColor: 'var(--bg-elevated)',
      border: '1px solid var(--border-subtle)', borderRadius: 'var(--radius-md)',
    },
    '.cm-tooltip-autocomplete > ul > li': {
      fontFamily: 'var(--font-code)', fontSize: '12px',
      padding: '2px 6px', color: 'var(--text-primary)',
    },
    '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
      backgroundColor: 'var(--accent-subtle)', color: 'var(--text-primary)',
    },
    '.cm-completionLabel': { color: 'var(--text-primary)' },
    '.cm-completionDetail': { color: 'var(--text-muted)', fontStyle: 'normal', marginLeft: '0.6em' },
    '.cm-completionMatchedText': { color: 'var(--accent)', textDecoration: 'none', fontWeight: '700' },

    // Shared docs block (autocomplete info side-panel + hover tooltip).
    '.cm-grv-hover': { maxWidth: '420px' },
    '.cm-grv-doc': { padding: '4px 2px', maxWidth: '420px' },
    '.cm-grv-doc-sig': {
      fontFamily: 'var(--font-code)', fontSize: '12px', fontWeight: '600',
      color: 'var(--accent)', marginBottom: '4px', whiteSpace: 'pre-wrap',
    },
    '.cm-grv-doc-summary': {
      fontSize: '11.5px', lineHeight: '1.5', color: 'var(--text-secondary)',
    },
    '.cm-grv-doc-params': { margin: '6px 0 0', display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '1px 8px' },
    '.cm-grv-doc-params dt': {
      fontFamily: 'var(--font-code)', fontSize: '11px', fontWeight: '600',
      color: 'var(--grv-syntax-note, #e5c07b)', margin: '0',
    },
    '.cm-grv-doc-params dd': { margin: '0', fontSize: '11px', color: 'var(--text-secondary)', lineHeight: '1.45' },
    '.cm-grv-doc-example': {
      margin: '6px 0 0', padding: '5px 7px',
      background: 'var(--bg-input)', border: '1px solid var(--border-subtle)',
      borderRadius: 'var(--radius-sm, 4px)',
      fontFamily: 'var(--font-code)', fontSize: '11px', color: 'var(--text-primary)',
      whiteSpace: 'pre-wrap',
    },

    // Active-hap underline — tinted with the per-track colour via --grv-hap.
    '.cm-grv-hap': {
      backgroundColor: 'color-mix(in srgb, var(--grv-hap) 22%, transparent)',
      boxShadow: 'inset 0 -2px 0 0 var(--grv-hap)',
      borderRadius: '2px',
    },
  },
  { dark: true },
);

// ── Extensions factory ─────────────────────────────────────────────────────────

export interface NemusExtensionsOptions {
  readOnly?: boolean;
  /** Ctrl/Cmd+Click on an identifier — host resolves + jumps (go-to-decl). */
  onGoto?: (word: string, view: EditorView) => void;
  /** The DSL catalogue source for autocomplete + hover docs. When omitted, the
   *  editor still works (highlight, lint, go-to-decl) without language hints. */
  intel?: NemusIntelSource;
}

/** The search keymap minus its open binding: the NemusShell owns `Ctrl+F` so it
 *  can route it to the editor (when the pane is focused) or the Console search
 *  (otherwise). The in-panel navigation (next / previous / replace / close) stays
 *  so the search panel is fully keyboard-driven once opened. */
const nemusSearchKeymap = searchKeymap.filter((b) => b.key !== 'Mod-f');

/**
 * The full nemus editor extension set. CodeMirror's search panel IS wired (so
 * `Ctrl+F` searches the buffer when the editor is focused), but its *open*
 * binding is removed from the keymap — the NemusShell decides whether `Ctrl+F`
 * opens the editor search (pane focused) or the Console search (otherwise) and
 * calls `openSearch()` imperatively, keeping one authoritative router.
 */
export function createNemusExtensions(opts: NemusExtensionsOptions = {}): Extension {
  const exts: Extension[] = [
    nemusTheme,
    lineNumbers(),
    history(),
    drawSelection(),
    indentOnInput(),
    bracketMatching(),
    highlightActiveLine(),
    highlightActiveLineGutter(),
    highlightSelectionMatches(),
    search({ top: true }),
    nemusHighlight,
    activeHapsField,
    lintGutter(),
    // Editing ergonomics (comments, autoclose, delete-line, soft wrap, folding).
    // Placed before the base keymap so its `Mod-/` / `Mod-y` win over the
    // history defaults (`Mod-y` would otherwise redo).
    nemusEditingExtensions(),
  ];
  // Language intelligence (autocomplete + hover) — only when a catalogue source
  // is provided and the pane is editable (no completions in a read-only viewer).
  // Added BEFORE the base keymap so its completion keymap (Enter / ↑↓ / Esc while
  // the popup is open) takes precedence over the editor defaults.
  if (opts.intel && !opts.readOnly) {
    exts.push(nemusLanguageIntel(opts.intel));
  }
  exts.push(
    keymap.of([...defaultKeymap, ...historyKeymap, ...lintKeymap, ...nemusSearchKeymap, indentWithTab]),
    EditorState.readOnly.of(!!opts.readOnly),
  );
  if (opts.onGoto) {
    const onGoto = opts.onGoto;
    exts.push(EditorView.domEventHandlers({
      mousedown(event, view) {
        if (!(event.ctrlKey || event.metaKey)) return false;
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos == null) return false;
        const tree = getNemusTree(view);
        if (!tree) return false;
        const word = identifierAt(tree, pos);
        if (!word) return false;
        event.preventDefault();
        onGoto(word, view);
        return true;
      },
    }));
  }
  return exts;
}
