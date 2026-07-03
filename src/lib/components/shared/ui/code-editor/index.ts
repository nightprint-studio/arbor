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
} from './types';
export { codeEditorTheme } from './theme';
export { createCodeEditorExtensions, refTextAt, type CodeEditorExtensionsOptions } from './extensions';
export { createHighlightPlugin, makeByteToU16, makeU16ToByte, parserReady } from './highlight';
