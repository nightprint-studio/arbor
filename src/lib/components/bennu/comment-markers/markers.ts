/**
 * Comment markers — `TODO`, `FIXME`, `HACK`… written inside a comment.
 *
 * The word list and the case rule are the same ones the TODO tool window scans for (the editor passes them
 * to `bennu_todos`), so a marker is coloured exactly when the window lists it.
 *
 * Pure: it works on text and a list of comment spans, so it is tested without an editor.
 */

import { commentSpans } from './comments';

export const DEFAULT_KEYWORDS = ['TODO', 'FIXME', 'HACK', 'XXX', 'NOTE', 'BUG', 'OPTIMIZE', 'DEPRECATED'] as const;

/** The most words a list may hold — every one is another alternative in the search. */
const MAX_KEYWORDS = 24;

/** Which theme colour a word takes. A tone, not a colour: the stylesheet maps it onto the theme's own. */
export type MarkerTone = 'accent' | 'danger' | 'warning' | 'info' | 'success' | 'muted';

const TONES: Record<string, MarkerTone> = {
  TODO: 'accent',
  FIXME: 'danger',
  BUG: 'danger',
  HACK: 'warning',
  XXX: 'warning',
  NOTE: 'info',
  OPTIMIZE: 'success',
  DEPRECATED: 'muted',
};

/** The tone of `keyword` (any spelling). A word the list gained without a tone of its own reads as a TODO. */
export function toneOf(keyword: string): MarkerTone {
  return TONES[keyword.toUpperCase()] ?? 'accent';
}

/** A list as the settings field writes it, to words: separators are commas, spaces and new lines. */
export function parseKeywords(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of text.split(/[\s,;]+/)) {
    const word = raw.trim();
    if (!word || !/^[A-Za-z0-9_]+$/.test(word)) continue;
    const key = word.toUpperCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(word);
    if (out.length >= MAX_KEYWORDS) break;
  }
  return out;
}

/** The list as one editable string. */
export const keywordsText = (words: readonly string[]): string => words.join(', ');

/** The list in force: what was configured, or the built-in words when that is empty. */
export function effectiveKeywords(configured: readonly string[]): string[] {
  const words = parseKeywords(configured.join(' '));
  return words.length ? words : [...DEFAULT_KEYWORDS];
}

/** One marker found in a comment. */
export interface MarkerHit {
  /** The word, and the `(author)` after it when there is one. */
  from: number;
  to: number;
  /** The word as the list spells it, upper-cased. */
  keyword: string;
  tone: MarkerTone;
  /** 1-based line of the marker. */
  line: number;
}

export interface MarkerOptions {
  keywords: readonly string[];
  caseSensitive: boolean;
}

/** The search for `keywords`: a whole word, optionally followed by `(author)`. Null for an empty list. */
export function markerPattern(keywords: readonly string[], caseSensitive: boolean): RegExp | null {
  const words = keywords.filter((w) => /^[A-Za-z0-9_]+$/.test(w));
  if (!words.length) return null;
  // Longest first: with `TODO` and `TODOS` listed, the alternation must not stop at the shorter one.
  const alt = [...words].sort((a, b) => b.length - a.length).join('|');
  return new RegExp(`(?<![A-Za-z0-9_])(${alt})(?![A-Za-z0-9_])(\\([^)\\n]{0,40}\\))?`, caseSensitive ? 'g' : 'gi');
}

/**
 * Every marker of `src` that sits inside a comment of the file `path`.
 *
 * The word search is a native regular expression over the whole text, and only when it finds something does
 * the comment scan run — so a file with no marker costs one pass of the engine and nothing else.
 */
export function findMarkers(src: string, path: string | null, opts: MarkerOptions): MarkerHit[] {
  const pattern = markerPattern(opts.keywords, opts.caseSensitive);
  if (!pattern) return [];
  const found: RegExpExecArray[] = [];
  for (let m = pattern.exec(src); m; m = pattern.exec(src)) found.push(m);
  if (!found.length) return [];

  const spans = commentSpans(src, path);
  if (!spans.length) return [];
  const lineStarts = lineStartsOf(src);
  const hits: MarkerHit[] = [];
  for (const m of found) {
    if (!insideAny(spans, m.index)) continue;
    const keyword = m[1].toUpperCase();
    hits.push({
      from: m.index,
      to: m.index + m[0].length,
      keyword,
      tone: toneOf(keyword),
      line: lineAt(lineStarts, m.index),
    });
  }
  return hits;
}

function insideAny(spans: readonly { from: number; to: number }[], pos: number): boolean {
  let lo = 0;
  let hi = spans.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const s = spans[mid];
    if (pos < s.from) hi = mid - 1;
    else if (pos >= s.to) lo = mid + 1;
    else return true;
  }
  return false;
}

function lineStartsOf(src: string): number[] {
  const starts = [0];
  for (let i = src.indexOf('\n'); i >= 0; i = src.indexOf('\n', i + 1)) starts.push(i + 1);
  return starts;
}

function lineAt(starts: readonly number[], pos: number): number {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= pos) lo = mid;
    else hi = mid - 1;
  }
  return lo + 1;
}
