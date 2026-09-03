/**
 * Custom Prism language extensions.
 *
 * Each module in this folder:
 *  1. Registers its grammar into `Prism.languages` on import.
 *  2. Exports a `highlightLine(code: string): string` function that handles
 *     per-line dispatch logic (e.g. routing to CSS grammar for style blocks).
 *
 * `CUSTOM_HIGHLIGHTERS` is consumed by `diff-formatter.ts`: whenever a file's
 * language has an entry here, the custom function is called instead of the
 * default `Prism.highlight(code, Prism.languages[lang], lang)`.
 */

import { highlightLine as svelteLine } from './svelte';
// Side-effect-only: each registers its own entry in `Prism.languages`.
//
// `ron`, `dig`, `merula` and `wgsl` are the languages **Bennu highlights and Prism does not**.
// `mermaid` is the other case: Prism HAS one, and it names its tokens things no theme here
// styles — see that file.
// Bennu's own buffers
// get their colour from a tree-sitter grammar or a CodeMirror mode; a fenced code block in a
// markdown document has neither, so without these a ```dig or ```merula block in a README is
// grey text — the one place where the languages this app is built around read worse than
// JavaScript.
import './xsd';
import './ron';
import './dig';
import './merula';
import './wgsl';
import './mermaid';

export const CUSTOM_HIGHLIGHTERS: Record<string, (code: string) => string> = {
  svelte: svelteLine,
};
