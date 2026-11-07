/**
 * The `.ptn` {@link LanguageDescriptor} — Patina, the Voriz mod language.
 *
 * ## Two layers, the same split as `.dev`
 *
 * - **Colour is local and instant**: a stream mode that knows the *shape* of the text and nothing about which
 *   names exist. It cannot go stale, because the only lists it carries are the language's own (keywords, base
 *   types, the four prelude constructors), which do not change with a mod or a game version.
 * - **Correctness comes from the backend**: completion, hover, go-to, find-usages, rename and diagnostics are
 *   answered by the `language-intel` extension that serves `.ptn` (the Patina package), through `bennu-be`'s
 *   `patina_intel` route. This file only wires completion and hover, which are descriptor hooks; the rest are
 *   editor gestures routed by path (see `file-kind.ts`).
 *
 * ## What the shape alone can tell apart
 *
 * More than it looks, because Patina's syntax says a lot about a name from its neighbours:
 *
 * | written | read as |
 * |---|---|
 * | `name(` | a call |
 * | `.name(` | a method call |
 * | `.name` | a field |
 * | `Type::name(` | an associated function — the "static" of Patina, italic as in IntelliJ |
 * | `Mood::Calm`, `Some`, `None`, `Ok`, `Err` | a variant, coloured like a constant |
 * | `POWER` | a constant |
 * | `fn name`, `struct Name` | a declaration |
 * | `name:` in a struct, a literal or a call | a field / named argument |
 * | `a::b::` | a module path |
 * | `block!(…)` | a form, a name the language brings |
 *
 * What it cannot tell is what needs the index: a parameter from a local, a `mut` binding from another, a trait
 * from a struct. That is a semantic layer the backend would have to send.
 *
 * ## Folding
 *
 * A stream mode carries no tree, so blocks fold by their braces: a line whose last real `{` opens a block that
 * closes on a later line folds up to that `}` (strings and `//` comments skipped).
 */

import { foldService, HighlightStyle, StreamLanguage, syntaxHighlighting } from '@codemirror/language';
import type { StreamParser } from '@codemirror/language';
import { Tag, tags as t } from '@lezer/highlight';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import { backendCompletionSource, backendHoverSource } from './lsp-lang';

/** Patina's keywords, as its lexer has them — reserved-for-later words included, so they read as reserved. */
const KEYWORDS = new Set([
  'as', 'break', 'const', 'continue', 'crate', 'else', 'enum', 'fn', 'for', 'if', 'impl', 'in', 'let', 'loop',
  'match', 'mut', 'pub', 'return', 'super', 'struct', 'trait', 'use', 'where', 'while',
  'async', 'await', 'dyn', 'move', 'ref', 'static', 'task', 'type', 'unsafe', 'yield',
]);

/** Keywords after which the next name is the one a declaration introduces. */
const DECLARES_FN = new Set(['fn']);
const DECLARES_TYPE = new Set(['struct', 'enum', 'trait']);

/** The language's lowercase types: names in the base library, read as types wherever they appear. */
const BASE_TYPES = new Set(['int', 'fx', 'f32', 'bool', 'str']);

/** The prelude's constructors, written bare. */
const PRELUDE_VARIANTS = new Set(['Some', 'None', 'Ok', 'Err']);

/**
 * A number as Patina's lexer reads it: `1_000`, `1.5` (an `fx`: a decimal needs no suffix), `2.0f32`, and durations
 * (`2s`, `500ms`, `3t`, `1min`). Hex and binary too.
 */
const NUMBER = /^(?:0x[0-9a-fA-F_]+|0b[01_]+|\d[\d_]*(?:\.\d[\d_]*)?)(?:f32|min|ms|s|t)?\b/;

/** An associated-function call (`BlockPos::new(…)`): Patina's "static". A tag of its own, styled below with the call
 *  colour and the italic. (`Tag.define` refuses a modified tag such as `t.function(…)` as its parent, so it derives
 *  from the plain name and the style says the rest.) */
const staticCall = Tag.define(t.variableName);

/** A primitive type (`int`, `fx`, `bool`…): a type, but not one the file declared. A tag of its own, styled below with
 *  `--syntax-primitive`, so it reads apart from both the keywords and the other types. */
const primitiveType = Tag.define(t.typeName);

interface PatinaState {
  /** What the last meaningful token was: `.`, `::`, a keyword that declares, or nothing in particular. */
  prev: '' | '.' | '::' | 'fn' | 'type' | 'let';
  /** Inside a `use` line: every name in it is a path segment or an imported name. */
  inUse: boolean;
  /** Inside a `fn` signature, before its body: `name:` there is a parameter, not a field. */
  inSignature: boolean;
}

/** A name followed by a single `:` (not `::`). */
const SINGLE_COLON = /^\s*:(?!:)/;

export const patinaMode: StreamParser<PatinaState> = {
  name: 'patina',

  startState: () => ({ prev: '', inUse: false, inSignature: false }),

  // `'` is a label, never a character literal: Patina has none. The rainbow-bracket scanner reads this, so a
  // `'outer` does not swallow the brackets after it.
  languageData: { commentTokens: { line: '//' }, rainbowSingleQuote: false },

  tokenTable: { staticCall, primitiveType },

  token(stream, state): string | null {
    if (stream.sol() && state.inUse) state.inUse = false;
    if (stream.eatSpace()) return null;

    // `//` and `///` to the end of the line. Patina has no block comments (its lexer rejects them).
    if (stream.match('//')) {
      stream.skipToEnd();
      return 'comment';
    }
    // An attribute, whole: `#[on(StepOn, block: "landmine")]`.
    if (stream.match(/^#!?\[[^\]]*\]?/)) return 'meta';
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) {
      state.prev = '';
      return 'string';
    }
    // A label: `'outer`.
    if (stream.match(/^'[A-Za-z_]\w*/)) return 'labelName';
    if (stream.match(NUMBER)) {
      state.prev = '';
      return 'number';
    }
    if (stream.match('::')) {
      state.prev = '::';
      return 'punctuation';
    }
    if (stream.eat('.')) {
      // `..` and `..=` are ranges; a lone `.` is member access.
      if (stream.match(/^\.=?/)) {
        state.prev = '';
        return 'operator';
      }
      state.prev = '.';
      return 'punctuation';
    }

    const word = stream.match(/^[\p{L}_][\p{L}\p{N}_]*/u) as RegExpMatchArray | null;
    if (word) return name(word[0], stream, state);

    // Brackets get no style of their own: the rainbow-bracket layer colours them by depth, and a highlight span
    // inside its mark would win and paint them all one grey.
    if (stream.eat('{')) {
      // The signature is over: names with a colon inside the body are fields or named arguments again.
      state.inSignature = false;
      state.prev = '';
      return null;
    }
    if (stream.match(/^[()[\]}]/)) {
      state.prev = '';
      return null;
    }
    if (stream.match(/^(?:->|=>|==|!=|<=|>=|&&|\|\||\+=|-=|\*=|\/=|[-+*/%=<>!&|^?])/)) {
      state.prev = '';
      return 'operator';
    }
    stream.next();
    state.prev = '';
    return 'punctuation';
  },
};

/** One identifier, coloured by what is around it. */
function name(w: string, stream: Parameters<StreamParser<PatinaState>['token']>[0], state: PatinaState): string {
  const prev = state.prev;
  state.prev = '';

  if (w === 'self' || w === 'Self') return 'self';
  if (w === 'true' || w === 'false') return 'bool';
  if (KEYWORDS.has(w)) {
    if (w === 'use') state.inUse = true;
    if (DECLARES_FN.has(w)) {
      state.prev = 'fn';
      state.inSignature = true;
    } else if (DECLARES_TYPE.has(w)) state.prev = 'type';
    else if (w === 'let' || w === 'mut') state.prev = 'let';
    return 'keyword';
  }

  // A form: `block!("tnt")`, `t!("mod:key")`. A name the language brings, like a builtin.
  if (stream.peek() === '!' && stream.string.charAt(stream.pos + 1) !== '=') {
    stream.next();
    return 'variableName.standard';
  }

  const calls = stream.peek() === '(';
  const pathFollows = stream.match(/^::/, false) !== null;
  const capital = /^\p{Lu}/u.test(w);

  if (prev === 'fn') return 'variableName.definition';
  if (prev === 'type') return 'typeName.definition';
  // After a dot: a method when called, a field otherwise.
  if (prev === '.') return calls ? 'propertyName.function' : 'propertyName';
  // A module path segment: lowercase and followed by `::` (`voriz::prelude::`, `crate::tally::`).
  if (pathFollows && !capital) return 'namespace';
  if (state.inUse) return capital ? 'typeName' : 'namespace';
  // After `Type::` — an associated function when called, a variant when capitalised, a constant otherwise.
  if (prev === '::') {
    if (calls) return 'staticCall';
    if (capital) return pathFollows ? 'typeName' : 'variableName.constant';
    return 'variableName.constant';
  }
  if (PRELUDE_VARIANTS.has(w)) return 'variableName.constant';
  if (/^[A-Z][A-Z0-9_]+$/.test(w)) return 'variableName.constant';
  // A base type reads apart from the other types — unless a colon follows, which makes it a name (`str: 1`, `fx: 2.0`).
  if (BASE_TYPES.has(w) && !stream.match(SINGLE_COLON, false)) return 'primitiveType';
  if (capital) return 'typeName';
  if (calls) return 'variableName.function';
  // `name:` — a field in a struct or a literal, a named argument in a call. Not a parameter (the signature), not a
  // `let x: T`.
  if (prev !== 'let' && !state.inSignature && stream.match(SINGLE_COLON, false)) return 'propertyName';
  return 'variableName';
}

/** The colour of a primitive type. Both shipped themes define `--syntax-primitive`; a custom theme that does not gets a
 *  blend of its type and string colours, which is distinct from the keyword, the type and the number alike. */
const PRIMITIVE_COLOR =
  'var(--syntax-primitive, color-mix(in srgb, var(--syntax-type, #4d78cc) 50%, var(--syntax-string, #6a9956)))';

/** What the shared theme has no rule for: the italic of an associated call, a declared type, a label. */
export const patinaHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: staticCall, color: 'var(--syntax-function, #ffc66d)', fontStyle: 'italic' },
    // `int`, `fx`, `bool`…: the primitive colour, a mix of the type and string colours when a theme has none of its own.
    { tag: primitiveType, color: PRIMITIVE_COLOR },
    // A type a line declares, as a `fn` name is: the type colour, bolder.
    { tag: t.definition(t.typeName), color: 'var(--syntax-type, #4d9be6)', fontWeight: '600' },
    { tag: t.labelName, color: 'var(--syntax-label, var(--text-secondary))', fontStyle: 'italic' },
  ]),
);

// ── folding by braces ───────────────────────────────────────────────────────────

/**
 * Fold the block a line opens: from after its last real `{` to the `}` that closes it, when that is on a later
 * line. Strings and `//` comments are skipped on both sides; Patina has no other construct that hides a brace.
 */
const braceFold = foldService.of((state, lineStart, lineEnd) => {
  const line = state.doc.sliceString(lineStart, lineEnd);
  let open = -1;
  let inStr = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (inStr) {
      if (c === '\\') i++;
      else if (c === '"') inStr = false;
      continue;
    }
    if (c === '"') inStr = true;
    else if (c === '/' && line[i + 1] === '/') break;
    else if (c === '{') open = i;
    else if (c === '}' && open >= 0) open = -1;
  }
  if (open < 0) return null;
  const from = lineStart + open + 1;
  const text = state.doc.sliceString(from, Math.min(state.doc.length, from + 200_000));
  let depth = 1;
  inStr = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inStr) {
      if (c === '\\') i++;
      else if (c === '"') inStr = false;
      continue;
    }
    if (c === '"') inStr = true;
    else if (c === '/' && text[i + 1] === '/') {
      const nl = text.indexOf('\n', i);
      if (nl < 0) return null;
      i = nl;
    } else if (c === '{') depth++;
    else if (c === '}' && --depth === 0) {
      const to = from + i;
      return state.doc.lineAt(to).number > state.doc.lineAt(from).number ? { from, to } : null;
    }
  }
  return null;
});

/** The `.ptn` language: shape from here, truth from the `language-intel` extension. */
export const patinaLanguage: LanguageDescriptor = {
  id: 'patina',
  createParser: () => Promise.reject(new Error('patina: highlighted by a stream mode')),
  classify: () => null,
  cmExtension: [StreamLanguage.define(patinaMode), patinaHighlight, braceFold],
  // The gutter: the ranges come from `braceFold` above, not from a grammar.
  cmFold: true,
  serverFold: false,
  commentTokens: { line: '//' },
  // A Rust-shaped language: completion after a dot, a `::`, or a word — the default backend source's gate.
  intel: { completion: backendCompletionSource, hover: backendHoverSource },
};
