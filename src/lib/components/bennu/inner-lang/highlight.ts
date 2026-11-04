/**
 * Colouring for the text of an injected language: `(value, language) → spans`, in value coordinates.
 *
 * Regex has its own parser (`../regex`). The rest reuse CodeMirror's legacy stream modes — the same
 * ones the editor uses for whole files of those languages — driven here by a small loop of ours,
 * because a stream mode is a tokenizer and the editor's own driver (`StreamLanguage`) is built to
 * own a whole document, not to colour a fragment of one.
 */

import { StringStream, type StreamParser } from '@codemirror/language';
import { css } from '@codemirror/legacy-modes/mode/css';
import { json } from '@codemirror/legacy-modes/mode/javascript';
import { lua } from '@codemirror/legacy-modes/mode/lua';
import { shell } from '@codemirror/legacy-modes/mode/shell';
import { html, xml } from '@codemirror/legacy-modes/mode/xml';
import { yaml } from '@codemirror/legacy-modes/mode/yaml';
import { javascriptStream } from '$lib/components/shared/ui/code-editor/js-mode';
import { sqlTokens } from '$lib/utils/sql-tokens';
import { highlightRegex } from '../regex/tokens';
import type { Flavour } from '../regex/types';
import type { InnerLang } from './detect';

export interface InnerSpan {
  from: number;
  to: number;
  /** A `cm-tok-<cls>` class. */
  cls: string;
  /** A hover message (on an `invalid` span). */
  message?: string;
}

/** Legacy token names → the theme's token classes (same table the editor's own embedded modes use). */
function tokenClass(type: string): string | null {
  switch (type.split(' ')[0]) {
    case 'keyword': return 'keyword';
    case 'number': case 'unit': return 'number';
    case 'comment': return 'comment';
    case 'string': case 'string-2': return 'string';
    case 'atom': return 'constant';
    case 'def': return 'declaration';
    case 'variable-3': case 'type': case 'builtin': case 'qualifier': return 'type';
    case 'variable': case 'variable-2': return 'ident';
    case 'property': case 'attribute': return 'field';
    case 'operator': return 'operator';
    case 'meta': return 'annotation';
    case 'tag': return 'keyword';
    case 'bracket': case 'punctuation': return 'punctuation';
    case 'callee': return 'function';
    case 'self': return 'self';
    case 'label': return 'label';
    default: return null;
  }
}

const STREAMS: Partial<Record<InnerLang, StreamParser<unknown>>> = {
  lua,
  css,
  json,
  yaml,
  shell,
  xml,
  html,
  javascript: javascriptStream as unknown as StreamParser<unknown>,
};

/** Run a stream mode over `text` line by line. Lines are what the modes are written for; carrying
 *  the state across them keeps multi-line comments and strings right. */
export function streamSpans(parser: StreamParser<unknown>, text: string): InnerSpan[] {
  const out: InnerSpan[] = [];
  let state: unknown;
  try { state = parser.startState ? parser.startState(2) : {}; } catch { return out; }
  let offset = 0;
  for (const line of text.split('\n')) {
    if (line.length === 0) {
      try { parser.blankLine?.(state, 2); } catch { /* a mode without blank-line handling */ }
    } else {
      const stream = new StringStream(line, 2, 2);
      let guard = 0;
      while (!stream.eol() && guard++ < 20_000) {
        const start = stream.pos;
        stream.start = start;
        let tok: string | null = null;
        try { tok = parser.token(stream, state); } catch { break; }
        if (stream.pos <= start) { stream.pos = start + 1; continue; }
        const cls = tok ? tokenClass(tok) : null;
        if (cls) out.push({ from: offset + start, to: offset + stream.pos, cls });
      }
    }
    offset += line.length + 1;
  }
  return out;
}

export interface InnerHighlightOptions {
  flavour: Flavour;
  /** Regex flag letters implied by the call. */
  flags: string;
}

/** Spans for `text` in language `lang`; empty for a language with no highlighter. */
export function highlightInner(lang: InnerLang, text: string, opts: InnerHighlightOptions): InnerSpan[] {
  if (lang === 'regex') return highlightRegex(text, opts.flavour, opts.flags);
  if (lang === 'sql') return sqlSpans(text);
  const parser = STREAMS[lang];
  return parser ? streamSpans(parser, text) : [];
}

const SQL_CLASS = { keyword: 'keyword', string: 'string', number: 'number', comment: 'comment', param: 'field' } as const;

function sqlSpans(text: string): InnerSpan[] {
  const out: InnerSpan[] = [];
  let at = 0;
  for (const t of sqlTokens(text)) {
    if (t.kind !== 'text') out.push({ from: at, to: at + t.text.length, cls: SQL_CLASS[t.kind] });
    at += t.text.length;
  }
  return out;
}
