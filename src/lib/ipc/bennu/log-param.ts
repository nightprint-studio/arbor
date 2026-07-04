/**
 * Bennu log-parameterization IPC — the "parameterize a logging call" quick-fix. Given the buffer
 * + a caret byte offset, returns the edit that rewrites `logger.info("x " + v)` into
 * `logger.info("x {}", v)` when the caret is inside such a call, or `null` otherwise.
 *
 * Its own file so concurrent edits to the main bennu IPC surface don't race. Round-trips through
 * the generic `bennu(...)` rpc bridge to the `bennu_log_param` handler.
 */

import { bennu } from '../rpc';

/** The edit to apply — replace `source[start..end]` (the argument list, parens excluded) with
 *  `replacement`. Byte offsets. Mirrors the BE `LogParamResult`. */
export interface LogParamResult {
  /** Start byte offset of the argument list (just after `(`). */
  start: number;
  /** End byte offset of the argument list (the `)` position). */
  end: number;
  /** The rewritten argument list. */
  replacement: string;
}

/** Ask whether the logging call at byte `offset` can be parameterized. Resolves the edit, or
 *  `null` when the caret isn't inside a qualifying concatenated-message logging call.
 *  Wire: `bennu_log_param` — `LogParamArgs { file, source, offset }`. */
export function logParam(file: string, source: string, offset: number): Promise<LogParamResult | null> {
  return bennu('bennu_log_param', { args: { file, source, offset } });
}
