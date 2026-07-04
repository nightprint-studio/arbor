/**
 * Bennu Alt+Enter intentions IPC — one round-trip that returns every quick-fix applicable at the
 * caret (parameterize logging, NP-safe equals, isEmpty()/boolean/negated-comparison
 * simplifications). Each offer is a byte-range edit the editor applies via `replaceByteRange`.
 *
 * Routes through the generic `bennu(...)` bridge to the `bennu_intentions_at` handler. Adding a new
 * intention is a change in the Rust `bennu-intentions` crate only — no new IPC.
 */

import { bennu } from '../rpc';

/** One applicable intention — a stable id, a human label, and a byte-range edit. Mirrors the BE
 *  `OfferWire`. */
export interface IntentionOffer {
  id: string;
  label: string;
  /** Start byte offset of the range to replace. */
  start: number;
  /** End byte offset (exclusive). */
  end: number;
  replacement: string;
}

/** Every intention applicable at byte `offset` in `source` (empty when none fit).
 *  Wire: `bennu_intentions_at` — `{ file, source, offset }`. */
export function intentionsAt(file: string, source: string, offset: number): Promise<IntentionOffer[]> {
  return bennu('bennu_intentions_at', { args: { file, source, offset } });
}
