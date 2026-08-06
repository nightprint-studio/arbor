/**
 * A JavaScript tokenizer for **embedded** code — the `<script>` body of a JSP, and any other
 * region a host grammar hands over as raw text.
 *
 * ## Why not the legacy mode
 *
 * `@codemirror/legacy-modes`' `javascript` is a CM5 port, and the port kept CM5's vocabulary:
 * keywords, strings and comments, and then it stops. In the markup a legacy project is actually
 * full of —
 *
 * ```js
 * $("#container").dialog({ dialogClass: "no-close", width: larghezzaDialog - 100, show: {
 *   effect: "blind", duration: 500
 * }});
 * ```
 *
 * — the object **keys**, the **members**, the **calls** and the **numbers** all came out the same
 * flat colour as the locals, which is most of the line. It reads as one long expression and none
 * of the structure a reader is scanning for.
 *
 * So: a real tokenizer for the parts that carry meaning. Numbers in every form, template literals
 * with their `${…}` holes, regex told apart from division, object keys, member accesses, call
 * sites, declarations, and the globals a jQuery-era page leans on.
 *
 * ## Why a stream parser and not a parser
 *
 * The seam takes a `StreamParser` — the host grammar has already decided where the region is, and
 * colouring one needs no AST. It also has to survive **a fragment**: a JSP `<script>` block is
 * regularly not valid JavaScript on its own (a `<% %>` in the middle of it, an `if` whose closing
 * brace a scriptlet prints), and a tokenizer degrades on that where a parser gives up. Same
 * reasoning as `dtd-mode.ts`.
 *
 * ## The token names are the legacy vocabulary
 *
 * It returns CM5-style names (`keyword`, `def`, `property`, `variable-3`, …) so it drops into the
 * injection path in `highlight.ts` with no special case — plus `callee` and `self`, which that
 * file maps onto two classes the theme already had and the CM5 vocabulary never did.
 */

import type { StreamParser, StringStream } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';

const KEYWORDS = new Set([
  'break', 'case', 'catch', 'class', 'const', 'continue', 'debugger', 'default', 'delete', 'do',
  'else', 'export', 'extends', 'finally', 'for', 'function', 'if', 'import', 'in', 'instanceof',
  'let', 'new', 'of', 'return', 'static', 'switch', 'throw', 'try', 'typeof', 'var', 'void',
  'while', 'with', 'yield', 'async', 'await', 'get', 'set',
]);

/** The words that open a declaration, so the name after one is a definition. */
const DECLARERS = new Set(['var', 'let', 'const', 'function', 'class']);

/** Values that are their own literal. `undefined` is not one in the language and is one to a
 *  reader, which is the only thing a colour is for. */
const ATOMS = new Set(['true', 'false', 'null', 'undefined', 'NaN', 'Infinity']);

/** The names a page reaches for without declaring. Not a complete list of anything — it is the
 *  set worth telling apart from a local, and a browser page's is short and stable. */
const GLOBALS = new Set([
  'window', 'document', 'console', 'navigator', 'location', 'history', 'screen', 'localStorage',
  'sessionStorage', 'alert', 'confirm', 'prompt', 'setTimeout', 'setInterval', 'clearTimeout',
  'clearInterval', 'parseInt', 'parseFloat', 'isNaN', 'encodeURIComponent', 'decodeURIComponent',
  'encodeURI', 'decodeURI', 'require', 'module', 'exports', 'process', 'globalThis', 'arguments',
  // The one every JSP of this generation is built on.
  '$', 'jQuery',
]);

/** After one of these a `/` opens a **regex**; after a value it is division. Getting this wrong
 *  paints the rest of the line as a literal, which is the most visible way a JS highlighter can
 *  be broken. */
const REGEX_OK_AFTER = new Set(['none', 'operator', 'keyword', 'punct']);

// Written as escapes rather than literal characters: the upper bound is a non-character code
// point, and a source file is the wrong place to keep one where an encoding conversion can eat it.
const IDENT_START = /[A-Za-z_$\u00A1-\uFFFF]/;
const IDENT = /[A-Za-z0-9_$\u00A1-\uFFFF]/;

/** One level of nesting the tokenizer has to come back out of.
 *
 *  `` `a ${ cond ? `b` : c } d` `` is why it is a stack and not a flag, and `braces` is why an
 *  object literal written inside a hole does not close the hole. A quoted string is on the same
 *  stack because a JSP marker can interrupt one — see [`JSP_BLOCKS`]. */
interface Frame {
  kind: 'tmpl' | 'expr' | 'str';
  braces: number;
  /** For `str`: the quote that closes it. */
  quote?: string;
}

interface JsState {
  /** Inside an unterminated block comment. */
  inComment: boolean;
  /** The terminator of a JSP marker still open at the end of the previous line — a scriptlet
   *  spanning lines is the normal way one is written. */
  jspClose: string | null;
  frames: Frame[];
  /** What the last significant token was — decides regex-vs-division. */
  lastType: 'none' | 'value' | 'operator' | 'keyword' | 'punct';
  /** The last significant character — decides property access and object keys. */
  lastChar: string;
  /** A `var` / `let` / `const` / `function` / `class` is open: the next name is a declaration. */
  declaring: boolean;
  /** A `new` is open: the next name is a constructor. */
  constructing: boolean;
}

function startState(): JsState {
  return {
    inComment: false,
    jspClose: null,
    frames: [],
    lastType: 'none',
    lastChar: '',
    declaring: false,
    constructing: false,
  };
}

/** Record what was just consumed, so the next token can be read in its light. */
function mark(state: JsState, type: JsState['lastType'], ch: string): void {
  state.lastType = type;
  state.lastChar = ch;
}

/** The next non-space character after the stream's position, or `''`.
 *
 *  One character of lookahead is the whole of the context this tokenizer needs — `foo:` is a key,
 *  `foo(` is a call — and the reason it does not need to be a parser. */
function peekAhead(stream: StringStream): string {
  const line = stream.string;
  let i = stream.pos;
  while (i < line.length && /\s/.test(line[i])) i++;
  return line[i] ?? '';
}

/** Whether a name followed by `:` is an **object key** rather than the middle of something else.
 *
 *  A key sits right after a `{` or a `,`. The two other things that put a `:` after a name — a
 *  ternary's second arm and a `case`/label — do not, and telling them apart is worth the guard,
 *  because an object literal is most of what a page's JavaScript is made of. */
function isKeyPosition(prevChar: string): boolean {
  return prevChar === '{' || prevChar === ',';
}

// ── JSP markers ──────────────────────────────────────────────────────────────
//
// A `<script>` body in a JSP is not JavaScript. It is a **template** that produces JavaScript,
// and the page writes into it:
//
//   errore = "<wp:i18n key='LABEL_REQUIRED_COMUNE' />";
//   var rows = ${count};
//   <% if (admin) { %> showAdmin(); <% } %>
//
// Read as plain JavaScript, the first line is the interesting one: the tag's own `"` closes the
// string, and everything after it on the line is coloured as something it is not. That is not a
// cosmetic problem — a broken string is the failure mode that makes a whole file look wrong.
//
// So a marker is recognised **first, and inside strings too**, which is exactly the rule the JSP
// grammar already applies to attribute values (`quoted_value_double` interleaves nested
// constructs precisely so their quotes cannot close the outer value). It is also what the server
// does: substitution happens before there is any JavaScript to quote.
//
// The whole marker gets one colour — the JSP meta colour every scriptlet elsewhere in the page
// wears — rather than being tokenized inside. Inside a block of JavaScript, "this part is not
// JavaScript" is the useful thing to say, and saying it in the colour the rest of the page uses
// for the same idea is what makes it read at a glance.

/** `[opener, terminator]`, **longest opener first** — `<%--` has to be tried before `<%`. */
const JSP_BLOCKS: readonly (readonly [string, string])[] = [
  ['<%--', '--%>'],
  ['<%=', '%>'],
  ['<%@', '%>'],
  ['<%!', '%>'],
  ['<%', '%>'],
  ['${', '}'],
  ['#{', '}'],
  ['%{', '}'],
];

/** A whole namespaced taglib tag on one line: `<wp:i18n key="X" />`, `</s:iterator>`.
 *
 *  The `prefix:` is required, and that is what keeps this from firing on JavaScript. A bare
 *  `<div>` in a string is text nobody needs coloured, and `a < b` has no name after the `<` at
 *  all — so demanding a name, a colon and another name is both what the JSP grammar calls a tag
 *  name and the narrowest thing that cannot be mistaken for a comparison. */
const JSP_TAG = /^<\/?[A-Za-z][A-Za-z0-9.\-]*:[A-Za-z][A-Za-z0-9.\-]*[^>]*>/;

/** The same, unterminated — a tag whose attributes run onto the next line. */
const JSP_TAG_OPEN = /^<\/?[A-Za-z][A-Za-z0-9.\-]*:[A-Za-z]/;

/** Whether a JSP marker begins at the stream's current position. Look only — the caller ends its
 *  own token here so the marker gets one of its own. */
function jspStartsAt(stream: StringStream): boolean {
  const at = stream.pos;
  return (
    JSP_BLOCKS.some(([open]) => stream.string.startsWith(open, at)) ||
    JSP_TAG_OPEN.test(stream.string.slice(at))
  );
}

/** Consume up to and including `close`. `false` when it is not on this line — the caller then
 *  remembers the terminator and picks up on the next one. */
function consumeTo(stream: StringStream, close: string): boolean {
  while (!stream.eol()) {
    if (stream.match(close)) return true;
    stream.next();
  }
  return false;
}

/** Consume a JSP marker starting exactly here, or leave the stream untouched and answer `null`.
 *
 *  `el` is `false` inside a **template literal**, where `${…}` is JavaScript's own interpolation
 *  and nothing to do with EL. It is the one opener the two languages share, and getting it wrong
 *  in that direction would grey out the interesting half of every template. */
function tryJsp(stream: StringStream, state: JsState, el = true): string | null {
  for (const [open, close] of JSP_BLOCKS) {
    if (!el && open === '${') continue;
    if (stream.match(open)) {
      if (!consumeTo(stream, close)) state.jspClose = close;
      // It stands where a value stands: `x = <%= n %> + 1` is an addition, not a syntax error.
      mark(state, 'value', 'a');
      return 'meta';
    }
  }
  if (stream.match(JSP_TAG)) {
    mark(state, 'value', 'a');
    return 'meta';
  }
  if (JSP_TAG_OPEN.test(stream.string.slice(stream.pos))) {
    stream.skipToEnd();
    state.jspClose = '>';
    mark(state, 'value', 'a');
    return 'meta';
  }
  return null;
}

/** Consume the rest of an open quoted string, stopping early at a JSP marker.
 *
 *  A JS string does not survive a newline, so an unterminated one ends with the line rather than
 *  swallowing the file — the usual outcome of a `'` inside a marker this tokenizer did not see. */
function inString(stream: StringStream, state: JsState): string {
  const top = state.frames[state.frames.length - 1];
  const quote = top?.quote ?? '"';
  const from = stream.pos;
  while (!stream.eol()) {
    const c = stream.peek();
    if (c === '\\') {
      stream.next();
      stream.next();
      continue;
    }
    if (c === quote) {
      stream.next();
      state.frames.pop();
      mark(state, 'value', quote);
      return 'string';
    }
    // Only once something has been consumed: a marker sitting right here was already offered to
    // `tryJsp`, and returning with no progress would stall the tokenizer.
    if (stream.pos > from && jspStartsAt(stream)) return 'string';
    stream.next();
  }
  state.frames.pop();
  mark(state, 'value', quote);
  return 'string';
}

/** Consume to the end of an open block comment, or to the end of the line. */
function inBlockComment(stream: StringStream, state: JsState): string {
  while (!stream.eol()) {
    if (stream.match('*/')) {
      state.inComment = false;
      return 'comment';
    }
    stream.next();
  }
  return 'comment';
}

/** Consume template-literal text up to its close or its next `${` hole. */
function inTemplate(stream: StringStream, state: JsState): string {
  while (!stream.eol()) {
    if (stream.peek() === '\\') {
      stream.next();
      stream.next();
      continue;
    }
    if (stream.match('${')) {
      state.frames.push({ kind: 'expr', braces: 0 });
      mark(state, 'punct', '{');
      return 'string-2';
    }
    if (stream.peek() === '`') {
      stream.next();
      state.frames.pop();
      mark(state, 'value', '`');
      return 'string';
    }
    stream.next();
  }
  return 'string';
}

function readName(stream: StringStream, state: JsState): string {
  // Captured before anything is marked: `lastChar` is about to be overwritten, and two of the
  // three answers below depend on what came *before* this name.
  const prevChar = state.lastChar;
  const wasDeclaring = state.declaring;
  const wasConstructing = state.constructing;

  stream.eatWhile(IDENT);
  const word = stream.current();
  const next = peekAhead(stream);

  // A member is a member even when it is spelled like a keyword: `promise.catch(…)`,
  // `obj.default`, `x.class`. Checking the dot first is what stops those turning orange.
  if (prevChar === '.') {
    mark(state, 'value', 'a');
    return next === '(' ? 'callee' : 'property';
  }

  if (word === 'this' || word === 'super') {
    mark(state, 'value', 'a');
    return 'self';
  }
  if (ATOMS.has(word)) {
    mark(state, 'value', 'a');
    return 'atom';
  }
  if (KEYWORDS.has(word)) {
    // `get` and `set` are keywords only in front of a name. Everywhere else they are the two
    // most ordinary method names in the language.
    if ((word === 'get' || word === 'set') && !IDENT_START.test(next)) {
      mark(state, 'value', 'a');
      return next === '(' ? 'callee' : 'variable';
    }
    state.declaring = DECLARERS.has(word);
    state.constructing = word === 'new';
    mark(state, 'keyword', 'a');
    return 'keyword';
  }

  state.declaring = false;
  state.constructing = false;
  mark(state, 'value', 'a');

  if (wasDeclaring) return 'def';
  if (wasConstructing) return 'variable-3';
  if (next === ':' && isKeyPosition(prevChar)) return 'property';
  if (next === '(') return 'callee';
  if (GLOBALS.has(word)) return 'variable-3';
  // A capitalised name is a constructor or a namespace by every convention this code follows —
  // `Math`, `JSON`, `Date`, `OrderDialog`.
  if (/^[A-Z]/.test(word)) return 'variable-3';
  return 'variable';
}

function token(stream: StringStream, state: JsState): string | null {
  // A scriptlet spanning lines outranks everything, including an open string: the marker is
  // resolved by the server, so nothing inside it is JavaScript to begin with.
  if (state.jspClose) {
    if (consumeTo(stream, state.jspClose)) state.jspClose = null;
    return 'meta';
  }
  if (state.inComment) return inBlockComment(stream, state);

  const frame = () => state.frames[state.frames.length - 1];
  if (frame()?.kind === 'str') return tryJsp(stream, state) ?? inString(stream, state);
  if (frame()?.kind === 'tmpl') return tryJsp(stream, state, false) ?? inTemplate(stream, state);

  if (stream.eatSpace()) return null;

  // Before anything else, because two of the openers start with characters JavaScript also uses:
  // `$` begins an identifier and `<` an operator, and reading `${count}` as a name followed by a
  // block is how a page's own values disappear into the surrounding code.
  const jsp = tryJsp(stream, state);
  if (jsp) return jsp;

  const ch = stream.peek() ?? '';

  // ── comments ───────────────────────────────────────────────────────────────
  if (stream.match('//')) {
    stream.skipToEnd();
    return 'comment';
  }
  if (stream.match('/*')) {
    state.inComment = true;
    return inBlockComment(stream, state);
  }

  // ── a regex, or a division ─────────────────────────────────────────────────
  if (ch === '/' && REGEX_OK_AFTER.has(state.lastType)) {
    stream.next();
    let inClass = false;
    let closed = false;
    while (!stream.eol()) {
      const c = stream.next();
      if (c === '\\') {
        stream.next();
        continue;
      }
      if (c === '[') inClass = true;
      else if (c === ']') inClass = false;
      else if (c === '/' && !inClass) {
        closed = true;
        break;
      }
    }
    if (closed) stream.eatWhile(/[dgimsuvy]/);
    mark(state, 'value', '/');
    return 'string-2';
  }

  // ── strings ────────────────────────────────────────────────────────────────
  if (ch === '"' || ch === "'") {
    stream.next();
    // A frame rather than a loop to the closing quote: a JSP marker can interrupt one, and the
    // tokenizer has to be able to come back into the string afterwards.
    //
    // The opening quote is returned on its own rather than consuming the body here, so the body
    // is entered through the frame — which offers the very first character to `tryJsp`. Consume
    // it here and `"<wp:i18n key="X"/>"` would have eaten the `<` before anyone looked at it,
    // which is the entire bug this exists to fix.
    state.frames.push({ kind: 'str', braces: 0, quote: ch });
    mark(state, 'value', ch);
    return 'string';
  }
  if (ch === '`') {
    stream.next();
    state.frames.push({ kind: 'tmpl', braces: 0 });
    mark(state, 'value', '`');
    return 'string';
  }

  // ── numbers ────────────────────────────────────────────────────────────────
  //
  // Every form, because the one a highlighter misses is always the one on screen: a hex colour,
  // a bitmask in binary, a separator in a big constant, a `500` for a duration.
  if (/[0-9]/.test(ch) || (ch === '.' && /[0-9]/.test(stream.string[stream.pos + 1] ?? ''))) {
    if (
      stream.match(/^0[xX][0-9a-fA-F_]+n?/) ||
      stream.match(/^0[bB][01_]+n?/) ||
      stream.match(/^0[oO][0-7_]+n?/) ||
      stream.match(/^(?:\d[\d_]*)?\.\d[\d_]*(?:[eE][+-]?\d+)?/) ||
      stream.match(/^\d[\d_]*(?:\.\d*)?(?:[eE][+-]?\d+)?n?/)
    ) {
      mark(state, 'value', '0');
      return 'number';
    }
  }

  // ── names ──────────────────────────────────────────────────────────────────
  if (IDENT_START.test(ch)) return readName(stream, state);

  // ── braces, and the template holes they can close ──────────────────────────
  if (ch === '{') {
    stream.next();
    const top = frame();
    if (top?.kind === 'expr') top.braces += 1;
    mark(state, 'punct', '{');
    return 'bracket';
  }
  if (ch === '}') {
    stream.next();
    const top = frame();
    if (top?.kind === 'expr') {
      if (top.braces === 0) {
        state.frames.pop();
        mark(state, 'value', '}');
        return 'string-2';
      }
      top.braces -= 1;
    }
    mark(state, 'punct', '}');
    return 'bracket';
  }
  if ('()[];,'.includes(ch)) {
    stream.next();
    // `)` and `]` leave a value behind them, so a `/` after one is division. `(`, `[`, `,` and
    // `;` do not.
    mark(state, ch === ')' || ch === ']' ? 'value' : 'punct', ch);
    return 'bracket';
  }
  if (ch === '.') {
    stream.next();
    // `lastChar` is the point: the next name has to know it follows a dot.
    mark(state, 'operator', '.');
    return 'operator';
  }

  // ── operators ──────────────────────────────────────────────────────────────
  if (
    stream.match(
      /^(?:>>>=|\.\.\.|===|!==|\*\*=|<<=|>>=|>>>|&&=|\|\|=|\?\?=|==|!=|<=|>=|&&|\|\||\?\?|\?\.|\+\+|--|\+=|-=|\*=|\/=|%=|&=|\|=|\^=|=>|\*\*|<<|>>|[-+*/%<>=!&|^~?:])/,
    )
  ) {
    const op = stream.current();
    // `++` and `--` leave a value behind them (`i++ / 2` is division); nothing else does.
    mark(state, op === '++' || op === '--' ? 'value' : 'operator', op);
    return 'operator';
  }

  stream.next();
  mark(state, 'punct', ch);
  return null;
}

/**
 * The JavaScript stream parser.
 *
 * Used two ways, which is why it carries a `tokenTable`: **injected** into a host grammar's
 * raw-text region (a JSP `<script>` body — `highlight.ts` maps the names itself), and wrapped in
 * `StreamLanguage` for a standalone `.js` file, where CodeMirror maps them through Lezer tags and
 * only knows the CM5 vocabulary. The two names that are not in it are declared here so the same
 * mode colours the same code the same way whichever door it came through.
 */
export const javascriptStream: StreamParser<JsState> = {
  name: 'bennu-js',
  startState,
  token,
  tokenTable: {
    callee: t.function(t.variableName),
    self: t.self,
  },
  languageData: {
    commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
  },
};
