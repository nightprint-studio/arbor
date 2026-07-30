/**
 * Structural search and replace across the repository.
 *
 * Three calls in the order they are meant to be used, and the order is the point:
 * **find** shows what the pattern caught and what each match would become,
 * **preview** returns the exact bytes with a digest, **apply** hands the digests
 * back and refuses if anything moved. The same discipline as a generation, for
 * the same reason — this writes into the same scripts.
 */

import { picus } from '../rpc';
import type { Dialect, FolderRole, LineEnding } from '$lib/types/picus';

/** Which scripts a transformation may touch. Every field narrows. */
export interface RestructureScope {
  /** Project-relative folder. */
  folder?: string;
  /** A portable script belongs to both engines, so it survives either. */
  engine?: Dialect;
  role?: FolderRole;
  /** Explicit list — wins over the rest. */
  paths?: string[];
}

/** One place a pattern matched, in whatever text it was matched against. */
export interface Hit {
  /** UTF-8 byte offsets — everything the backend reports is in bytes. */
  range: { start: number; end: number };
  /** 1-based. */
  line: number;
  text: string;
  /** What each placeholder caught here. The column that says whether the pattern
   *  caught what was meant, before anything is rewritten. */
  captures: Record<string, string>;
  /** What this match would become. Only when a replacement was supplied. */
  replacement?: string;
  /** Why this one could not be rendered — a template naming a placeholder that
   *  matched nothing here, an index past the end of a list. */
  problem?: string;
}

/** A {@link Hit}, and which script it was in. Flat on the wire. */
export interface FoundMatch extends Hit {
  path: string;
}

export interface FindResult {
  matches: FoundMatch[];
  /** Scripts actually looked at — the denominator the count means nothing without. */
  scanned: number;
  /** The placeholder names the pattern declares, in order. */
  placeholders: string[];
}

/**
 * Find every place the pattern matches.
 *
 * `replacement` is optional, and passing it is how a template is checked before a
 * preview is asked for: each match comes back carrying what it would become.
 */
export function structuralFind(
  root: string,
  pattern: string,
  replacement?: string,
  scope?: RestructureScope,
): Promise<FindResult> {
  return picus('picus_structural_find', { root, pattern, replacement, scope });
}

export interface RestructuredFile {
  path: string;
  encoding: string;
  eol: LineEnding;
  before: string;
  after: string;
  matches: number;
  /** Hand back to `structuralApply` unchanged. */
  digest: string;
}

export interface RestructurePreview {
  files: RestructuredFile[];
  /** Matched but could not be prepared, with the reason. A migration missing a
   *  file is worse than one that says which file it cannot do. */
  refused: { path: string; reason: string }[];
}

export function structuralPreview(
  root: string,
  pattern: string,
  replacement: string,
  scope?: RestructureScope,
): Promise<RestructurePreview> {
  return picus('picus_structural_preview', { root, pattern, replacement, scope });
}

export interface ScanResult {
  matches: Hit[];
  placeholders: string[];
}

/**
 * Find every place the pattern matches **one buffer** — the document in front of
 * the user rather than the repository.
 *
 * It writes nothing, and there is deliberately no `apply` beside it: the ranges
 * come back and the editor splices them itself, which is what keeps a structural
 * replace inside the buffer's own undo history instead of being the one edit that
 * cannot be taken back.
 */
export function structuralScan(
  text: string,
  pattern: string,
  replacement?: string,
): Promise<ScanResult> {
  return picus('picus_structural_scan', { text, pattern, replacement });
}

export function structuralApply(
  root: string,
  pattern: string,
  replacement: string,
  digests: { path: string; digest: string }[],
  scope?: RestructureScope,
): Promise<{ written: string[]; unchanged: string[] }> {
  return picus('picus_structural_apply', { root, pattern, replacement, scope, digests });
}
