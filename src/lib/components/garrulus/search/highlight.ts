/**
 * Turning the backend's match ranges into something a template can render.
 *
 * **The ranges are UTF-8 byte offsets.** `garrulus-index` spells that out on both
 * `Hit.title_matches` and `Snippet.ranges`, and JavaScript strings are UTF-16:
 * slicing with a byte offset is correct for ASCII and silently wrong the moment a
 * note contains an accent, an em dash or an emoji — which, in an Italian vault, is
 * the first line of the first note. Every highlight in the search view goes
 * through {@link highlightSegments} so that conversion exists once.
 *
 * The other direction — highlighting text the backend sent no ranges for, such as
 * the preview of a whole note — is {@link findTermRanges}, which works in UTF-16
 * from the start and never touches a byte offset.
 */

import type { MatchRange } from '$lib/ipc/garrulus';

/** A run of text, either matched or not. Rendered as `<mark>` or as itself. */
export interface Segment {
  text: string;
  hit: boolean;
}

/** Sort, drop the empty ones, and merge what overlaps or touches — two terms
 *  matching adjacent characters are one highlight, not two `<mark>`s that
 *  happen to abut. */
function mergeRanges(ranges: readonly MatchRange[]): MatchRange[] {
  const sorted = ranges
    .filter((r) => r.end > r.start)
    .slice()
    .sort((a, b) => a.start - b.start || a.end - b.end);

  const out: MatchRange[] = [];
  for (const r of sorted) {
    const last = out[out.length - 1];
    if (last && r.start <= last.end) last.end = Math.max(last.end, r.end);
    else out.push({ start: r.start, end: r.end });
  }
  return out;
}

/**
 * Map the given UTF-8 byte offsets to UTF-16 string indices, in one pass.
 *
 * An offset that lands inside a multi-byte character clamps forward to the next
 * character boundary: a highlight one character wide in the wrong direction is a
 * cosmetic error, while a split surrogate pair is a broken string.
 */
function byteToCharIndex(text: string, offsets: readonly number[]): Map<number, number> {
  const wanted = [...new Set(offsets)].sort((a, b) => a - b);
  const map = new Map<number, number>();

  let byte = 0;
  let w = 0;
  let i = 0;

  while (i < text.length && w < wanted.length) {
    while (w < wanted.length && wanted[w] <= byte) {
      map.set(wanted[w], i);
      w++;
    }
    if (w >= wanted.length) break;

    const cp = text.codePointAt(i) as number;
    byte += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    i += cp > 0xffff ? 2 : 1;
  }

  // Whatever is left addresses a byte at or past the end of the string.
  while (w < wanted.length) {
    map.set(wanted[w], text.length);
    w++;
  }
  return map;
}

/**
 * Split `text` into rendered runs, given match ranges expressed in **bytes**.
 *
 * Returns a single unmatched segment when there is nothing to highlight, so a
 * caller can always render the result the same way.
 */
export function highlightSegments(text: string, ranges: readonly MatchRange[]): Segment[] {
  const merged = mergeRanges(ranges);
  if (merged.length === 0) return text ? [{ text, hit: false }] : [];

  const map = byteToCharIndex(text, merged.flatMap((r) => [r.start, r.end]));

  const out: Segment[] = [];
  let at = 0;
  for (const r of merged) {
    const start = Math.max(at, map.get(r.start) ?? 0);
    const end = Math.max(start, map.get(r.end) ?? start);
    if (start > at) out.push({ text: text.slice(at, start), hit: false });
    if (end > start) out.push({ text: text.slice(start, end), hit: true });
    at = end;
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: false });
  return out;
}

/**
 * The words a free-text query searches for.
 *
 * Mirrors `garrulus-index`'s `tokenize`: alphanumeric runs, lowercased, so
 * `[[Nota]]` and `nota` are the same term and punctuation never becomes one.
 */
export function queryTerms(text: string): string[] {
  return text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? [];
}

/**
 * Every occurrence of every term in `text`, as **UTF-16** ranges.
 *
 * For surfaces the backend sent no ranges for — the preview of a whole note.
 * Case-insensitive through `toLowerCase()` on both sides, which can change a
 * string's length for a handful of characters (`İ`); the ranges are therefore
 * computed against the lowercased copy and only used when its length matches the
 * original, rather than risking an offset that slices mid-character.
 */
export function findTermRanges(text: string, terms: readonly string[]): MatchRange[] {
  const hay = text.toLowerCase();
  if (hay.length !== text.length) return [];

  const out: MatchRange[] = [];
  for (const term of terms) {
    if (!term) continue;
    let from = 0;
    for (;;) {
      const at = hay.indexOf(term, from);
      if (at === -1) break;
      out.push({ start: at, end: at + term.length });
      from = at + term.length;
    }
  }
  return out;
}

/**
 * Split `text` into runs given ranges already expressed in UTF-16 indices — the
 * companion of {@link findTermRanges}.
 */
export function highlightCharSegments(text: string, ranges: readonly MatchRange[]): Segment[] {
  const merged = mergeRanges(ranges);
  if (merged.length === 0) return text ? [{ text, hit: false }] : [];

  const out: Segment[] = [];
  let at = 0;
  for (const r of merged) {
    const start = Math.max(at, Math.min(r.start, text.length));
    const end = Math.max(start, Math.min(r.end, text.length));
    if (start > at) out.push({ text: text.slice(at, start), hit: false });
    if (end > start) out.push({ text: text.slice(start, end), hit: true });
    at = end;
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: false });
  return out;
}
