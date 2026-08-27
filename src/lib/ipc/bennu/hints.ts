/**
 * Parameter hints and inlay hints — what the editor draws around a call.
 *
 * Java-only by construction: a file served by a language server gets both from its server, through
 * `lsp.ts`. Offsets are **UTF-8 bytes**, like every other Bennu span, and the editor maps them the
 * way it maps a diagnostic's.
 */

import { bennu } from '../rpc';

/** The signature of the call the caret is inside. */
export interface SignatureHelp {
  /** The rendered signature — `transfer(String source, String target, long amount)`. */
  label: string;
  /** `[start, end)` **byte** ranges within `label`, one per parameter. */
  params: [number, number][];
  /** Index into `params` of the argument the caret is on. */
  active: number;
  /** Byte offset of the call's opening paren — what the strip is anchored to. */
  anchor: number;
  /** `[index, count]` when the name was overloaded. */
  overload?: [number, number];
}

/** One hint drawn between the code. */
export interface InlayHint {
  /** **Byte** offset the hint is drawn at. */
  offset: number;
  /** `source:` for a parameter name, `: Order` for an inferred type. */
  label: string;
  /** `true` when the hint belongs in front of what is at `offset`. */
  before: boolean;
}

/** The signature of the call at `offset`, or `null` when there isn't one to show.
 *  Wire: `bennu_signature_help` — `SignatureArgs { file, source, offset }`. */
export function signatureHelp(
  file: string,
  source: string,
  offset: number,
): Promise<SignatureHelp | null> {
  return bennu('bennu_signature_help', { args: { file, source, offset } });
}

/** Every inlay hint for the buffer.
 *  Wire: `bennu_inlay_hints` — `InlayArgs { file, source }`. */
export function inlayHints(file: string, source: string): Promise<InlayHint[]> {
  return bennu('bennu_inlay_hints', { args: { file, source } });
}
