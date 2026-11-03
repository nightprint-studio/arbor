/**
 * Where the colours of a logging pattern go — the pure half of {@link ./log-pattern-highlight}.
 *
 * A pattern is a small language written inside a string — `%d{HH:mm:ss.SSS} [%t] %-5level %logger{36} - %msg%n` —
 * and this finds its parts: every `%`-conversion by what it prints, its `{options}`, a `${property}`. No CodeMirror in
 * here, so it is testable as plain text in, marks out.
 *
 * ## Where a pattern is
 *
 * Four places, all found by one pass over the text: a `pattern="…"` (Log4j2's `PatternLayout`), a `filePattern="…"` (its
 * rolling archives), a `<pattern>…</pattern>` or `<fileNamePattern>…</…>` (Logback) and Log4j 1.x's
 * `<param name="ConversionPattern" value="…"/>`. A scan and not a parse — the XML mode is a stream parser with no
 * tree — bounded by {@link MAX_SCANNED}.
 */

/** Past this many characters the file is left to the plain XML colouring. */
const MAX_SCANNED = 600_000;

/** What a coloured stretch of a pattern is. Each is a CSS class, `cm-lp-<kind>`. */
export type PatternKind = 'date' | 'level' | 'logger' | 'thread' | 'msg' | 'mdc' | 'dim' | 'other' | 'opt' | 'prop';

export interface PatternMark {
  from: number;
  to: number;
  kind: PatternKind;
}

/** A conversion word → what it prints. Log4j2, Logback and Log4j 1.x spell the same things several ways. */
const KIND_OF_WORD: Record<string, PatternKind> = {
  d: 'date', date: 'date',
  p: 'level', level: 'level', le: 'level',
  c: 'logger', logger: 'logger', lo: 'logger', C: 'logger', class: 'logger',
  t: 'thread', thread: 'thread', tn: 'thread', threadName: 'thread',
  m: 'msg', msg: 'msg', message: 'msg',
  X: 'mdc', mdc: 'mdc', MDC: 'mdc', K: 'mdc', map: 'mdc',
  // What ends or follows the line: the newline and the exception.
  n: 'dim', ex: 'dim', exception: 'dim', throwable: 'dim', rEx: 'dim', rootException: 'dim', xEx: 'dim',
  xException: 'dim', xThrowable: 'dim',
};

/** Conversions whose **first option is itself a pattern**: `%highlight{%-5level}{FATAL=red}`. */
const WRAPPERS = new Set([
  'highlight', 'style', 'maxLen', 'maxLength', 'notEmpty', 'varsNotEmpty', 'replace', 'encode', 'equals', 'equalsIgnoreCase',
  'black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white', 'gray',
  'boldRed', 'boldGreen', 'boldYellow', 'boldBlue', 'boldMagenta', 'boldCyan', 'boldWhite',
]);

/** The index of the bracket that closes the one at `open`, nesting respected, or -1. */
function closing(text: string, open: number, to: number, left: string, right: string): number {
  let depth = 0;
  for (let i = open; i < to; i++) {
    if (text[i] === left) depth++;
    else if (text[i] === right && --depth === 0) return i;
  }
  return -1;
}

/** The marks of one pattern, `[from, to)` of `text`, in document order and never overlapping. */
function scanPattern(text: string, from: number, to: number, out: PatternMark[]): void {
  let i = from;
  while (i < to) {
    const c = text[i];
    if (c === '$' && text[i + 1] === '{') {
      const end = closing(text, i + 1, to, '{', '}');
      if (end < 0) break;
      out.push({ from: i, to: end + 1, kind: 'prop' });
      i = end + 1;
      continue;
    }
    if (c !== '%') {
      i++;
      continue;
    }
    if (text[i + 1] === '%') {
      i += 2;
      continue;
    }
    // `%-5.30level`: the padding and truncation, then the word that says what is printed.
    let j = i + 1;
    while (j < to && /[-.\d]/.test(text[j])) j++;
    const wordStart = j;
    while (j < to && /[A-Za-z]/.test(text[j])) j++;
    if (j === wordStart) {
      i++;
      continue;
    }
    const word = text.slice(wordStart, j);
    out.push({ from: i, to: j, kind: KIND_OF_WORD[word] ?? 'other' });

    const wrapper = WRAPPERS.has(word);
    let first = true;
    while (j < to && text[j] === '{') {
      const end = closing(text, j, to, '{', '}');
      if (end < 0) break;
      if (first && wrapper) scanPattern(text, j + 1, end, out);
      else out.push({ from: j, to: end + 1, kind: 'opt' });
      first = false;
      j = end + 1;
    }
    // Logback's colour wrappers take the pattern in parentheses: `%highlight(%-5level)`.
    if (wrapper && text[j] === '(') {
      const end = closing(text, j, to, '(', ')');
      if (end > 0) {
        scanPattern(text, j + 1, end, out);
        j = end + 1;
      }
    }
    i = j;
  }
}

/**
 * Every pattern in the document: an attribute, an element's text, or Log4j 1.x's `ConversionPattern`
 * param. Groups: 1/2 a quoted attribute value, 3/4 an element's name and text, 5/6 the param's value.
 */
const PATTERNS =
  /\b(?:pattern|filePattern|fileNamePattern)\s*=\s*(?:"([^"]*)"|'([^']*)')|<(pattern|fileNamePattern)>([\s\S]*?)<\/\3>|name\s*=\s*["']ConversionPattern["']\s+value\s*=\s*(?:"([^"]*)"|'([^']*)')/g;

/** The marks of a whole `log4j2.xml` / `logback.xml`, in document order. Pure — the plugin below only draws them. */
export function patternMarks(text: string): PatternMark[] {
  const out: PatternMark[] = [];
  if (text.length > MAX_SCANNED) return out;
  PATTERNS.lastIndex = 0;
  for (let m = PATTERNS.exec(text); m; m = PATTERNS.exec(text)) {
    const end = m.index + m[0].length;
    if (m[3] !== undefined) {
      // `<pattern>` + text: the text starts after the opening tag.
      const from = m.index + m[3].length + 2;
      scanPattern(text, from, from + m[4].length, out);
      continue;
    }
    // An attribute value ends one quote before the match does.
    const value = m[1] ?? m[2] ?? m[5] ?? m[6] ?? '';
    scanPattern(text, end - 1 - value.length, end - 1, out);
  }
  // Regions come out in document order and a pattern's marks never overlap, so no sort is needed.
  return out;
}
