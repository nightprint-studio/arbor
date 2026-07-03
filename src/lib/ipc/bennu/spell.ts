/**
 * Bennu spell-check IPC — Hunspell (EN + IT) checking of declared identifiers and
 * comments. Kept in its own file (not `index.ts`) so concurrent edits to the main
 * surface don't race.
 *
 * All calls route through the generic `bennu(...)` rpc bridge, wrapping fields under
 * `{ args: … }` (the proven convention). Wire shapes mirror the BE handlers
 * (`crates/products/bennu/be/src/spell.rs`) verbatim.
 */

import { bennu } from '../rpc';

/** One misspelled sub-word — mirrors the BE `SpellHit`. Byte offsets into the file. */
export interface SpellHit {
  /** Start byte offset of the misspelled sub-word. */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  /** The offending word. */
  word: string;
  /** Up to ~5 suggested corrections (EN first, then IT). */
  suggestions: string[];
}

/** Dictionary availability — mirrors the BE `SpellStatus`. */
export interface SpellStatus {
  /** At least one dictionary is on disk. */
  installed: boolean;
  /** Which of `en_US` / `it_IT` are present. */
  languages: string[];
}

/** Spell-check `source` (the current buffer) for `file`. Checks declaration-name
 *  identifiers (split by case/underscore/hyphen) + comments against EN/IT + the
 *  allow-list + the custom dictionaries. Returns `[]` when no dictionary is installed
 *  or the file isn't Java. Wire: `bennu_spellcheck — { file, source }`. */
export function spellcheck(file: string, source: string): Promise<SpellHit[]> {
  return bennu('bennu_spellcheck', { args: { file, source } });
}

/** Add `word` to a custom dictionary (`scope` = 'project' | 'global'); `root` is the
 *  project root (for the per-project dict). Subsequent `spellcheck` calls reflect it.
 *  Wire: `bennu_dict_add — { word, scope, root }`. */
export function dictAdd(word: string, scope: 'project' | 'global', root: string): Promise<void> {
  return bennu('bennu_dict_add', { args: { word, scope, root } });
}

/** Query which dictionaries are installed. Wire: `bennu_spell_status — {}`. */
export function spellStatus(): Promise<SpellStatus> {
  return bennu('bennu_spell_status', { args: {} });
}

/** Download the EN + IT Hunspell dictionaries (LibreOffice) into the data dir. Emits
 *  `arbor://bennu/dict-progress` per file; resolves with the resulting status. Wire:
 *  `bennu_download_dictionaries — {}`. */
export function downloadDictionaries(): Promise<SpellStatus> {
  return bennu('bennu_download_dictionaries', { args: {} });
}
