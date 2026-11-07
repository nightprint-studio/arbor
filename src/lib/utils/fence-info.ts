/**
 * The language a fenced code block names, read from its info string.
 *
 * ```` ```ptn title="x" ```` carries the language first and anything else after it; the tag is
 * case-insensitive (```` ```Patina ````), and a few authors glue attributes on with a brace or a
 * comma (```` ```ptn{1,3} ````). Only the first word is the language.
 */

/** Names that mean Patina, the Voriz mod language, to the Prism-backed views. */
const FENCE_ALIAS: Record<string, string> = { ptn: 'patina' };

/** The language word of a fence info string, lowercased; `null` when there is none. */
export function fenceLanguage(info: string | null | undefined): string | null {
  if (!info) return null;
  const word = info.trim().split(/[\s{,]/, 1)[0].toLowerCase();
  return word || null;
}

/** The canonical Prism grammar name for a fence info string (`ptn` → `patina`), or the bare word. */
export function fenceGrammarName(info: string | null | undefined): string | null {
  const lang = fenceLanguage(info);
  return lang ? (FENCE_ALIAS[lang] ?? lang) : null;
}
