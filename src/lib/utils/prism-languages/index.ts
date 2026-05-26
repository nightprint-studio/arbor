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
// Side-effect-only: registers `Prism.languages.xsd` (XML Schema).
import './xsd';

export const CUSTOM_HIGHLIGHTERS: Record<string, (code: string) => string> = {
  svelte: svelteLine,
};
