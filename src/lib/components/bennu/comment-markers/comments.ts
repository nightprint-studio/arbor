/**
 * Where the comments of a file are.
 *
 * Java, JavaScript/TypeScript and Rust go through the literal scanner of the injected languages (one
 * scanner that knows strings, char literals and regex literals, so a `//` inside a string is not a
 * comment). The other languages the editor opens use a small quote-aware scanner driven by the comment
 * syntax their extension implies.
 */

import { hostForPath, scanComments } from '../inner-lang/literals';

export interface Span { from: number; to: number }

interface Syntax {
  /** Line-comment openers, longest first. */
  line: string[];
  block: [open: string, close: string][];
  /** Quote characters that open a string a comment opener inside must not escape from. */
  quotes: string;
}

const C_STYLE: Syntax = { line: ['//'], block: [['/*', '*/']], quotes: '"\'' };
const HASH: Syntax = { line: ['#'], block: [], quotes: '"\'' };
/** Patina: `//` and `///` only (its lexer rejects block comments), and a `'` is a loop label, never a quote. */
const PATINA: Syntax = { line: ['//'], block: [], quotes: '"' };
const MARKUP: Syntax = { line: [], block: [['<!--', '-->']], quotes: '' };

const BY_EXTENSION: Record<string, Syntax> = {
  c: C_STYLE, h: C_STYLE, cpp: C_STYLE, hpp: C_STYLE, cs: C_STYLE, go: C_STYLE, kt: C_STYLE, kts: C_STYLE,
  groovy: C_STYLE, gradle: C_STYLE, scala: C_STYLE, swift: C_STYLE, php: C_STYLE, json5: C_STYLE,
  css: { line: [], block: [['/*', '*/']], quotes: '"\'' },
  scss: C_STYLE, less: C_STYLE,
  sql: { line: ['--'], block: [['/*', '*/']], quotes: '\'' },
  lua: { line: ['--'], block: [['--[[', ']]']], quotes: '"\'' },
  ptn: PATINA,
  yaml: HASH, yml: HASH, toml: HASH, sh: HASH, bash: HASH, zsh: HASH, py: HASH, rb: HASH, conf: HASH,
  ini: { line: ['#', ';'], block: [], quotes: '' },
  properties: { line: ['#', '!'], block: [], quotes: '' },
  xml: MARKUP, html: MARKUP, htm: MARKUP, xhtml: MARKUP, jsp: { line: [], block: [['<!--', '-->'], ['<%--', '--%>']], quotes: '' },
  jspx: MARKUP, xsd: MARKUP, xsl: MARKUP, svg: MARKUP, pom: MARKUP, fxml: MARKUP,
  bat: { line: ['::', 'REM '], block: [], quotes: '' },
  cmd: { line: ['::', 'REM '], block: [], quotes: '' },
};

function extensionOf(path: string): string {
  const name = path.slice(Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')) + 1);
  const dot = name.lastIndexOf('.');
  return dot < 0 ? name.toLowerCase() : name.slice(dot + 1).toLowerCase();
}

/** Whether the language of `path` has comments this module can find. */
export function knowsComments(path: string): boolean {
  return hostForPath(path) !== null || Object.hasOwn(BY_EXTENSION, extensionOf(path));
}

/** The comment spans of `src` (sorted, non-overlapping), or none when the file's language is not known. */
export function commentSpans(src: string, path: string | null): Span[] {
  if (!path) return [];
  const host = hostForPath(path);
  if (host) return scanComments(src, host);
  const ext = extensionOf(path);
  return Object.hasOwn(BY_EXTENSION, ext) ? scanGeneric(src, BY_EXTENSION[ext]) : [];
}

function scanGeneric(src: string, syntax: Syntax): Span[] {
  const out: Span[] = [];
  const n = src.length;
  let i = 0;
  while (i < n) {
    const c = src[i];
    const block = syntax.block.find(([open]) => src.startsWith(open, i));
    if (block) {
      const e = src.indexOf(block[1], i + block[0].length);
      const end = e < 0 ? n : e + block[1].length;
      out.push({ from: i, to: end });
      i = end;
      continue;
    }
    if (syntax.line.some((open) => src.startsWith(open, i))) {
      let e = src.indexOf('\n', i);
      if (e < 0) e = n;
      out.push({ from: i, to: e });
      i = e;
      continue;
    }
    if (syntax.quotes.includes(c)) {
      // A string ends at its closing quote or at the end of its line; a lone apostrophe in prose must not run on.
      let j = i + 1;
      while (j < n && src[j] !== c && src[j] !== '\n') j += src[j] === '\\' ? 2 : 1;
      i = src[j] === c ? j + 1 : i + 1;
      continue;
    }
    i++;
  }
  return out;
}
