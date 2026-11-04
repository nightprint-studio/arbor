/**
 * Where a parse error is, in the text that was pasted — so the import modals can mark it in the editor and not
 * only say it in a bar. Offsets here are characters (what the DOM and JS strings use); `toDiagnostic` turns them
 * into the UTF-8 byte offsets the shared editor's diagnostics are in.
 */
import type { EditorDiagnostic } from '$lib/components/shared/ui/code-editor/types';

export interface TextError {
  message: string;
  /** Character offsets, `to` exclusive. */
  from: number;
  to: number;
}

const encoder = new TextEncoder();

/** The UTF-8 byte length of the first `chars` characters of `text`. */
export function byteOffset(text: string, chars: number): number {
  return encoder.encode(text.slice(0, Math.max(0, chars))).length;
}

/** The character offset of a 1-based line and column. */
export function offsetOfLine(text: string, line: number, column = 1): number {
  let at = 0;
  for (let l = 1; l < line; l++) {
    const next = text.indexOf('\n', at);
    if (next === -1) return text.length;
    at = next + 1;
  }
  return Math.min(text.length, at + Math.max(0, column - 1));
}

/** A position named by an engine's error message: `at position 12`, `line 3 column 5`, `at line 3 at column 5`. */
export function positionFromMessage(text: string, message: string): number | null {
  const pos = /position (\d+)/i.exec(message);
  if (pos) return Math.min(text.length, Number(pos[1]));
  const lc = /line (\d+)(?:[^\d]+column (\d+))?/i.exec(message);
  if (lc) return offsetOfLine(text, Number(lc[1]), lc[2] ? Number(lc[2]) : 1);
  return null;
}

/** The whole of a line (1-based) as an error range; the first line when the message names none. */
export function lineError(text: string, message: string, line = 1): TextError {
  const from = offsetOfLine(text, line);
  const end = text.indexOf('\n', from);
  return { message, from, to: end === -1 ? text.length : end };
}

/** `null` when the text is JSON; otherwise where the parser gave up (the first character when it does not say). */
export function jsonError(text: string): TextError | null {
  try {
    JSON.parse(text);
    return null;
  } catch (e) {
    const message = String((e as Error).message ?? e);
    const at = positionFromMessage(text, message) ?? 0;
    return { message, from: at, to: Math.min(text.length, at + 1) };
  }
}

/** `null` when the text is well-formed XML (or there is no parser to ask, outside a browser). */
export function xmlError(text: string): TextError | null {
  if (typeof DOMParser === 'undefined') return null;
  const doc = new DOMParser().parseFromString(text, 'application/xml');
  const bad = doc.getElementsByTagName('parsererror')[0];
  if (!bad) return null;
  const message = (bad.textContent ?? 'The XML is not well-formed.').trim().split('\n')[0];
  const at = positionFromMessage(text, message);
  return at === null ? lineError(text, message) : { message, from: at, to: Math.min(text.length, at + 1) };
}

export function toDiagnostic(text: string, error: TextError): EditorDiagnostic {
  return { from: byteOffset(text, error.from), to: byteOffset(text, error.to), severity: 'error', message: error.message };
}
