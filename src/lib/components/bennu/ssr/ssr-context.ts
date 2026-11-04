/**
 * The text around a match, for the hover preview of structural search. Pure: it takes a file's text and the
 * match's byte range, which is what the backend reports.
 */

import type { Hunk } from './ssr-diff';

/** The UTF-16 index of UTF-8 byte offset `byte` in `text` (the end of the text when it is past it). */
export function byteToIndex(text: string, byte: number): number {
  let bytes = 0;
  let i = 0;
  while (i < text.length && bytes < byte) {
    const cp = text.codePointAt(i) ?? 0;
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    i += cp > 0xffff ? 2 : 1;
  }
  return i;
}

export interface ContextLine {
  /** 1-based line number in the file. */
  no: number;
  text: string;
  /** The columns of the match on this line (`to` exclusive); absent on a line the match does not touch. */
  from?: number;
  to?: number;
}

export interface MatchContext {
  lines: ContextLine[];
  /** First and last line of the match itself. */
  startLine: number;
  endLine: number;
  /** The match was longer than the preview and its tail is not shown. */
  cut: boolean;
}

/** The longest stretch of the match itself the preview draws. */
const MAX_MATCH_LINES = 12;

/** `radius` lines before and after the match, the match marked line by line. */
export function contextAround(text: string, range: { start: number; end: number }, radius = 3): MatchContext {
  const s = byteToIndex(text, range.start);
  const e = Math.max(s, byteToIndex(text, range.end));
  const all = text.split('\n');
  const starts: number[] = [];
  let at = 0;
  for (const line of all) {
    starts.push(at);
    at += line.length + 1;
  }
  const lineOf = (index: number) => {
    let lo = 0;
    let hi = starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (starts[mid] <= index) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  };
  const first = lineOf(s);
  const last = Math.max(first, lineOf(Math.max(s, e - 1)));
  const shownLast = Math.min(last, first + MAX_MATCH_LINES - 1);
  const from = Math.max(0, first - radius);
  const to = Math.min(all.length - 1, shownLast + radius);

  const lines: ContextLine[] = [];
  for (let i = from; i <= to; i++) {
    const text0 = all[i].replace(/\r$/, '');
    const entry: ContextLine = { no: i + 1, text: text0 };
    if (i >= first && i <= shownLast) {
      entry.from = i === first ? s - starts[i] : 0;
      entry.to = i === last ? e - starts[i] : text0.length;
      entry.to = Math.min(entry.to, text0.length);
      entry.from = Math.min(entry.from, entry.to);
    }
    lines.push(entry);
  }
  return { lines, startLine: first + 1, endLine: last + 1, cut: last > shownLast };
}

/**
 * The hunk of a file's replacement diff that belongs to the match on lines `startLine..=endLine` (old
 * numbering): the one holding a changed line inside that span, else the one that covers it.
 */
export function hunkFor(hunks: readonly Hunk[], startLine: number, endLine: number): Hunk | null {
  const inSpan = (n: number | undefined) => n !== undefined && n >= startLine && n <= endLine;
  const changed = hunks.find((h) => h.lines.some((l) => l.kind !== 'same' && inSpan(l.oldNo ?? l.newNo)));
  if (changed) return changed;
  return hunks.find((h) => h.lines.some((l) => inSpan(l.oldNo))) ?? null;
}
