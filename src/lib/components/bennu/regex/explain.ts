/**
 * A plain-English, token-by-token reading of a pattern — the "explanation" pane of the Regex Tester.
 *
 * The AST is flattened into rows (`depth` gives the nesting) each pairing a slice of the pattern with
 * what it means. Runs of ordinary characters are merged into one row ("the text `abc`"), so a
 * pattern that is mostly literal reads as a sentence rather than as one row per letter.
 */

import { parseRegex } from './parse';
import type { ClassItem, ClassKind, Flavour, ParsedRegex, RegexNode } from './types';

export interface ExplainRow {
  from: number;
  to: number;
  /** The pattern text the row covers. */
  token: string;
  text: string;
  depth: number;
}

const SHORT_NAMES: Record<ClassKind, string> = {
  d: 'a digit (0–9)',
  D: 'anything but a digit',
  w: 'a word character (letter, digit or underscore)',
  W: 'anything but a word character',
  s: 'a whitespace character',
  S: 'anything but whitespace',
  h: 'a horizontal whitespace character',
  H: 'anything but horizontal whitespace',
  v: 'a vertical whitespace character (line break)',
  V: 'anything but vertical whitespace',
};

const ANCHORS: Record<string, string> = {
  '^': 'the start of the input (or of a line, in multi-line mode)',
  '$': 'the end of the input (or of a line, in multi-line mode)',
  A: 'the very start of the input',
  Z: 'the end of the input, before an optional final line break',
  z: 'the very end of the input',
  b: 'a word boundary',
  B: 'a position that is not a word boundary',
  G: 'the end of the previous match',
};

const GROUPS: Record<string, string> = {
  capture: 'Capturing group',
  named: 'Named capturing group',
  noncapture: 'Non-capturing group',
  atomic: 'Atomic group — once matched, the engine never backtracks into it',
  lookahead: 'Positive look-ahead — what follows must match, without consuming it',
  neglookahead: 'Negative look-ahead — what follows must not match',
  lookbehind: 'Positive look-behind — what precedes must match',
  neglookbehind: 'Negative look-behind — what precedes must not match',
  flagged: 'Group with its own flags',
};

const FLAG_NAMES: Record<string, string> = {
  i: 'case-insensitive', m: 'multi-line', s: 'dot matches line breaks', x: 'free-spacing',
  u: 'Unicode case folding', U: 'Unicode classes', d: 'unix lines',
};

/** `ch` shown readably: control characters and spaces named, the rest quoted. */
export function describeChar(cp: number): string {
  switch (cp) {
    case 9: return 'a tab';
    case 10: return 'a line feed';
    case 13: return 'a carriage return';
    case 12: return 'a form feed';
    case 32: return 'a space';
    case 0: return 'the NUL character';
    case 7: return 'the bell character';
    case 27: return 'the escape character';
    default: break;
  }
  const ch = String.fromCodePoint(cp);
  if (cp < 32 || cp === 127) return `control character U+${cp.toString(16).toUpperCase().padStart(4, '0')}`;
  return `the character “${ch}”`;
}

function quantText(min: number, max: number): string {
  if (min === 0 && max === Infinity) return 'zero or more times';
  if (min === 1 && max === Infinity) return 'one or more times';
  if (min === 0 && max === 1) return 'optionally (zero or one time)';
  if (min === max) return `exactly ${min} time${min === 1 ? '' : 's'}`;
  if (max === Infinity) return `${min} or more times`;
  return `between ${min} and ${max} times`;
}

function describeItem(it: ClassItem): string {
  switch (it.k) {
    case 'char': return describeChar(it.cp).replace(/^the character /, '');
    case 'range': return `${String.fromCodePoint(it.lo)} to ${String.fromCodePoint(it.hi)}`;
    case 'short': return SHORT_NAMES[it.kind];
    case 'prop': return `${it.negate ? 'not ' : ''}the Unicode property ${it.name}`;
    case 'nested': return `a nested class`;
  }
}

/** The rows for a pattern. */
export function explainRegex(pattern: string, flavour: Flavour, flags = ''): ExplainRow[] {
  return explainParsed(pattern, parseRegex(pattern, flavour, flags));
}

export function explainParsed(pattern: string, parsed: ParsedRegex): ExplainRow[] {
  const rows: ExplainRow[] = [];
  const row = (n: { from: number; to: number }, text: string, depth: number) =>
    rows.push({ from: n.from, to: n.to, token: pattern.slice(n.from, n.to), text, depth });

  const items = (node: RegexNode): RegexNode[] => (node.t === 'seq' ? node.items : [node]);

  const emitSeq = (list: RegexNode[], depth: number) => {
    for (let i = 0; i < list.length; i++) {
      const n = list[i];
      if (n.t === 'lit' && !n.raw.startsWith('\\')) {
        // Merge a run of plain characters into one row.
        let j = i;
        let text = '';
        while (j < list.length) {
          const m = list[j];
          if (m.t !== 'lit' || m.raw.startsWith('\\')) break;
          text += m.raw;
          j++;
        }
        // A quantifier binds to the last character only, and the parser already wraps that one in a
        // `quant` node, so the run ends before it and no character is described twice.
        row({ from: n.from, to: (list[j - 1] as { to: number }).to }, `Matches the text “${text}” literally`, depth);
        i = j - 1;
        continue;
      }
      emit(n, depth);
    }
  };

  const emit = (n: RegexNode, depth: number): void => {
    switch (n.t) {
      case 'seq': emitSeq(n.items, depth); break;
      case 'lit': row(n, `Matches ${describeChar(n.cp)}`, depth); break;
      case 'quote': row(n, `Matches the text “${n.text}” literally`, depth); break;
      case 'any': row(n, n.fl.s ? 'Matches any character, including line breaks' : 'Matches any character except a line break', depth); break;
      case 'short': row(n, `Matches ${SHORT_NAMES[n.kind]}`, depth); break;
      case 'prop': row(n, `Matches ${n.negate ? 'a character without' : 'a character with'} the Unicode property ${n.name}`, depth); break;
      case 'linebreak': row(n, 'Matches a line break (\\n, \\r, \\r\\n and the Unicode ones)', depth); break;
      case 'anchor': row(n, `Matches ${ANCHORS[n.kind]}`, depth); break;
      case 'backref':
        row(n, n.name !== null ? `Matches the same text as group “${n.name}”` : `Matches the same text as group ${n.index}`, depth);
        break;
      case 'flags': {
        const on = [...n.on].map((f) => FLAG_NAMES[f] ?? f).join(', ');
        const off = [...n.off].map((f) => FLAG_NAMES[f] ?? f).join(', ');
        row(n, `Sets flags from here on${on ? `: ${on}` : ''}${off ? `; clears ${off}` : ''}`, depth);
        break;
      }
      case 'comment': row(n, 'Comment — ignored', depth); break;
      case 'class': {
        const parts = n.operands.map((op) => op.map(describeItem).join(', '));
        const body = parts.length > 1 ? parts.map((p) => `(${p})`).join(' and also ') : parts[0];
        row(n, `Matches ${n.negate ? 'any character except' : 'one of'}: ${body || 'nothing'}`, depth);
        break;
      }
      case 'group': {
        const label = n.index !== null ? `${GROUPS[n.kind]} #${n.index}${n.name ? ` “${n.name}”` : ''}` : GROUPS[n.kind];
        const flags = n.kind === 'flagged'
          ? ` (on: ${[...(n.on ?? '')].map((f) => FLAG_NAMES[f] ?? f).join(', ') || 'none'}; off: ${[...(n.off ?? '')].map((f) => FLAG_NAMES[f] ?? f).join(', ') || 'none'})`
          : '';
        row({ from: n.open.from, to: n.open.to }, `${label}${flags}`, depth);
        emit(n.body, depth + 1);
        if (n.close) row(n.close, 'End of group', depth);
        break;
      }
      case 'quant': {
        const how = n.mode === 'lazy' ? ', as few as possible' : n.mode === 'possessive' ? ', as many as possible and never giving any back' : '';
        emit(n.body, depth);
        row(n.token, `Repeats the previous ${quantText(n.min, n.max)}${how}`, depth);
        break;
      }
      case 'alt':
        n.branches.forEach((b, i) => {
          if (i > 0) row({ from: n.pipes[i - 1], to: n.pipes[i - 1] + 1 }, 'Or', depth);
          emitSeq(items(b), depth);
        });
        break;
    }
  };

  // Rows come out grouped by structure; present them in pattern order.
  emit(parsed.root, 0);
  return rows.sort((a, b) => a.from - b.from || a.depth - b.depth);
}
