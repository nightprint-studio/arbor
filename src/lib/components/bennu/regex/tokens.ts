/**
 * Colouring for a regex: the AST turned into `cm-tok-*` spans.
 *
 * Every class here already has a rule in the editor theme, so a pattern takes whatever palette the
 * active theme gives code: group delimiters read as keywords, quantifiers and `|` as operators,
 * predefined classes as types, anchors as constants, back-references and group names as fields,
 * escapes in the theme's escape colour. A parse problem becomes an `invalid` span that carries the
 * message, so the same spans drive both the colour and the squiggle.
 */

import { parseRegex } from './parse';
import type { Flavour, ParsedRegex, RegexNode, Span } from './types';

export interface RegexSpan extends Span {
  cls: string;
  /** Present on an `invalid` span: what is wrong. */
  message?: string;
}

/** The spans for an already parsed pattern, ascending by start (nested ones after their parent). */
export function regexSpans(parsed: ParsedRegex): RegexSpan[] {
  const out: RegexSpan[] = [];
  const add = (from: number, to: number, cls: string) => { if (to > from) out.push({ from, to, cls }); };
  visit(parsed.root, add);
  for (const e of parsed.errors) out.push({ from: e.from, to: e.to, cls: 'invalid', message: e.message });
  return out.sort((a, b) => a.from - b.from || b.to - a.to);
}

/** Parse and colour in one go. */
export function highlightRegex(pattern: string, flavour: Flavour, flags = ''): RegexSpan[] {
  return regexSpans(parseRegex(pattern, flavour, flags));
}

function visit(node: RegexNode, add: (from: number, to: number, cls: string) => void): void {
  switch (node.t) {
    case 'lit':
      // A plain character stays the string colour; an escape (`\n`, `\.`, `\x41`) stands out.
      if (node.raw.startsWith('\\')) add(node.from, node.to, 'escape');
      break;
    case 'quote':
      add(node.from, Math.min(node.from + 2, node.to), 'escape');
      if (node.to - node.from >= 4 && node.to >= node.from + 2) add(node.to - 2, node.to, 'escape');
      break;
    case 'any': add(node.from, node.to, 'type'); break;
    case 'short': case 'prop': case 'linebreak': add(node.from, node.to, 'type'); break;
    case 'anchor': add(node.from, node.to, 'constant'); break;
    case 'backref': add(node.from, node.to, 'field'); break;
    case 'flags': add(node.from, node.to, 'annotation'); break;
    case 'comment': add(node.from, node.to, 'comment'); break;
    case 'class':
      add(node.from, node.from + (node.negate ? 2 : 1), 'type');
      visitClassItems(node, add);
      if (node.to > node.from && sliceEndsWithBracket(node)) add(node.to - 1, node.to, 'type');
      break;
    case 'group': {
      add(node.open.from, node.open.to, 'keyword');
      if (node.kind === 'named' && node.name) {
        // The name sits between `(?<` (or `(?P<`) and `>`.
        const nameFrom = node.open.to - 1 - node.name.length;
        add(nameFrom, nameFrom + node.name.length, 'field');
      }
      visit(node.body, add);
      if (node.close) add(node.close.from, node.close.to, 'keyword');
      break;
    }
    case 'quant':
      visit(node.body, add);
      add(node.token.from, node.token.to, 'operator');
      break;
    case 'alt':
      node.branches.forEach((b) => visit(b, add));
      node.pipes.forEach((p) => add(p, p + 1, 'operator'));
      break;
    case 'seq':
      node.items.forEach((b) => visit(b, add));
      break;
  }
}

/** A class is closed iff the parser stopped just past a `]`; an unclosed one runs to the end. */
function sliceEndsWithBracket(node: Extract<RegexNode, { t: 'class' }>): boolean {
  const last = node.operands.flat().at(-1);
  return !last || last.to < node.to;
}

function visitClassItems(
  node: Extract<RegexNode, { t: 'class' }>,
  add: (from: number, to: number, cls: string) => void,
): void {
  for (const operand of node.operands) {
    for (const it of operand) {
      switch (it.k) {
        case 'short': case 'prop': add(it.from, it.to, 'type'); break;
        case 'range': add(it.from, it.to, 'constant'); break;
        case 'nested': add(it.from, it.from + (it.cls.negate ? 2 : 1), 'type'); visitClassItems(it.cls, add); add(it.to - 1, it.to, 'type'); break;
        case 'char': if (it.to - it.from > 1 && it.cp < 0x10000 && it.to - it.from !== String.fromCodePoint(it.cp).length) add(it.from, it.to, 'escape'); break;
      }
    }
  }
}
