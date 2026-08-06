/**
 * Structural search & replace — find code by its shape, count it, rewrite it.
 *
 * Four calls, and they answer four different questions:
 *
 *   • {@link explainQuery} — "does this query read, and what does it mean?" Touches no files, so
 *     it runs on every keystroke and turns a syntax error into a message under the field rather
 *     than an empty result list you have to interpret.
 *   • {@link ssrSearch} — run it. Fire-and-forget: results stream back as events, because a
 *     query over a legacy tree takes seconds and a list that fills is worth more than one that
 *     appears.
 *   • {@link ssrPreview} — what a replacement WOULD do, file by file, with the before and after.
 *   • {@link ssrApply} — write it, refusing any file that changed since the preview.
 *
 * Same convention as the rest of the bennu IPC: one `args` object, snake_case fields.
 */

import { bennu } from '../rpc';

/** `[start, end)` in bytes — never characters. */
export interface ByteRange {
  start: number;
  end: number;
}

/** One match. */
export interface SsrHit {
  /** Absolute, forward-slashed — what a click opens. */
  file: string;
  /** Project-relative — what a row shows. */
  rel: string;
  line: number;
  range: ByteRange;
  preview: string;
  /** The method or class it sits in, when the query grouped by that. */
  enclosing: string | null;
  /**
   * A type constraint here could not be decided — the classpath does not reach that far.
   * Shown, never dropped: a count that quietly excludes what it could not resolve is a count
   * that lies about being complete.
   */
  unresolved: boolean;
}

/** One row of a grouped report. */
export interface SsrRow {
  key: string;
  count: number;
  unresolved: number;
  /** Distinct files — the "where" half of "what, where, how often". */
  files: number;
}

export interface SsrReport {
  /** `null` for an ungrouped query, which is a plain hit list. */
  groupedBy: string | null;
  rows: SsrRow[];
  total: number;
  unresolved: number;
  files: number;
}

/** The terminal progress event's payload. */
export interface SsrDone {
  report: SsrReport;
  /** Files the scope admitted. */
  scanned: number;
  /** Files actually parsed — what the literal pre-filter saved. */
  parsed: number;
  /** Whether the pre-filter applied at all. */
  prefiltered: boolean;
  capped: boolean;
}

/** What a query means, without running it. */
export interface SsrExplained {
  /** `null` when it reads. */
  error: string | null;
  /** 1-based line of the error, `0` for the query as a whole. */
  errorLine: number;
  alternatives: number;
  /** The names EVERY alternative binds — what a replacement may use. */
  captures: string[];
  /** `use of` written out as the patterns it stands for. Empty for an ordinary query. */
  expansion: string[];
  /** The literals the pre-filter will grep for. Empty means every file gets parsed. */
  literals: string[];
}

export interface SsrPreviewedFile {
  file: string;
  rel: string;
  hits: number;
  before: string;
  after: string;
  /** What the file was when this was built. Handed back to {@link ssrApply}. */
  digest: number;
}

export interface SsrPreview {
  files: SsrPreviewedFile[];
  hits: number;
}

export interface SsrApplied {
  written: string[];
  refused: { file: string; reason: string }[];
}

/** Read a query and say what it means. No files are touched. Wire: `bennu_ssr_explain`. */
export function explainQuery(query: string): Promise<SsrExplained> {
  return bennu('bennu_ssr_explain', { args: { query } });
}

/**
 * Start a search. Results stream back as `arbor://bennu/ssr-progress` events tagged with
 * `searchId` — `{ id, hits }` batches, then exactly one `{ id, done: true, … }` carrying the
 * report. The caller mints a fresh id per search and ignores events from superseded ones.
 *
 * Resolves once the scan is scheduled, not when it finishes. Wire: `bennu_ssr_search`.
 */
export function ssrSearch(root: string, query: string, searchId: string): Promise<void> {
  return bennu('bennu_ssr_search', { args: { root, query, search_id: searchId } });
}

/** What a replacement would do, file by file. Nothing is written. Wire: `bennu_ssr_preview`. */
export function ssrPreview(
  root: string,
  query: string,
  replacement: string,
): Promise<SsrPreview> {
  return bennu('bennu_ssr_preview', { args: { root, query, replacement } });
}

/**
 * Write what the preview showed.
 *
 * Each file carries the digest it had when the preview was built; the backend re-reads and
 * refuses any that changed. Rewriting a file from a plan built against a different version of it
 * is how a structural replace becomes a bug report.
 *
 * Wire: `bennu_ssr_apply`.
 */
export function ssrApply(
  root: string,
  files: { file: string; digest: number; after: string }[],
): Promise<SsrApplied> {
  return bennu('bennu_ssr_apply', { args: { root, files } });
}
