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
  /** What the hint says on hover — a parameter-name hint carries the parameter's declared type.
   *  Absent when the label already says everything there is to say. */
  tooltip?: string;
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

/** One declaration's use count — mirrors the BE `UsageCountWire`. */
export interface UsageCount {
  /** Byte offset of the declaration, annotations included — where the row above it is drawn. */
  decl: number;
  /** Byte span of the NAME token — what the "nothing reaches this" tint colours. */
  start: number;
  end: number;
  /** `"type"` | `"method"` | `"field"`. */
  kind: string;
  /** The declared name. */
  name: string;
  /** How many use sites the index holds. The declaration itself is not one of them. */
  count: number;
  /** True only when a count of zero is a fact about the **program** and not merely about the
   *  index — anything carrying an annotation, any override, and `main` are excluded, because
   *  something outside the code can reach each of them. Grey exactly these. */
  unused: boolean;
}

/** How many places use each declaration in the buffer — **minus the ones not worth a word**.
 *
 *  A declaration a framework calls and nothing else does (a `@Test`, a `@Bean`, `main`, an
 *  override) is not in the list at all: its count is zero by design, and saying so above every
 *  method of a test file is noise. What arrives is what should be drawn.
 *
 *  Java-only; resolves to `[]` while the index is still building or for a file no project owns.
 *  Wire: `bennu_usage_counts` — `{ file, source }`. */
export function usageCounts(file: string, source: string): Promise<UsageCount[]> {
  return bennu('bennu_usage_counts', { args: { file, source } });
}
