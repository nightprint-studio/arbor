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
import type { EditorView, Tooltip } from '@codemirror/view';

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
  ) => TokenClass | null;

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
}

export type { Parser, Tree, Node };
export type { CompletionSource };
