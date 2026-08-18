/**
 * Which lines of a source file can hold a breakpoint — Java, and Rust.
 *
 * A breakpoint is a *bytecode location*, and only a line that compiles to bytecode has one. A
 * package statement, a field declaration, a method signature, an annotation, a comment or a
 * lone brace compile to nothing — so a breakpoint set on one binds to whatever executable line
 * comes after it. That is how you end up watching a line that never runs while the one you
 * meant is three lines up.
 *
 * The backend knows the true answer, but only once a VM is attached and the class is loaded,
 * which is far too late to be the thing that tells you where you may click. So the gutter
 * offers a breakpoint only on lines this module accepts, and the absence of the affordance is
 * the explanation: there is nothing to click, rather than a click that quietly does something
 * else.
 *
 * ## A deny-list, deliberately
 *
 * The two failure directions are not symmetric. **Wrongly refusing** an executable line means
 * you cannot set a breakpoint where you must — the feature is broken for that line. **Wrongly
 * allowing** a non-executable one means the breakpoint slides to the next statement and the
 * backend says so in the tooltip, which is what happened before this module existed.
 *
 * So this refuses only what is *certainly* not code, and lets everything else through. It is a
 * syntactic reading of one file — no type information, no parse tree — which is enough for
 * every shape it names and deliberately silent about the rest.
 *
 * Not covered on purpose: the **closing brace of a method**, which does carry the implicit
 * return and which IntelliJ does allow. Telling it apart from the brace that closes a class,
 * an `if` or a lambda needs a real parse; refusing all bare braces costs a placement whose
 * alternative (the last statement, one line up) is right there.
 *
 * ## Rust, and why its reading is shorter
 *
 * The same asymmetry, leaning further the same way. A native breakpoint is resolved from DWARF by
 * LLDB or GDB, which are much more permissive than a JVM — and, unlike the JVM, the debug **adapter
 * answers whether it bound**, so a wrongly-allowed line is reported in the tooltip within a moment
 * of the launch. So the Rust deny-list refuses only what is certainly a declaration and lets
 * everything else through.
 *
 * Two Rust-specific hazards the scanner has to survive, both of which would otherwise swallow the
 * rest of a line and mis-read every one after it:
 *
 * * **lifetimes look like character literals.** `&'a str` opens a quote that never closes under a
 *   Java-shaped scanner, so `'` starts a literal only when the line really holds one (`'x'`, `'\n'`).
 * * **raw strings have no escapes and a variable delimiter** — `r#"…"#` — so they end at a matching
 *   `"###…` rather than at the first unescaped quote. A `\` inside one is a backslash, not an escape.
 *
 * Block comments **nest** in Rust, which the depth counter below tracks; in Java they do not.
 */

/** The keywords a *statement* can start with. A line beginning with one of these is code, and
 *  is never mistaken for a declaration however much the rest of it looks like one — without
 *  this, `return compute(x);` reads as a method signature. */
const STATEMENT_KEYWORDS = new Set([
  'assert', 'break', 'case', 'catch', 'continue', 'default', 'do', 'else', 'finally', 'for',
  'if', 'new', 'return', 'super', 'switch', 'synchronized', 'this', 'throw', 'try', 'while',
  'yield',
]);

/**
 * Declaration modifiers, skipped before deciding what the rest of a line is.
 *
 * `sealed`, `non-sealed` and `default` are deliberately absent even though they can modify a
 * declaration: they are *contextual* keywords, so they are also legal identifiers, and treating
 * `sealed.close()` as a declaration would refuse a line that plainly runs. Their declarations
 * fall through to "allowed", which costs nothing.
 */
const MODIFIERS = new Set([
  'public', 'private', 'protected', 'static', 'final', 'abstract', 'synchronized', 'native',
  'transient', 'volatile', 'strictfp',
]);

/** Type-declaration keywords. Matched only as `class Name` and never as a bare word, because
 *  `record` is a contextual keyword and `record.save();` is a method call. */
const TYPE_DECLARATION = /^(?:class|interface|enum|record)\s+\w/;

/** Which language's rules to read a file by. */
export type BreakpointLanguage = 'java' | 'rust';

/**
 * The 1-based lines of `source` a breakpoint may be set on.
 *
 * One pass, no parse tree: comments and string literals are removed as the scan goes (so a `{`
 * or a `//` inside a string cannot mislead it), then each line is judged on its shape.
 */
export function breakpointableLines(
  source: string,
  language: BreakpointLanguage = 'java',
): Set<number> {
  return language === 'rust' ? rustLines(source) : javaLines(source);
}

function javaLines(source: string): Set<number> {
  const out = new Set<number>();
  const lines = source.split('\n');
  let inBlockComment = false;

  for (let i = 0; i < lines.length; i += 1) {
    const [code, stillInComment] = stripLine(lines[i], inBlockComment);
    inBlockComment = stillInComment;
    if (isExecutable(code.trim())) out.add(i + 1);
  }
  return out;
}

/**
 * One line with its comments and literal contents removed, plus whether a block comment is
 * still open at the end of it.
 *
 * String and character contents are blanked rather than dropped so nothing inside them can be
 * read as syntax — a `;` in a message must not turn a line into a declaration.
 */
function stripLine(line: string, inBlockComment: boolean): [string, boolean] {
  let out = '';
  let inComment = inBlockComment;
  let quote: '"' | "'" | null = null;
  let i = 0;

  while (i < line.length) {
    const c = line[i];
    const next = line[i + 1];

    if (inComment) {
      if (c === '*' && next === '/') {
        inComment = false;
        i += 2;
      } else {
        i += 1;
      }
      continue;
    }
    if (quote) {
      // A backslash escapes the next character, including the closing quote.
      if (c === '\\') {
        i += 2;
        continue;
      }
      if (c === quote) quote = null;
      i += 1;
      continue;
    }
    if (c === '/' && next === '*') {
      inComment = true;
      i += 2;
      continue;
    }
    if (c === '/' && next === '/') break; // the rest of the line is a comment
    if (c === '"' || c === "'") {
      quote = c;
      // A placeholder, so `"a; b"` still reads as one token rather than as two statements.
      out += '_';
      i += 1;
      continue;
    }
    out += c;
    i += 1;
  }
  return [out, inComment];
}

/** Whether a stripped, trimmed line carries something that runs. */
function isExecutable(code: string): boolean {
  if (!code) return false;
  // Punctuation only — a lone brace, a closing `});`, a stray semicolon.
  if (!/[A-Za-z0-9_$]/.test(code)) return false;

  const first = code.match(/^[A-Za-z_$][\w$]*/)?.[0];

  // A file-level statement, and an annotation: neither is inside a method.
  if (first === 'package' || first === 'import') return false;
  if (code.startsWith('@')) return false;

  // A statement keyword settles it, and is checked FIRST so nothing below can talk it out of
  // it: `return compute(x);` would otherwise read as a signature, and `synchronized (lock) {`
  // as a modifier followed by a declaration.
  if (first && STATEMENT_KEYWORDS.has(first)) return true;

  // Drop the leading modifiers to reach the thing being declared — or not declared.
  let rest = code;
  let hadModifiers = false;
  for (;;) {
    const word = rest.match(/^[A-Za-z_$][\w$]*/)?.[0];
    if (!word || !MODIFIERS.has(word)) break;
    rest = rest.slice(word.length).trimStart();
    hadModifiers = true;
  }

  // `class Foo {`, `public enum Colour {` — the line opens a body, it is not inside one.
  if (TYPE_DECLARATION.test(rest)) return false;
  // `static {` — an initializer block's opening line: modifiers and nothing else.
  if (hadModifiers && !/[A-Za-z0-9_$]/.test(rest)) return false;

  // An assignment is the one declaration that also runs: `private static final int MAX =
  // compute();` executes, `private int max;` does not.
  if (/(^|[^=!<>])=([^=]|$)/.test(code)) return true;

  // `void run(` / `List<String> names(` — a signature: a type before the name, then a
  // parameter list. A call has nothing before its name (`run(x);`) or reaches it through a
  // receiver (`svc.run();`), and neither shape matches.
  if (/^[\w$<>[\],.\s?]+\s+\w+\s*\(/.test(rest)) return false;
  // A constructor: `Foo(` reached only after modifiers, which a call never has in front of it.
  if (hadModifiers && /^\w+\s*\(/.test(rest)) return false;

  // `private String name;` / `int count;` — a declaration with nothing to run: a type, a name,
  // a terminator, and no parentheses to make it a call.
  if (!code.includes('(') && /^[\w$<>[\],.\s?]+\s+\w+\s*[;,]\s*$/.test(rest)) return false;

  return true;
}

// ── Rust ──────────────────────────────────────────────────────────────────────

/**
 * Declaration keywords: a line that starts with one of these, after its modifiers, declares
 * something rather than running it.
 *
 * `fn` is deliberately **absent**. A function's signature line maps to its prologue in DWARF, both
 * LLDB and GDB bind a breakpoint there, and it is the natural place to put one when you want to stop
 * on every call — refusing it would be refusing the most common breakpoint in a debugger session.
 */
const RUST_DECLARATIONS = new Set([
  'use', 'extern', 'mod', 'struct', 'enum', 'union', 'trait', 'impl', 'type', 'where',
  // A `const`/`static` initializer is const-evaluated: there is no runtime code on the line. The one
  // exception, `const fn`, is handled below — it is a function, and its name here is a modifier.
  'const', 'static',
]);

/** Modifiers to skip before deciding what a line declares — or does not. */
const RUST_MODIFIERS = new Set(['pub', 'unsafe', 'async', 'default', 'move']);

/**
 * One Rust line with its comments and literal contents blanked, plus the block-comment depth still
 * open at the end of it.
 *
 * Depth rather than a boolean: Rust's block comments **nest**, so an inner one's closing delimiter
 * does not end the outer one, and a boolean would end the comment one delimiter early — reading the
 * rest of a commented-out block as live code.
 *
 * (Written without the delimiters spelled out, because spelling them out closes this comment. Which is
 * the same class of mistake the depth counter exists to avoid.)
 */
function stripRustLine(line: string, depth: number): [string, number] {
  let out = '';
  let i = 0;

  while (i < line.length) {
    if (depth > 0) {
      if (line.startsWith('/*', i)) { depth += 1; i += 2; continue; }
      if (line.startsWith('*/', i)) { depth -= 1; i += 2; continue; }
      i += 1;
      continue;
    }
    if (line.startsWith('//', i)) break; // a line comment: nothing after it counts
    if (line.startsWith('/*', i)) { depth += 1; i += 2; continue; }

    // A raw string — `r"…"`, `r#"…"#`, `br##"…"##`. No escapes inside, and the terminator carries the
    // same number of hashes as the opener, which is the whole reason it cannot be scanned as a string.
    const raw = /^(b?r)(#*)"/.exec(line.slice(i));
    if (raw) {
      const hashes = raw[2];
      const close = `"${hashes}`;
      const from = i + raw[0].length;
      const end = line.indexOf(close, from);
      out += ' ';
      if (end < 0) return [out, depth]; // a multi-line raw string: the rest of the line is content
      i = end + close.length;
      continue;
    }

    // A character literal, and ONLY when it really is one: `'a` in `&'a str` is a lifetime, and
    // treating it as an open quote swallows the rest of the line.
    const ch = /^b?'(?:\\.|[^'\\])'/.exec(line.slice(i));
    if (ch) { out += ' '; i += ch[0].length; continue; }

    if (line[i] === '"') {
      i += 1;
      while (i < line.length) {
        if (line[i] === '\\') { i += 2; continue; }
        if (line[i] === '"') { i += 1; break; }
        i += 1;
      }
      out += ' ';
      continue;
    }
    out += line[i];
    i += 1;
  }
  return [out, depth];
}

/** Whether a stripped, trimmed Rust line carries something that runs. */
function isRustExecutable(code: string): boolean {
  if (!code) return false;
  // Punctuation only — a lone brace, a `});`, a `,`.
  if (!/[A-Za-z0-9_]/.test(code)) return false;
  // An attribute or an inner attribute: `#[derive(Debug)]`, `#![allow(…)]`. Not code, and a `#[test]`
  // sitting above the function you want is the line people click by accident.
  if (code.startsWith('#[') || code.startsWith('#![')) return false;

  // Drop the modifiers to reach the thing being declared. `pub(crate)` / `pub(super)` come off with
  // their parenthesised part, which is not a call however much it looks like one.
  let rest = code;
  for (;;) {
    const vis = /^pub\s*\([^)]*\)\s*/.exec(rest);
    if (vis) { rest = rest.slice(vis[0].length); continue; }
    const word = /^[A-Za-z_][\w]*/.exec(rest)?.[0];
    if (!word || !RUST_MODIFIERS.has(word)) break;
    // `extern "C" fn` — the ABI string was blanked by the stripper, so only the keyword is left.
    rest = rest.slice(word.length).trimStart();
  }

  const first = /^[A-Za-z_][\w]*/.exec(rest)?.[0];
  if (!first) return !/^[^A-Za-z0-9_]*$/.test(rest);

  // `const fn` is a function, not a constant — and it is the one place the deny-list would refuse a
  // line every debugger accepts.
  if (first === 'const' && /^const\s+fn\b/.test(rest)) return true;
  // `extern crate serde;` and `extern "C" { … }` are declarations; `extern "C" fn` reached `fn` above.
  if (RUST_DECLARATIONS.has(first)) return false;

  return true;
}

/** The 1-based lines of a Rust source a breakpoint may be set on. */
function rustLines(source: string): Set<number> {
  const out = new Set<number>();
  const lines = source.split('\n');
  let depth = 0;

  for (let i = 0; i < lines.length; i += 1) {
    const [code, nextDepth] = stripRustLine(lines[i], depth);
    // Judged with the depth it STARTED at: a line that opens a block comment after some code still
    // has that code on it.
    const wasInComment = depth > 0 && nextDepth > 0 && !code.trim();
    depth = nextDepth;
    if (!wasInComment && isRustExecutable(code.trim())) out.add(i + 1);
  }
  return out;
}
