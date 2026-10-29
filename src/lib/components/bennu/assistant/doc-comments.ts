/**
 * Where the assistant can help with documentation, found in the text of a buffer.
 *
 * Two kinds of place, one gesture. A **comment** (a Javadoc block, or a run of `///` lines) gets a small
 * button that offers to improve it, review it against the code under it, shorten it or translate it. A
 * **declaration with no comment** — a method, a class, a function — offers to write one, but only when the
 * caret is on it, like a light bulb: marking every undocumented method of a legacy file would turn the
 * margin into noise.
 *
 * The scan is textual and deliberately conservative. It runs on the live buffer, so it must be instant
 * and must work on code that does not compile yet; the price is that it recognises the common shapes
 * (modifier-led Java declarations, `fn`/`struct`/`enum`/`trait` in Rust) and ignores the exotic. A miss
 * costs a button that is not there; a false positive costs a button on the wrong line — so when in doubt it
 * says nothing.
 */

export type DocLanguage = 'java' | 'rust';

export interface DocTarget {
  /** 1-based line the button sits on: the comment's first line, or the declaration's. */
  line: number;
  kind: 'comment' | 'undocumented';
  /** For a comment: its last line. For a declaration: its own line. */
  endLine: number;
  /** 1-based line of the declaration the comment documents (or that needs one). */
  declLine: number | null;
  /** The declaration's text, trimmed — what the prompt names it by. */
  declaration: string;
}

/** The declaration on `line` (1-based), trimmed, when it is one a method, a type or a function. */
export function declarationOnLine(source: string, lang: DocLanguage, line: number): string | null {
  const text = source.split(/\r?\n/)[line - 1];
  return text !== undefined && isDeclaration(text, lang) ? text.trim() : null;
}

export function docLanguageOf(path: string | null | undefined): DocLanguage | null {
  if (!path) return null;
  if (/\.java$/i.test(path)) return 'java';
  if (/\.rs$/i.test(path)) return 'rust';
  return null;
}

const JAVA_DECL =
  /^\s*(?:(?:public|protected|private|static|final|abstract|synchronized|default|native|sealed|non-sealed)\s+)+(?:[\w<>\[\],.?& ]+?\s+)?(?:class|interface|enum|record|@interface|[\w$]+\s*\()/;
const JAVA_TYPE_DECL = /^\s*(?:(?:public|protected|private|static|final|abstract|sealed|non-sealed)\s+)*(?:class|interface|enum|record|@interface)\s+\w/;
const RUST_DECL = /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|unsafe|const|extern(?:\s+"[^"]*")?)\s+)*(?:fn|struct|enum|trait|mod|type)\s+\w/;

/** Statements that start with a modifier-looking word but are not declarations. */
const NOT_A_DECL = /^\s*(?:return|new|throw|else|if|for|while|switch|case|try|catch|do|import|package)\b/;

function isDeclaration(line: string, lang: DocLanguage): boolean {
  if (NOT_A_DECL.test(line)) return false;
  if (lang === 'java') return JAVA_TYPE_DECL.test(line) || JAVA_DECL.test(line);
  return RUST_DECL.test(line);
}

const isAnnotation = (line: string, lang: DocLanguage) =>
  lang === 'java' ? /^\s*@\w/.test(line) && !/^\s*@interface/.test(line) : /^\s*#\[/.test(line) || /^\s*#!\[/.test(line);

/** How many brackets a line leaves open: `(` and `[` count up, `)` and `]` down. Strings are not parsed. */
function openBrackets(line: string): number {
  let n = 0;
  for (const ch of line) {
    if (ch === '(' || ch === '[') n++;
    else if (ch === ')' || ch === ']') n--;
  }
  return n;
}

/**
 * The index of the first line of code after a comment, skipping blank lines and annotations — an
 * annotation that runs over several lines (`@RequestMapping(value = "x",\n method = GET)`) is skipped
 * whole. `null` when the comment is the last thing in the file.
 */
function codeAfter(lines: string[], from: number, lang: DocLanguage): number | null {
  for (let i = from; i < lines.length; i++) {
    const text = lines[i];
    if (text.trim() === '') continue;
    if (isAnnotation(text, lang)) {
      let depth = openBrackets(text);
      while (depth > 0 && i + 1 < lines.length) {
        i++;
        depth += openBrackets(lines[i]);
      }
      continue;
    }
    return i;
  }
  return null;
}

/** A first Javadoc that heads a file (a licence, a banner) documents the file, not a declaration. */
const FILE_HEADER = /^\s*(package|import)\b/;

/** The comment blocks of `lines`, as inclusive 0-based `[start, end]` index pairs. */
function commentBlocks(lines: string[], lang: DocLanguage): [number, number][] {
  const blocks: [number, number][] = [];
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    if (lang === 'java' && t.startsWith('/**')) {
      let end = i;
      while (end < lines.length && !lines[end].includes('*/')) end++;
      blocks.push([i, Math.min(end, lines.length - 1)]);
      i = end;
    } else if (lang === 'rust' && /^\/\/\/(?!\/)/.test(t)) {
      let end = i;
      while (end + 1 < lines.length && /^\/\/\/(?!\/)/.test(lines[end + 1].trim())) end++;
      blocks.push([i, end]);
      i = end;
    }
  }
  return blocks;
}

/** Whether the declaration at `index` has a doc comment right above it (annotations in between allowed). */
function isDocumented(lines: string[], index: number, lang: DocLanguage): boolean {
  for (let i = index - 1; i >= 0; i--) {
    const text = lines[i];
    if (text.trim() === '') return false;
    if (isAnnotation(text, lang)) continue;
    const t = text.trim();
    return lang === 'java' ? t.endsWith('*/') : t.startsWith('///');
  }
  return false;
}

/**
 * The places documentation help is offered in `source`. `caretLine` (1-based) decides which undocumented
 * declaration, if any, is offered one.
 */
export function docTargets(source: string, lang: DocLanguage, caretLine: number): DocTarget[] {
  const lines = source.split(/\r?\n/);
  const out: DocTarget[] = [];

  const commented = new Set<number>();
  for (const [start, end] of commentBlocks(lines, lang)) {
    const next = codeAfter(lines, end + 1, lang);
    // A doc comment is a doc comment whatever it sits on — a method, a field, a type, an annotated
    // class. The ones that are not ours are a banner before `package`/`import`, and one at the very end.
    if (next === null || FILE_HEADER.test(lines[next])) continue;
    commented.add(next);
    out.push({ line: start + 1, kind: 'comment', endLine: end + 1, declLine: next + 1, declaration: lines[next].trim() });
  }

  const here = caretLine - 1;
  if (here >= 0 && here < lines.length && isDeclaration(lines[here], lang) && !commented.has(here) && !isDocumented(lines, here, lang)) {
    out.push({ line: here + 1, kind: 'undocumented', endLine: here + 1, declLine: here + 1, declaration: lines[here].trim() });
  }
  return out;
}
