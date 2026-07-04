/**
 * Bennu mojibake-check IPC — find UTF-8-decoded-as-Cp1252 corruption (`Ã©` → `é`,
 * `â€™` → `'`) in a buffer, with the corrected character for a one-click fix.
 *
 * Its own file so concurrent edits to the main bennu IPC surface don't race. Round-trips
 * through the generic `bennu(...)` rpc bridge to the `bennu_mojibake_check` handler.
 */

import { bennu } from '../rpc';

/** One detected mojibake sequence + its correction — mirrors the BE `MojibakeHit`. */
export interface MojibakeHit {
  /** Start byte offset of the garbled sequence in the source. */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  /** The garbled text as it appears (e.g. `"Ã©"`). */
  bad: string;
  /** The single correct character it should be (e.g. `"é"`). */
  fix: string;
}

/** Scan `source` for mojibake, returning every hit (byte spans + suggested fix).
 *  Wire: `bennu_mojibake_check` — `MojibakeArgs { file, source }`. */
export function mojibakeCheck(file: string, source: string): Promise<MojibakeHit[]> {
  return bennu('bennu_mojibake_check', { args: { file, source } });
}
