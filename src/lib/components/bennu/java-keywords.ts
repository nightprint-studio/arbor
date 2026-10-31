/**
 * Which Java keywords make sense **where the caret is**, in the order a person would reach for them.
 *
 * The editor offers the language's keywords as a fallback because no index is needed to know them. Offering all of
 * them, alphabetically, is how `re` in a method body came to open on `record`, `requires`, `return` — module words
 * and a local-type keyword ahead of the one statement anybody meant. The position decides:
 *
 *  - **top level** — `package`, `import`, modifiers, type declarations (`module` only in `module-info.java`);
 *  - **class body** — member modifiers, `void`, primitives, type declarations;
 *  - **statement start** in a method, constructor, lambda or initializer — the statement words, `return` first in a
 *    method that returns a value and after `if`/`for`/`while` in one that does not; `else`/`catch`/`finally` only
 *    right after the block they belong to;
 *  - **inside an expression** (after `=`, `(`, `,`, an operator) — only the words an expression can start with.
 *
 * Pure text in, labels out: a brace/paren scan that skips strings, chars and comments, so it runs on every keystroke
 * without a parser. `null` means "cannot tell": the caller keeps its old flat list.
 */

type Kind = 'class' | 'method' | 'lambda' | 'control' | 'init' | 'expr';

interface Frame {
  kind: Kind;
  /** For a `method`: whether it returns a value. A constructor, an initializer and `void` do not. */
  returns?: boolean;
  /** The first word of the header: what a closing `}` leaves behind (`if`, `try`, ...). */
  head: string;
  /** The parenthesis depth outside this block: a lambda or an anonymous class lives inside a call's parentheses. */
  parensOutside?: number;
}

export interface Position {
  where: 'top' | 'class' | 'statement' | 'expression';
  /** Whether the nearest enclosing method/lambda returns a value (lambdas count as yes). */
  returnsValue: boolean;
  /** The block that just closed right before the caret, when the statement starts after its `}`. */
  justClosed: string | null;
  /** Whether a `switch` encloses the caret (where `yield` is legal). */
  inSwitch: boolean;
  /** Whether the declaration/statement already has text before the caret (`public st|`, `final re|`). */
  midDeclaration: boolean;
}

const CONTROL = new Set(['if', 'for', 'while', 'switch', 'catch', 'synchronized', 'try']);
const MODIFIERS = new Set([
  'public', 'private', 'protected', 'static', 'final', 'abstract', 'sealed', 'strictfp', 'default', 'native', 'transient', 'volatile', 'synchronized',
]);
const TYPE_WORDS = ['class', 'interface', 'enum', 'record', '@interface'];

/** The text with comments, strings and chars blanked (same length), so a scan sees only code. */
function blank(text: string): string {
  const out = text.split('');
  let i = 0;
  const wipe = (from: number, to: number) => {
    for (let k = from; k < to && k < out.length; k++) if (out[k] !== '\n') out[k] = ' ';
  };
  while (i < text.length) {
    const c = text[i];
    const d = text[i + 1];
    if (c === '/' && d === '/') {
      const end = text.indexOf('\n', i);
      const to = end < 0 ? text.length : end;
      wipe(i, to);
      i = to;
    } else if (c === '/' && d === '*') {
      const end = text.indexOf('*/', i + 2);
      const to = end < 0 ? text.length : end + 2;
      wipe(i, to);
      i = to;
    } else if (c === '"' && text.startsWith('"""', i)) {
      const end = text.indexOf('"""', i + 3);
      const to = end < 0 ? text.length : end + 3;
      wipe(i, to);
      i = to;
    } else if (c === '"' || c === "'") {
      let j = i + 1;
      while (j < text.length && text[j] !== c && text[j] !== '\n') j += text[j] === '\\' ? 2 : 1;
      wipe(i, j + 1);
      i = j + 1;
    } else {
      i++;
    }
  }
  return out.join('');
}

const stripAnnotationsAndModifiers = (header: string): string => {
  let h = header.trim();
  for (;;) {
    const ann = /^@(?!interface\b)[\w.]+\s*(\([^)]*\))?\s*/.exec(h);
    if (ann) {
      h = h.slice(ann[0].length);
      continue;
    }
    const mod = /^(public|private|protected|static|final|abstract|sealed|non-sealed|strictfp|default|native|transient|volatile|synchronized)\b\s*/.exec(h);
    if (mod) {
      h = h.slice(mod[0].length);
      continue;
    }
    return h;
  }
};

/** What a `{` opens, from the header since the last `;`, `{` or `}`. */
function frameFor(rawHeader: string): Frame {
  const header = rawHeader.trim();
  const head = /^[\w@]+/.exec(header)?.[0] ?? '';
  const stripped = stripAnnotationsAndModifiers(header);
  if (TYPE_WORDS.some((w) => new RegExp(`^${w.replace('@', '@')}\\b`).test(stripped))) return { kind: 'class', head: 'class' };
  if (/->\s*$/.test(header)) return { kind: 'lambda', head };
  if (/=\s*$/.test(header) || /[,(]\s*$/.test(header)) return { kind: 'expr', head };
  // `new Foo(...) {` and `new Foo<>() {`: an anonymous class body
  if (/\bnew\s+[\w.<>?,\s]+\([^()]*(\([^()]*\)[^()]*)*\)\s*$/.test(header)) return { kind: 'class', head: 'new' };
  const closeParen = header.lastIndexOf(')');
  if (closeParen >= 0 && /^\s*(throws\s+[\w.,\s<>]+)?\s*$/.test(header.slice(closeParen + 1))) {
    // find the `(` matching that `)`
    let depth = 0;
    let open = -1;
    for (let i = closeParen; i >= 0; i--) {
      if (header[i] === ')') depth++;
      else if (header[i] === '(' && --depth === 0) {
        open = i;
        break;
      }
    }
    if (open >= 0) {
      const before = header.slice(0, open).trim();
      const name = /([\w$]+)$/.exec(before)?.[1] ?? '';
      if (CONTROL.has(name)) return { kind: 'control', head: name };
      const beforeName = stripAnnotationsAndModifiers(before.slice(0, before.length - name.length)).replace(/^<[^>]*>\s*/, '').trim();
      // `void f()` and a constructor (nothing before the name) return nothing
      const returns = beforeName !== '' && !/^void$/.test(beforeName);
      return { kind: 'method', returns, head: name };
    }
  }
  // `else {`, `try {`, `do {`, `finally {`, `static {`, a bare initializer `{`
  return { kind: header === '' || /^(static)$/.test(header) ? 'init' : 'control', head };
}

/** Where the caret is, or `null` when the text cannot say (an unbalanced mess). */
export function positionAt(before: string): Position | null {
  const code = blank(before);
  const stack: Frame[] = [];
  let headerStart = 0;
  let parens = 0;
  let lastClosed: Frame | null = null;
  let sinceClose = true;
  for (let i = 0; i < code.length; i++) {
    const c = code[i];
    if (c === '(') parens++;
    else if (c === ')') parens = Math.max(0, parens - 1);
    else if (c === '{') {
      // a block opened inside parentheses (a lambda body, an anonymous class) starts counting parentheses afresh
      stack.push({ ...frameFor(code.slice(headerStart, i)), parensOutside: parens });
      parens = 0;
      headerStart = i + 1;
      lastClosed = null;
    } else if (c === '}') {
      if (stack.length === 0) return null;
      lastClosed = stack.pop() ?? null;
      parens = lastClosed?.parensOutside ?? 0;
      // inside the parentheses it closed in, the rest of the call is still one statement
      headerStart = parens > 0 ? headerStart : i + 1;
    } else if (parens === 0 && c === ';') {
      headerStart = i + 1;
      lastClosed = null;
    }
    if (!/\s/.test(c) && c !== '}') sinceClose = false;
  }
  void sinceClose;
  const header = code.slice(headerStart).trim();
  const top = stack[stack.length - 1];
  const owner = [...stack].reverse().find((f) => f.kind === 'method' || f.kind === 'lambda' || f.kind === 'init');
  const returnsValue = owner ? (owner.kind === 'method' ? owner.returns === true : owner.kind === 'lambda') : false;
  const inSwitch = stack.some((f) => f.head === 'switch');
  // a header of only annotations and modifiers is still "at the start" of a declaration
  const onlyPrelude = stripAnnotationsAndModifiers(header) === '';
  const midDeclaration = header !== '' && !onlyPrelude;
  const lastWordHeader = header === '' ? '' : header;
  void lastWordHeader;
  const base = { returnsValue, justClosed: header === '' && lastClosed ? lastClosed.head : null, inSwitch, midDeclaration };
  if (!top) return { where: 'top', ...base };
  if (top.kind === 'class') return { where: 'class', ...base };
  if (top.kind === 'expr') return null;
  // inside a body: at a statement start, or inside an expression that began earlier
  const statementStart = header === '' || onlyPrelude || /^(final|var)\s*$/.test(header);
  return { where: statementStart ? 'statement' : 'expression', ...base };
}

const PRIMITIVE_WORDS = ['int', 'long', 'boolean', 'double', 'float', 'char', 'byte', 'short'];

const STATEMENT_VALUE = ['return', 'if', 'for', 'while', 'try', 'throw', 'switch', 'var', 'final', 'new', 'this', 'super', 'do', 'break', 'continue', 'synchronized', 'assert'];
const STATEMENT_VOID = ['if', 'for', 'while', 'return', 'try', 'throw', 'switch', 'var', 'final', 'new', 'this', 'super', 'do', 'break', 'continue', 'synchronized', 'assert'];
const LITERALS = ['true', 'false', 'null'];
const LOCAL_TYPES = ['class', 'record', 'interface', 'enum'];
const EXPRESSION_WORDS = ['new', 'this', 'super', 'null', 'true', 'false', 'switch', 'instanceof'];
const MEMBER_WORDS = ['public', 'private', 'protected', 'static', 'final', 'abstract', 'void', ...PRIMITIVE_WORDS, 'default', 'synchronized', 'native', 'transient', 'volatile', 'sealed', 'strictfp', 'class', 'interface', 'enum', 'record'];
const TOP_WORDS = ['package', 'import', 'public', 'final', 'abstract', 'sealed', 'strictfp', 'class', 'interface', 'enum', 'record'];
const MODULE_WORDS = ['module', 'open', 'requires', 'exports', 'opens', 'uses', 'provides', 'transitive', 'to', 'with'];

/** Words after which what follows is an expression or a statement, never the name of something being declared. */
const BEFORE_AN_EXPRESSION = new Set([
  'return', 'throw', 'new', 'case', 'default', 'else', 'do', 'assert', 'yield', 'instanceof', 'try', 'finally', 'extends',
  'implements', 'throws', 'static', 'final', 'public', 'private', 'protected', 'abstract', 'synchronized', 'native',
  'transient', 'volatile', 'strictfp', 'sealed', 'permits', 'class', 'interface', 'enum', 'record', 'import', 'package',
]);

/**
 * Whether the word being typed is the NAME of a declaration: the statement so far is a type and a space (`val `, `String `,
 * `Map<K, V> `) or a type and a name (`val x `). A keyword there is never what is wanted — `null` after `val` is not Java —
 * so the popup is left to the names the backend offers.
 */
export function namesADeclaration(before: string): boolean {
  const statement = before.slice(Math.max(before.lastIndexOf(';'), before.lastIndexOf('{'), before.lastIndexOf('}')) + 1);
  const m = /^\s*(?:final\s+)?([A-Za-z_$][\w$.]*(?:<[^<>;=()]*(?:<[^<>;=()]*>[^<>;=()]*)*>)?(?:\[\])*)\s+(?:[A-Za-z_$][\w$]*\s+)?$/.exec(statement);
  if (!m) return false;
  const first = m[1].split(/[<[]/)[0];
  return !BEFORE_AN_EXPRESSION.has(first) && /^[A-Za-z_$]/.test(first);
}

/**
 * The keywords to offer for `prefix` at the end of `before`, best first; `null` when the position cannot be told and
 * the caller should keep its own list. `moduleInfo` allows the module words (only `module-info.java` has them).
 */
export function javaKeywordsAt(before: string, prefix: string, options: { moduleInfo?: boolean } = {}): string[] | null {
  const at = positionAt(before);
  if (!at) return null;
  if (namesADeclaration(before)) return [];
  const p = prefix.toLowerCase();
  const matching = (words: string[]) => [...new Set(words)].filter((w) => !p || w.startsWith(p));
  switch (at.where) {
    case 'top':
      return matching(options.moduleInfo ? [...TOP_WORDS, ...MODULE_WORDS] : TOP_WORDS);
    case 'class':
      return matching(MEMBER_WORDS);
    case 'expression':
      return matching(EXPRESSION_WORDS);
    case 'statement': {
      const afterBlock: string[] = [];
      if (at.justClosed === 'if' || at.justClosed === 'else') afterBlock.push('else');
      if (at.justClosed === 'try' || at.justClosed === 'catch') afterBlock.push('catch', 'finally');
      const words = [
        ...afterBlock,
        ...(at.returnsValue ? STATEMENT_VALUE : STATEMENT_VOID),
        ...(at.inSwitch ? ['yield'] : []),
        ...PRIMITIVE_WORDS,
        ...LITERALS,
        ...LOCAL_TYPES,
      ];
      return matching(words);
    }
  }
}
