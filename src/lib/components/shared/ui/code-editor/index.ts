/**
 * Generic, app-agnostic CodeMirror 6 editor core.
 *
 * Parametrised by a {@link LanguageDescriptor} so any product/language reuses the
 * same glue. Depends only on `@codemirror/*`, `web-tree-sitter`, and its own files
 * — no Arbor-domain imports, so it belongs in `shared/ui/`.
 *
 * Usage:
 *   import { CodeEditor, type LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
 */

export { default as CodeEditor } from './CodeEditor.svelte';
export type {
  LanguageDescriptor,
  CodeEditorIntel,
  TokenClass,
  TokenClassName,
  DiagnosticSeverity,
  EditorDiagnostic,
  EditorViewSnapshot,
  GotoTarget,
  Parser,
  Tree,
  Node,
  CompletionSource,
  InlineCompletionSource,
  InlineCompletion,
} from './types';
export {
  inlineCompletion,
  acceptInlineCompletion,
  dismissInlineCompletion,
  inlineCompletionActive,
} from './inline-completion';
// Semantic highlighting — the token layer only something that knows the types can supply
// (a language server). Painted OVER the base highlight, never instead of it.
export {
  semanticHighlight,
  setSemanticTokens,
  semanticTokenCount,
  type SemanticToken,
} from './semantic-tokens';
export { codeEditorTheme } from './theme';
// A colour per namespace family (a taglib prefix, an XML namespace) — see the module.
export {
  NAMESPACE_COLORS,
  NAMESPACE_SLOTS,
  namespaceSlotFor,
  namespaceTokenClass,
} from './namespace-palette';
// Static code rendered OUTSIDE an editor instance — a search result's context, the sticky-scroll
// header. Emits the editor's own `cm-tok-*` classes, so it takes the theme for free.
export { highlightToHtml } from './mini-highlight';
export { sqlHighlight, type SqlDialect } from './sql-modes';
export { dtdLanguage, dtdMode } from './dtd-mode';
export { ronLanguageExtension, ronMode } from './ron-mode';
export { mermaidLanguageExtension, mermaidMode } from './mermaid-mode';
export { wgslLanguageExtension, wgslMode } from './wgsl-mode';
export { javascriptStream } from './js-mode';
export {
  createCodeEditorExtensions, refTextAt, preferencesCompartment,
  type CodeEditorExtensionsOptions, type CodeEditorPreferences, type CompletionPreferences,
} from './extensions';
export { intellijEditingKeymap } from './intellij-keymap';
export { createHighlightPlugin, makeByteToU16, makeU16ToByte, parserReady } from './highlight';
// Tab stops of an inserted completion — see `snippet-stops.ts` for why this is not CodeMirror's own
// `snippet()`.
export { insertWithStops, snippetStops } from './snippet-stops';
// Layers a provider supplies and the buffer cannot — occurrences of the symbol under the caret, and
// where the file folds.
export {
  documentHighlights, serverFolding, setDocumentHighlights, setFoldRanges,
  type FoldRange, type HighlightRange,
} from './server-layers';
// The counts a provider draws above an item — a pushed layer like the two above, and the only one
// that is a control rather than a decoration.
export { codeLensLayer, setCodeLenses, type LensEntry } from './code-lens';
export { hoverCardDom, parseDoc, type HoverCard } from './hover-card';
export {
  pasteIntoLiteral,
  type LiteralPasteRenderer,
  type LiteralPasteRefusal,
} from './paste-literal';
// Postfix templates — `expr.if` → `if (expr) { … }`. The engine is language-agnostic; each language
// supplies its own table.
export {
  postfixCompletion,
  expressionStart,
  extractStops,
  CARET,
  type PostfixTemplate,
  type PostfixOptions,
} from './postfix';
// Parameter hints — the signature of the call the caret is inside, with the active argument marked.
export {
  signatureHints,
  setSignature,
  clearSignature,
  showSignatureHint,
  type SignatureInfo,
} from './signature-hint';
// Inlay hints — text the provider draws between the code, not in it.
export { inlayHints, setInlayHints, type InlayHint } from './inlay-hints';
