/**
 * Generic, app-agnostic CodeMirror 6 editor core — the language seam.
 *
 * This module owns NO Arbor-domain knowledge (no IPC, no stores, no product
 * types): it is parametrised by a {@link LanguageDescriptor} so any product /
 * language reuses the same CodeMirror glue (highlight, theme, extensions, host
 * component). It sits legitimately in `shared/ui/` because it only depends on
 * `@codemirror/*`, `web-tree-sitter`, and its own files.
 *
 * Offset model (mirrors merula-lang.ts): web-tree-sitter reports
 * `startIndex`/`endIndex` in **UTF-16 code units** (it parses the JS string in
 * UTF-16), which is exactly CodeMirror's document coordinate — so tree offsets
 * drop straight into CM with no mapping. Backends, by contrast, usually report
 * diagnostic spans in **UTF-8 byte offsets**; {@link EditorDiagnostic} carries
 * byte offsets and the host maps them via the byte→UTF-16 converter.
 */

import type { Parser, Tree, Node } from 'web-tree-sitter';
import type { CompletionSource } from '@codemirror/autocomplete';
import type { Extension } from '@codemirror/state';
import type { StreamParser } from '@codemirror/language';
import type { EditorView, Tooltip } from '@codemirror/view';
import type { InlineCompletion, InlineCompletionSource } from './inline-completion';

/** The generic highlight-class vocabulary. A language's {@link LanguageDescriptor.classify}
 *  maps its concrete CST node types onto one of these; the highlighter then emits a
 *  `cm-tok-<class>` mark decoration, styled by {@link import('./theme').codeEditorTheme}.
 *
 *  Kept intentionally broad + language-neutral so different grammars (Java, JSON,
 *  Rust, …) can all reuse the same theme. A descriptor may return any string, but
 *  only the classes below have theme styling out of the box — an unknown class is
 *  rendered as `cm-tok-<class>` (harmless, just unstyled) so a grammar can grow new
 *  token kinds and add matching theme rules later. */
export type TokenClass =
  | 'keyword'
  | 'string'
  | 'number'
  | 'comment'
  | 'type'
  | 'function'
  | 'ident'
  | 'operator'
  | 'punctuation'
  | 'constant'
  | 'annotation'
  | 'label'
  /** A field/property reference or declaration (distinct colour from locals). */
  | 'field'
  /** A method *declaration* name (vs `function` = a call site). */
  | 'declaration'
  /** `this` / `super` and other language self-references. */
  | 'self';

/** A {@link TokenClass}, or any other class name a grammar wants to grow — rendered as
 *  `cm-tok-<name>`, styled if the theme knows it and harmlessly plain if it doesn't. The
 *  `string & {}` keeps the union's autocomplete alive while admitting the rest (a JSP
 *  taglib prefix's `ns-3`, say). */
export type TokenClassName = TokenClass | (string & {});

/** Severity of an {@link EditorDiagnostic} (maps 1:1 onto CodeMirror's lint severities). */
export type DiagnosticSeverity = 'error' | 'warning' | 'info' | 'hint';

/** A quick-fix offered on a diagnostic (CodeMirror lint action). `apply` receives the
 *  view + the diagnostic's mapped UTF-16 range, so a "replace" action can edit in
 *  place and an "add to dictionary" action can ignore them. */
export interface EditorDiagnosticAction {
  name: string;
  apply: (view: EditorView, from: number, to: number) => void;
}

/** One located diagnostic in **UTF-8 byte offsets** into the source (the backend
 *  wire coordinate). The host maps `from`/`to` onto CodeMirror's UTF-16 offsets
 *  before pushing them into the lint gutter (see `CodeEditor.setDiagnostics`). A
 *  whole-file diagnostic can use `from === to === 0`. */
export interface EditorDiagnostic {
  /** Source byte offset of the span start. */
  from: number;
  /** Source byte offset of the span end (== `from` for a point diagnostic). */
  to: number;
  severity: DiagnosticSeverity;
  message: string;
  /**
   * The provider's stable kind slug (`unused-import`, `unhandled-checked-exception`), when it has
   * one.
   *
   * Carried because a **fix** is keyed by kind, never by message: it is what lets the host ask for
   * the repair of the squiggle under the caret without the message becoming a wire format that
   * cannot be reworded. Absent for a diagnostic whose provider has no catalogue.
   */
  code?: string;
  /** Optional quick-fixes (e.g. spell-check "Add to dictionary" / "Replace with …"). */
  actions?: EditorDiagnosticAction[];
}

/** A resolved go-to-declaration target within the same buffer (UTF-16 offset). A
 *  descriptor's {@link LanguageDescriptor.resolveGoto} may return one so the editor
 *  jumps locally; returning `null` lets the host handle a cross-file jump. */
export interface GotoTarget {
  /** UTF-16 document offset to scroll/select to. */
  offset: number;
}

/** A snapshot of the editor's cursor + scroll, so a host can persist and restore it
 *  across a remount (e.g. per-tab, so returning to a tab lands the caret and scroll
 *  exactly where they were left). Offsets are UTF-16 document positions. */
export interface EditorViewSnapshot {
  anchor: number;
  head: number;
  scrollTop: number;
  /**
   * The undo/redo history, serialised — CodeMirror's `historyField` as JSON.
   *
   * The editor is remounted per tab, so its state (and its history with it) is built fresh every
   * time you come back to a file: everything you had typed there was still on disk, and nothing
   * of it was undoable. Carried here so a host that already remembers a tab's cursor can remember
   * how it got there too.
   *
   * Only emitted when the editor is torn down — serialising a history on every cursor move would
   * pay for it thousands of times to use it once.
   */
  history?: unknown;
}

/**
 * Everything a language needs to plug into the generic editor. Products author one
 * of these (e.g. `bennu/java-lang.ts`) and hand it to {@link import('./CodeEditor.svelte')}.
 */
export interface LanguageDescriptor {
  /** Stable id (`java`, `json`, …) — for debugging / keying parser caches. */
  id: string;

  /** Create a web-tree-sitter {@link Parser} bound to this language's grammar
   *  (loads the runtime + grammar wasm). Rejecting (e.g. the grammar wasm is
   *  missing) is graceful: the highlighter stays plain-text, no crash. Parsers are
   *  cheap; one per editor is fine, the heavy `Language` is shared internally.
   *  Ignored when {@link cmExtension} is set. */
  createParser: () => Promise<Parser>;

  /**
   * Optional: a ready-made CodeMirror language {@link Extension} (a `LanguageSupport`
   * or `StreamLanguage`) used for highlighting **instead of** the tree-sitter grammar.
   * When present the core skips `createParser`/`classify` (and the tree-driven
   * features `resolveGoto`/`foldNode` are inactive — there is no live tree). This lets
   * a product register CodeMirror-built-in / legacy-mode languages (XML, YAML, JSON,
   * CSS, …) alongside the tree-sitter ones without a core change. Highlighting comes
   * from the shared Lezer highlight style (see `theme.ts`).
   */
  cmExtension?: Extension;

  /**
   * Only for a {@link cmExtension} (Lezer) language: opt into CodeMirror's built-in
   * fold gutter, driven by the language's own `foldNodeProp` (e.g. `@codemirror/lang-html`
   * folds tag bodies, `lang-json` folds objects/arrays). Left off by default so legacy
   * `StreamLanguage` modes that carry no fold info don't render an empty gutter. Ignored
   * for tree-sitter descriptors (they fold via {@link foldNode} instead).
   */
  cmFold?: boolean;

  /**
   * Fold from ranges a **provider** supplies rather than from the buffer's own structure.
   *
   * For a language whose descriptor has no fold information of its own — a legacy `StreamLanguage`
   * mode carries none, which is why a `.rs` file had no fold gutter at all. The host pushes the
   * ranges (`setFoldRanges`); this only says that the machinery should be installed.
   *
   * Independent of {@link cmFold}: that one drives folding from a Lezer grammar's `foldNodeProp`,
   * this one from something outside the editor. A descriptor would not normally set both.
   */
  serverFold?: boolean;

  /**
   * Classify a **leaf** CST node into a {@link TokenClass}, or `null` to leave it
   * unstyled. `field` is the parent's field name for this child (disambiguates a
   * bare `identifier` used as a call name vs a type vs a plain reference);
   * `parentType` is the parent node's type. Mirrors merula-lang's `classifyToken`.
   */
  classify: (
    node: Node,
    isNamed: boolean,
    field: string | null,
    parentType: string | null,
  ) => TokenClassName | null;

  /** Optional: resolve the identifier under a Ctrl/Cmd+Click to a local target.
   *  Return a {@link GotoTarget} to jump within the buffer, or `null` to defer to
   *  the host (e.g. a cross-file open). Omit → the editor reports the word to
   *  `onGoto` and lets the host decide entirely. */
  resolveGoto?: (tree: Tree, offset: number) => GotoTarget | null;

  /**
   * Optional client-side code folding. Given a leaf/container node, return a
   * `{ from, to }` range (UTF-16 doc offsets) to fold — typically the *inside*
   * of the node (after the opening brace/marker, before the close) so the head
   * line stays visible, IntelliJ-style. Return `null` to leave the node
   * unfoldable. The core walks the live tree and offers a fold for the smallest
   * foldable node spanning a line's end; a language folds braced blocks, block
   * comments, tag bodies, etc. Omit → no fold gutter is installed.
   */
  foldNode?: (node: Node) => { from: number; to: number } | null;

  /**
   * Optional embedded-language highlighting: map a **leaf** CST node type to a CodeMirror
   * {@link StreamParser} whose tokens colour that node's raw text (e.g. a JSP `<script>`
   * body → JavaScript, `<style>` → CSS). The highlighter tokenizes the node's text with the
   * parser and emits `cm-tok-*` marks — no nested tree-sitter grammar / extra wasm needed.
   * Highlighting only (no completion) unless the grammar/host adds it separately.
   */
  injections?: Record<string, StreamParser<unknown>>;

  /**
   * Optional highlight for text the grammar **does not put in the tree**, painted from the
   * raw document instead.
   *
   * The rovescio of {@link injections}: that one refines a leaf the tree does have, this one
   * covers text it never produced. An external scanner can consume input while deciding a
   * token — geode's `.dig` scanner eats `#` comment lines whole, because to compute a line's
   * indentation it has to treat them as blank — and everything it swallows is invisible to
   * `classify`, which only ever sees leaves. The symptom is a construct that renders as
   * default text with no error anywhere, which reads as "the theme is wrong".
   *
   * Installed alongside the highlighter (either kind) and **under** the semantic layer, so a
   * server that later says something better about the same range still wins.
   */
  extraHighlight?: Extension;

  /**
   * Optional comment syntax for the toggle-comment command (`Ctrl+/`). Shaped exactly
   * like CodeMirror's `commentTokens` language-data so the core surfaces it via the
   * `EditorState.languageData` facet; `@codemirror/commands`' `toggleComment` (already
   * bound to `Mod-/` in the default keymap) then works even for a **tree-sitter**
   * descriptor, which bypasses CodeMirror's `Language` and so carries no built-in comment
   * data. A `cmExtension` (Lezer / legacy-mode) language already provides its own comment
   * tokens, so it leaves this unset.
   */
  commentTokens?: { line?: string; block?: { open: string; close: string } };

  /**
   * Optional editing behaviours that belong to the **language** rather than to the
   * editor — an escaped paste into a string literal, a smart delimiter, a language's
   * own input handler.
   *
   * Installed whichever way the descriptor highlights, which is why it is not folded
   * into {@link cmExtension}: that one is the highlighter, and a tree-sitter
   * descriptor (Java) cannot set it without losing its own colouring.
   */
  editing?: Extension;

  /** Optional language-intelligence hooks (autocomplete / hover / …). A product
   *  fills this in to grow completions without a core change; the core installs
   *  the matching CodeMirror extensions when present (see
   *  {@link CodeEditorIntel}). */
  intel?: CodeEditorIntel;
}

/**
 * Language-intelligence hooks the core wires into CodeMirror when a descriptor
 * provides them. Kept small + app-agnostic: a product supplies concrete
 * behaviour (e.g. a completion source backed by an IPC call) and the core owns
 * the CodeMirror plumbing (the `autocompletion` extension + keymap).
 */
export interface CodeEditorIntel {
  /** A CodeMirror {@link CompletionSource} — invoked as the user types; returns
   *  a `CompletionResult` (or null for "no completions here"). The product owns
   *  debouncing / async fetching / UTF-16↔byte mapping inside the source. */
  completion?: CompletionSource;

  /** A CodeMirror `hoverTooltip` source — invoked when the pointer rests over a
   *  position; returns a {@link Tooltip} (or null for "nothing here"). The core wraps
   *  it in the `hoverTooltip` extension. The product owns the async fetch + DOM. */
  hover?: (
    view: EditorView,
    pos: number,
    side: -1 | 1,
  ) => Tooltip | null | Promise<Tooltip | null>;

  /**
   * Ghost text: the greyed continuation shown at the caret, Tab to accept.
   *
   * Return the text that **certainly** follows this position, or `null`. The
   * distinction matters: elsewhere ghost text means a language model guessing,
   * whereas here it is meant to be derived from what the tool already knows —
   * the column list after `INSERT INTO t (` is a fact, not a prediction. A
   * source that is unsure should return `null`; absent is always better than
   * plausibly wrong, and one product (Picus) forbids model-generated content
   * outright.
   *
   * An {@link InlineCompletion} instead of a string when accepting should
   * **replace** a range rather than insert at the caret — the shape a shorthand
   * that stands for something longer needs.
   */
  inlineCompletion?: InlineCompletionSource;
}

export type { Parser, Tree, Node };
export type { CompletionSource };
export type { InlineCompletion, InlineCompletionSource };
