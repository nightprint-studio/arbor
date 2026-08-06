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
export { createCodeEditorExtensions, refTextAt, type CodeEditorExtensionsOptions } from './extensions';
export { createHighlightPlugin, makeByteToU16, makeU16ToByte, parserReady } from './highlight';
export { hoverCardDom, parseDoc, type HoverCard } from './hover-card';
export {
  pasteIntoLiteral,
  type LiteralPasteRenderer,
  type LiteralPasteRefusal,
} from './paste-literal';
