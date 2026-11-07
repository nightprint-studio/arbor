/**
 * Which Prism grammars the hover card colours its parts with, chosen from the file the hover is on.
 *
 * `codeLanguage` is the language of the code blocks in the doc. `signatureLanguage` colours the title line;
 * it is only set for the languages whose signature the backend gives as source text in that language.
 * Patina (`.ptn`) is one: without it the card shows `pub fn f(x: int) -> int` as plain text next to
 * a Java or Rust one that has colour. Every other language keeps the card exactly as it was.
 */

import { getLanguage } from '$lib/utils/diff-formatter';

/** Languages whose hover signature is coloured as source in that language. */
const COLOURED_SIGNATURE = new Set(['patina']);

export function hoverLanguages(path: string): { codeLanguage: string; signatureLanguage?: string } {
  const codeLanguage = getLanguage(path);
  return COLOURED_SIGNATURE.has(codeLanguage) ? { codeLanguage, signatureLanguage: codeLanguage } : { codeLanguage };
}
