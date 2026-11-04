/**
 * Which string literals hold another language, and which.
 *
 * Two ways a literal is claimed:
 *
 * - **Said so** — IntelliJ's own conventions, so code already annotated for IntelliJ works here
 *   unchanged: a `// language=regex` line comment above the statement, a block comment of the same
 *   shape before the literal, or `@Language("html")` on the declaration. This is the only way for
 *   every language but regex.
 * - **Recognised** (regex only) — the literal is the pattern argument of `Pattern.compile`,
 *   `String.matches/replaceAll/replaceFirst/split`, `@Pattern(regexp = …)`, `new RegExp(…)`,
 *   `Regex::new(…)` and friends. Context decides; a *name* heuristic (`String emailRegex = "…"`) is
 *   a separate, opt-in level because it is a guess, not a fact.
 *
 * SQL is deliberately **not** claimed here: the JPA framework extension already finds, colours and
 * completes it (see `hql_sites.rs`), and claiming it twice would paint it twice.
 *
 * Pieces of a `"a" + "b"` concatenation form one site — a pattern built that way is one pattern.
 */

import type { Flavour } from '../regex/types';
import { scanLiterals, type Host, type Literal } from './literals';
import { literalUnits, type Unit } from './units';

export type InnerLang =
  | 'regex' | 'sql' | 'lua' | 'html' | 'javascript' | 'css' | 'json' | 'xml' | 'yaml' | 'shell';

/** How eagerly a literal is taken for a regex without being told. */
export type RegexDetection = 'off' | 'context' | 'names';

export const INNER_LANG_LABELS: Record<InnerLang, string> = {
  regex: 'Regular expression', sql: 'SQL', lua: 'Lua', html: 'HTML', javascript: 'JavaScript', css: 'CSS',
  json: 'JSON', xml: 'XML', yaml: 'YAML', shell: 'Shell',
};

/** Names a `language=` hint may use, lower-cased, mapped onto what is supported. */
const ALIASES: Record<string, InnerLang> = {
  regexp: 'regex', regex: 'regex', re: 'regex', pcre: 'regex',
  lua: 'lua',
  html: 'html', xhtml: 'html', html5: 'html',
  js: 'javascript', javascript: 'javascript', ecmascript: 'javascript', typescript: 'javascript', jsx: 'javascript',
  css: 'css', scss: 'css', less: 'css',
  json: 'json', json5: 'json',
  xml: 'xml',
  yaml: 'yaml', yml: 'yaml',
  sh: 'shell', shell: 'shell', bash: 'shell', zsh: 'shell',
};

/** The language a hint's name stands for, or null (JPQL and anything unknown). */
export function innerLangFromName(name: string): InnerLang | null {
  const word = name.trim().toLowerCase().match(/^[a-z0-9_]+/)?.[0];
  if (!word) return null;
  // `SQL`, `PostgreSQL`, `MySQL`, `Oracle`: any dialect is coloured by the same lexer. JPQL / HQL are not SQL.
  if (/sql$/.test(word) || word === 'oracle') return 'sql';
  return ALIASES[word] ?? null;
}

export interface InnerSite {
  lang: InnerLang;
  host: Host;
  /** The literals the value is made of, in order (more than one for a concatenation). */
  literals: Literal[];
  /** The value, unescaped, with the source span of every character. */
  units: Unit[];
  /** The value is only a fragment of the real string: something is concatenated on a side we cannot see. */
  partial: boolean;
  origin: 'hint' | 'context' | 'name' | 'manual';
  flavour: Flavour;
  /** Flag letters implied by the call (`Pattern.CASE_INSENSITIVE`, a `"gi"` second argument). */
  flags: string;
}

export interface DetectOptions {
  regexDetection: RegexDetection;
}

// ── Hints ─────────────────────────────────────────────────────────────────────────

interface Hint { lang: InnerLang | 'other'; end: number }

const LINE_HINT = /\/\/[ \t]*language[ \t]*[=:][ \t]*([^\r\n]*)/g;
const BLOCK_HINT = /\/\*[ \t]*language[ \t]*[=:][ \t]*([^*]*)\*\//g;
const ANNOTATION_HINT = /@(?:[\w.]+\.)?Language\s*\(\s*(?:value\s*=\s*)?"([^"]*)"[^)]*\)/g;

function lastHint(window: string, base: number): Hint | null {
  let best: Hint | null = null;
  for (const re of [LINE_HINT, BLOCK_HINT, ANNOTATION_HINT]) {
    re.lastIndex = 0;
    for (let m = re.exec(window); m; m = re.exec(window)) {
      const end = base + m.index + m[0].length;
      if (!best || end > best.end) best = { lang: innerLangFromName(m[1]) ?? 'other', end };
    }
  }
  return best;
}

/** Where the statement a literal belongs to begins: just past the last `;`, `{` or `}` before it. */
function statementStart(src: string, open: number): number {
  const lo = Math.max(0, open - 600);
  const w = src.slice(lo, open);
  const cut = Math.max(w.lastIndexOf(';'), w.lastIndexOf('{'), w.lastIndexOf('}'));
  return lo + (cut < 0 ? 0 : cut + 1);
}

function hintFor(src: string, lit: Literal, all: Literal[], index: number): InnerLang | 'other' | null {
  const from = statementStart(src, lit.open);
  const hint = lastHint(src.slice(from, lit.open), from);
  if (!hint) return null;
  // The hint speaks for the first literal after it only.
  for (let i = index - 1; i >= 0 && all[i].open >= from; i--) {
    if (all[i].open >= hint.end) return null;
  }
  return hint.lang;
}

// ── Context ───────────────────────────────────────────────────────────────────────

/** Source text before `pos` with whitespace and trailing comments removed, for looking back at what precedes a literal. */
function codeBefore(src: string, pos: number): string {
  let s = src.slice(Math.max(0, pos - 240), pos);
  for (;;) {
    const t = s.replace(/\/\*[\s\S]*?\*\/\s*$/, '').replace(/\/\/[^\n]*\n\s*$/, '').trimEnd();
    if (t === s.trimEnd()) return t;
    s = t;
  }
}

const JAVA_PATTERN_CALLS = new Set([
  'matches', 'replaceAll', 'replaceFirst', 'split', 'splitAsStream', 'useDelimiter', 'findInLine',
  'findWithinHorizon', 'skip', 'containsPattern', 'doesNotContainPattern', 'matchesPattern',
]);
const REGEX_META = /[\\[\](){}|*+?.^$]/;

/** What precedes `(`: the callee's receiver (may be empty) and name. */
function calleeOf(head: string): { receiver: string; name: string } | null {
  const m = /([\w$.]*?)\.?([A-Za-z_$][\w$]*)\s*$/.exec(head);
  if (!m) return null;
  return { receiver: m[1], name: m[2] };
}

function javaContext(src: string, lit: Literal, text: string): boolean {
  const head = codeBefore(src, lit.open);
  if (/\bregexp?\s*=$/.test(head)) return true;
  if (!head.endsWith('(')) return false;
  const callee = calleeOf(head.slice(0, -1));
  if (!callee) return false;
  const { receiver, name } = callee;
  const recvTail = receiver.split('.').pop() ?? '';
  if (name === 'compile') return recvTail === 'Pattern';
  if (name === 'matches' && recvTail === 'Pattern') return true;
  if (!JAVA_PATTERN_CALLS.has(name)) return false;
  // `matcher.replaceAll("$1")` takes a replacement, and Guava's `splitter.split("a.b")` an input.
  if ((name === 'replaceAll' || name === 'replaceFirst') && /matcher$/i.test(recvTail)) return false;
  if ((name === 'split' || name === 'splitAsStream') && (/splitter/i.test(receiver) || !REGEX_META.test(text))) return false;
  return true;
}

function jsContext(src: string, lit: Literal): boolean {
  const head = codeBefore(src, lit.open);
  if (!head.endsWith('(')) return false;
  const callee = calleeOf(head.slice(0, -1));
  return !!callee && callee.name === 'RegExp';
}

function rustContext(src: string, lit: Literal): boolean {
  const head = codeBefore(src, lit.open);
  return /\b(?:Regex|RegexBuilder|RegexSetBuilder)::new\s*\($/.test(head) || /\bregex!\s*\($/.test(head);
}

const DATE_LIKE = /^[yMdHhmsSaZzEGuXwTk\s\-:./,'+]+$/;

function nameContext(src: string, lit: Literal, text: string): boolean {
  const head = codeBefore(src, lit.open);
  const m = /([A-Za-z_$][\w$]*)\s*(?::[^=;]*)?=$/.exec(head);
  if (!m || head.endsWith('==')) return false;
  const name = m[1];
  const lower = name.toLowerCase();
  if (/regexp?/.test(lower) || /^re_|_re$|_re_/.test(lower) || /[a-z]Re$/.test(name)) return true;
  return lower.endsWith('pattern') && !DATE_LIKE.test(text);
}

// ── Call flags ────────────────────────────────────────────────────────────────────

const JAVA_FLAGS: Record<string, string> = {
  CASE_INSENSITIVE: 'i', MULTILINE: 'm', DOTALL: 's', COMMENTS: 'x', UNICODE_CASE: 'u',
  UNICODE_CHARACTER_CLASS: 'U', UNIX_LINES: 'd',
};

/** The flag letters the call after the pattern asks for: `Pattern.compile(p, Pattern.CASE_INSENSITIVE | …)`,
 *  `new RegExp(p, "gi")`. Empty when the call has no second argument. */
function callFlags(src: string, afterClose: number, host: Host): string {
  const tail = src.slice(afterClose, afterClose + 240);
  const m = /^\s*,\s*([^)]*)\)/.exec(tail);
  if (!m) return '';
  if (host === 'java') {
    return [...m[1].matchAll(/\b(?:Pattern\.)?([A-Z_]+)\b/g)].map((x) => JAVA_FLAGS[x[1]] ?? '').join('');
  }
  if (host === 'js') {
    const str = /^\s*["'`]([a-z]*)["'`]\s*$/.exec(m[1]);
    return str ? str[1] : '';
  }
  return '';
}

// ── Sites ─────────────────────────────────────────────────────────────────────────

/** Whether only a `+` (and comments) lies between two literals. */
function concatenated(src: string, a: Literal, b: Literal): boolean {
  const between = src.slice(a.close, b.open).replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/[^\n]*/g, '');
  return between.trim() === '+';
}

const flavourOf = (host: Host): Flavour => (host === 'java' ? 'java' : host === 'js' ? 'js' : 'rust');

function makeSite(
  src: string, host: Host, chain: Literal[], lang: InnerLang, origin: InnerSite['origin'],
): InnerSite {
  const units = chain.flatMap((l) => literalUnits(src, l, host));
  const first = chain[0];
  const last = chain[chain.length - 1];
  const before = codeBefore(src, first.open);
  const after = src.slice(last.close, last.close + 40).replace(/^\s+/, '');
  return {
    lang, host, literals: chain, units, origin, flavour: flavourOf(host),
    partial: before.endsWith('+') || after.startsWith('+'),
    flags: lang === 'regex' ? callFlags(src, last.close, host) : '',
  };
}

/** Every injected-language site of a file. */
export function findSites(src: string, host: Host, opts: DetectOptions): InnerSite[] {
  const lits = scanLiterals(src, host);
  const sites: InnerSite[] = [];
  for (let i = 0; i < lits.length; i++) {
    const lit = lits[i];
    const claim = claimOf(src, host, lits, i, opts);
    if (!claim) continue;
    const chain = [lit];
    while (i + 1 < lits.length && concatenated(src, lits[i], lits[i + 1])) { chain.push(lits[i + 1]); i++; }
    sites.push(makeSite(src, host, chain, claim.lang, claim.origin));
  }
  return sites;
}

function claimOf(
  src: string, host: Host, lits: Literal[], index: number, opts: DetectOptions,
): { lang: InnerLang; origin: InnerSite['origin'] } | null {
  const lit = lits[index];
  const hint = hintFor(src, lit, lits, index);
  if (hint === 'other') return null; // told it is something this module does not own (SQL, …)
  if (hint) return { lang: hint, origin: 'hint' };
  if (opts.regexDetection === 'off') return null;
  const text = src.slice(lit.start, lit.end);
  const byContext = host === 'java' ? javaContext(src, lit, text) : host === 'js' ? jsContext(src, lit) : rustContext(src, lit);
  if (byContext) return { lang: 'regex', origin: 'context' };
  if (opts.regexDetection === 'names' && nameContext(src, lit, text)) return { lang: 'regex', origin: 'name' };
  return null;
}

/**
 * The site under a selection, for "open in the Regex Tester". A recognised site wins; failing that,
 * any string literal the caret is in is offered as a regex anyway — the command is an explicit ask,
 * so it does not need the detection that the passive colouring does.
 */
export function regexSiteAt(src: string, host: Host, from: number, to: number, opts: DetectOptions): InnerSite | null {
  const touches = (l: Literal) => from >= l.open && to <= l.close;
  const sites = findSites(src, host, opts).filter((s) => s.lang === 'regex');
  const found = sites.find((s) => s.literals.some(touches));
  if (found) return found;
  const lit = scanLiterals(src, host).find(touches);
  return lit ? makeSite(src, host, [lit], 'regex', 'manual') : null;
}
