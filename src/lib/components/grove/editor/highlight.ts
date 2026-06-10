/**
 * Tiny `.grove` syntax tokenizer for the Step 0 mocked editor. NOT the real
 * highlighter — Phase 4 wires CodeMirror 6 + Tree-sitter. This just splits a
 * line into coarse token spans (keyword / fn / string / comment / number /
 * note / operator / island) so the read-only code view looks alive. It is
 * line-based and forgiving; it never throws on malformed input.
 */

export type TokenKind =
  | 'comment' | 'string' | 'keyword' | 'island' | 'fn' | 'number'
  | 'note' | 'operator' | 'punct' | 'ident' | 'plain';

export interface Token { text: string; kind: TokenKind; }

const KEYWORDS = new Set(['let', 'fn', 'import', 'from', 'cps', 'tracks', 'track', 'arrange', 'cycles', 'par', 'seq', 'cat']);
const ISLANDS  = new Set(['s', 'sound', 'n', 'note']);
const XFORMS   = new Set([
  'fast', 'slow', 'rev', 'every', 'off', 'degrade', 'sometimes', 'jux',
  'gain', 'pan', 'room', 'lpf', 'hpf', 'inst', 'scale', 'delay', 'crush',
  'shape', 'shift', 'speed', 'sample', 'audio', 'rand', 'choose', 'map',
  'log', 'debug', 'info', 'warn', 'error', 'trace',
]);

// A pitch-ish word: a–g, optional s/f accidental, optional octave digit.
const NOTE_RE = /^[a-g](s|f)?\d?$/;

/** Tokenize one source line into spans. */
export function tokenizeLine(line: string): Token[] {
  const out: Token[] = [];
  let i = 0;
  const n = line.length;

  const push = (text: string, kind: TokenKind) => { if (text) out.push({ text, kind }); };

  while (i < n) {
    const c = line[i];

    // line comment → rest of the line
    if (c === '/' && line[i + 1] === '/') { push(line.slice(i), 'comment'); break; }

    // block-comment fragment on a single line (best-effort)
    if (c === '/' && line[i + 1] === '*') {
      const end = line.indexOf('*/', i + 2);
      const stop = end === -1 ? n : end + 2;
      push(line.slice(i, stop), 'comment');
      i = stop; continue;
    }

    // string
    if (c === '"') {
      let j = i + 1;
      while (j < n && line[j] !== '"') j++;
      push(line.slice(i, Math.min(j + 1, n)), 'string');
      i = j + 1; continue;
    }

    // whitespace
    if (/\s/.test(c)) {
      let j = i; while (j < n && /\s/.test(line[j])) j++;
      push(line.slice(i, j), 'plain');
      i = j; continue;
    }

    // number (incl. ranges read as number.number — fine for a mock)
    if (/[0-9]/.test(c)) {
      let j = i; while (j < n && /[0-9.]/.test(line[j])) j++;
      push(line.slice(i, j), 'number');
      i = j; continue;
    }

    // word: keyword / island / xform / note / ident
    if (/[a-zA-Z_$]/.test(c)) {
      let j = i; while (j < n && /[a-zA-Z0-9_$]/.test(line[j])) j++;
      const word = line.slice(i, j);
      const bare = word.startsWith('$') ? word.slice(1) : word;
      let kind: TokenKind = 'ident';
      if (word.startsWith('$'))          kind = 'note';        // splice var → island colour
      else if (KEYWORDS.has(bare))       kind = 'keyword';
      else if (ISLANDS.has(bare))        kind = 'island';
      else if (XFORMS.has(bare))         kind = 'fn';
      else if (NOTE_RE.test(bare))       kind = 'note';
      push(word, kind);
      i = j; continue;
    }

    // operators / mini-notation sigils
    if ('&|*/!@:~_<>[]()'.includes(c)) { push(c, 'operator'); i++; continue; }
    if (',.='.includes(c))             { push(c, 'punct'); i++; continue; }

    push(c, 'plain');
    i++;
  }

  return out;
}
