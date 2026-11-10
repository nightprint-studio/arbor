/**
 * Merula evaluation diagnostics → console lines.
 *
 * The backend reports a span as UTF-8 byte offsets into the source it was handed; a console line
 * needs `file:line:col` and a link that opens the file there. Pure, so the mapping is tested
 * without a backend or a window.
 */

import { makeByteToU16 } from '$lib/components/merula/editor/merula-lang';
import type { MerulaDiagnostic } from '$lib/ipc/merula/merula';
import type { LogPiece } from '$lib/types/log';

/** 1-based line and column (UTF-16 units) of a UTF-16 offset. */
export function lineColAt(src: string, offset: number): { line: number; col: number } {
  const end = Math.max(0, Math.min(offset, src.length));
  let line = 1;
  let lineStart = 0;
  for (let i = 0; i < end; i++) {
    if (src.charCodeAt(i) === 10) { line++; lineStart = i + 1; }
  }
  return { line, col: end - lineStart + 1 };
}

export interface ConsoleDiagnostic {
  /** `error` / `warning` / `info`, as the console stream wants it. */
  severity: MerulaDiagnostic['severity'];
  text: string;
  /** The location cut as a clickable piece, then the message; absent when the span is unknown. */
  pieces?: LogPiece[];
}

/** One backend diagnostic as a console line. A diagnostic without a span has no link. */
export function consoleDiagnostic(path: string, src: string, d: MerulaDiagnostic): ConsoleDiagnostic {
  if (d.start === null) return { severity: d.severity, text: d.message };
  const at = lineColAt(src, makeByteToU16(src)(d.start));
  const where = `${basename(path)}:${at.line}:${at.col}`;
  return {
    severity: d.severity,
    text: `${where}  ${d.message}`,
    pieces: [
      { text: where, token: 'path', link: { kind: 'file', path, line: at.line } },
      { text: `  ${d.message}` },
    ],
  };
}

function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}
