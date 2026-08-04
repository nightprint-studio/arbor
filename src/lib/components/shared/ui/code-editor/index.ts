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
export { sqlHighlight, type SqlDialect } from './sql-modes';
export { createCodeEditorExtensions, refTextAt, type CodeEditorExtensionsOptions } from './extensions';
export { createHighlightPlugin, makeByteToU16, makeU16ToByte, parserReady } from './highlight';
export { hoverCardDom, parseDoc, type HoverCard } from './hover-card';
export {
  pasteIntoLiteral,
  type LiteralPasteRenderer,
  type LiteralPasteRefusal,
} from './paste-literal';
