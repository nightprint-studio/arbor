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
 * | `t!("ui:key")`, `block!("tnt")` | the id inside a catalog form: its own colour, one per kind (text key / catalog id) |
 * | `"hi {p.x}"` | an interpolation hole: the braces and the path inside read apart from the text around them |
 *
 * What it cannot tell is what needs the index: a parameter from a local, a `mut` binding from another, a trait
 * from a struct. That is a semantic layer the backend would have to send.
 *
 * ## Folding
 *
 * A stream mode carries no tree, so blocks fold by their braces: a line whose last real `{` opens a block that
 * closes on a later line folds up to that `}` (strings and `//` comments skipped).
 */

import { patinaExtraFold, useBlockRange } from './patina-fold';
import { foldService, HighlightStyle, StreamLanguage, syntaxHighlighting } from '@codemirror/language';
import type { StreamParser } from '@codemirror/language';
import { Tag, tags as t } from '@lezer/highlight';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import type { CompletionContext, CompletionResult, CompletionSource } from '@codemirror/autocomplete';
import { backendCompletionSource, backendHoverSource, eagerBackendCompletionSource, keyBackendCompletionSource } from './lsp-lang';
import { patinaIndent } from './patina-indent';
import { patinaLinkRange } from './patina-link';
import { stringContext } from './patina-strings';

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

/** An attribute (`#[server]`, `#[on(DecorateChunk, …)]`): its `#` and its name. A tag of its own, styled below with
 *  `--syntax-attribute`; the arguments inside the brackets keep their ordinary colours. */
const attribute = Tag.define(t.meta);

/** The string argument of `t!(…)`: a translation key. A tag of its own, styled below. */
const textKey = Tag.define(t.string);

/** The string argument of any other catalog form (`block!`, `item!`, `ui!`…): an id of the game's data. */
const catalogId = Tag.define(t.string);

/** The braces of an interpolation hole `"{p.x}"`, and the path inside. */
const holeBrace = Tag.define(t.punctuation);
const holePath = Tag.define(t.variableName);

/**
 * The forms whose string arguments are ids of the game's data (`#[form("…")]` in the library): a catalog id, or a text
 * key for `t`. `assert!`, `log!` and the other function-like forms are left out: their strings are text.
 */
const CATALOG_FORMS = new Set([
  'action', 'anim', 'biome', 'block', 'decoration', 'dimension', 'effect', 'flag', 'generator', 'group', 'hud_key', 'item', 'loot',
  'map', 'material', 'material_param', 'mode', 'patch', 'piece', 'projectile', 'screen', 'sound', 'species', 'status',
  'structure_piece', 't', 'ui', 'world_type',
]);

interface PatinaState {
  /** Bracket depth inside an attribute's `[ … ]`; `0` outside one. Carried across lines, so `#[` and its name may be apart. */
  attrDepth: number;
  /** Just after `#` / `#!`: the next `[` opens an attribute. */
  attrOpen: boolean;
  /** Just after an attribute's `[`: the next word is the attribute's name. */
  attrName: boolean;
  /** What the last meaningful token was: `.`, `::`, a keyword that declares, or nothing in particular. */
  prev: '' | '.' | '::' | 'fn' | 'type' | 'let';
  /** Inside a `use` line: every name in it is a path segment or an imported name. */
  inUse: boolean;
  /** Inside a `fn` signature, before its body: `name:` there is a parameter, not a field. */
  inSignature: boolean;
  /** One entry per open bracket: the name of the catalog form it is the argument list of (`block`, `t`…), or null. */
  stack: (string | null)[];
  /** The catalog form whose `!` was just read: the `(` right after it opens its argument list. */
  pendingForm: string | null;
  /** Inside a string: which tag its text takes, or `null` outside one. */
  str: 'string' | 'textKey' | 'catalogId' | null;
  /** Inside an interpolation hole `{…}` of the string. */
  hole: boolean;
}

/** A hole in a string's text (with the `{{` escapes already taken out): a brace and the first letter of a name. */
const HOLE = /\{[\p{L}_]/u;

/** Past this many open brackets the stack stops growing: a broken file must not make the state unbounded. */
const MAX_STACK = 64;

/** A name followed by a single `:` (not `::`). */
const SINGLE_COLON = /^\s*:(?!:)/;

export const patinaMode: StreamParser<PatinaState> = {
  name: 'patina',

  startState: () => ({ attrDepth: 0, attrOpen: false, attrName: false, prev: '', inUse: false, inSignature: false, stack: [], pendingForm: null, str: null, hole: false }),

  // `'` is a label, never a character literal: Patina has none. The rainbow-bracket scanner reads this, so a
  // `'outer` does not swallow the brackets after it.
  languageData: { commentTokens: { line: '//' }, rainbowSingleQuote: false, indentOnInput: /^\s*[}\])]$/ },

  tokenTable: { staticCall, primitiveType, attribute, textKey, catalogId, holeBrace, holePath },

  token(stream, state): string | null {
    if (stream.sol()) {
      if (state.inUse) state.inUse = false;
      // A string does not span lines: an unterminated one ends where its line does.
      state.str = null;
      state.hole = false;
    }
    if (state.str) return stringPart(stream, state);
    if (stream.eatSpace()) return null;
    // The `(` right after `block!` opens that form's argument list; anything else between them cancels it.
    const form = state.pendingForm;
    state.pendingForm = null;

    // `//` and `///` to the end of the line. Patina has no block comments (its lexer rejects them).
    if (stream.match('//')) {
      stream.skipToEnd();
      return 'comment';
    }
    // An attribute, `#[on(StepOn, block: "landmine")]`: the `#` and the name get the attribute colour, the brackets the
    // rainbow's and the arguments their own (a string is a string, `block:` a field) — read below as ordinary tokens.
    if (state.attrDepth === 0 && stream.match(/^#!?(?=\[)/)) {
      state.attrOpen = true;
      return 'attribute';
    }
    if (state.attrOpen && stream.eat('[')) {
      state.attrOpen = false;
      state.attrDepth = 1;
      state.attrName = true;
      state.stack.push(null);
      return null;
    }
    if (stream.peek() === '"') {
      state.prev = '';
      // The argument of a catalog form is an id (`t` a text key); every other string is text.
      const top = state.stack[state.stack.length - 1];
      const tag = top === undefined || top === null ? 'string' : top === 't' ? 'textKey' : 'catalogId';
      // A string with no hole is one token, as it always was; only one with a `{name}` in it is cut into pieces.
      const whole = stream.match(/^"(?:[^"\\]|\\.)*"?/, false) as RegExpMatchArray | null;
      if (whole && !HOLE.test(whole[0].replace(/\{\{/g, ''))) {
        stream.match(/^"(?:[^"\\]|\\.)*"?/);
        return tag;
      }
      stream.next();
      state.str = tag;
      return tag;
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
      // A `{` is never inside an attribute: an unclosed one ends here, instead of colouring the rest of the file.
      state.attrDepth = 0;
      state.attrName = false;
      // The signature is over: names with a colon inside the body are fields or named arguments again.
      state.inSignature = false;
      state.prev = '';
      if (state.stack.length < MAX_STACK) state.stack.push(null);
      return null;
    }
    if (stream.match(/^[()[\]}]/)) {
      const c = stream.current();
      if (c === '(' || c === '[') {
        if (state.stack.length < MAX_STACK) state.stack.push(c === '(' ? form : null);
      } else {
        state.stack.pop();
      }
      if (state.attrDepth > 0) {
        if (c === '[') state.attrDepth++;
        else if (c === ']') state.attrDepth--;
        else if (c === '}') state.attrDepth = 0;
        state.attrName = false;
      }
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

/**
 * One piece of the string the stream is inside: a hole's brace, the path in it, or the plain text up to the next one.
 * The text takes the tag the opening quote chose (`string`, `textKey`, `catalogId`), so a whole `t!("ui:conta")` reads as one
 * colour; only a hole breaks it.
 */
function stringPart(stream: Parameters<StreamParser<PatinaState>['token']>[0], state: PatinaState): string | null {
  const tag = state.str as string;
  if (stream.eat('"')) {
    state.str = null;
    state.hole = false;
    return tag;
  }
  if (state.hole) {
    if (stream.match(/^[\p{L}_][\p{L}\p{N}_.]*/u)) return 'holePath';
    if (stream.eat('}')) {
      state.hole = false;
      return 'holeBrace';
    }
    // Something a hole does not accept: the brace was only text.
    state.hole = false;
  }
  if (stream.peek() === '\\') {
    stream.next();
    stream.next();
    return tag;
  }
  if (stream.peek() === '{') {
    // `{{` is a literal brace; `{` and a name's first letter open a hole; a lone `{` is text.
    if (stream.match('{{')) return tag;
    if (stream.match(/^\{(?=[\p{L}_])/u)) {
      state.hole = true;
      return 'holeBrace';
    }
    stream.next();
    return tag;
  }
  if (!stream.match(/^[^"\\{]+/)) stream.next();
  return tag;
}

/** One identifier, coloured by what is around it. */
function name(w: string, stream: Parameters<StreamParser<PatinaState>['token']>[0], state: PatinaState): string {
  const prev = state.prev;
  state.prev = '';

  // The name of an attribute (`server`, `on`, `command`), whatever follows it.
  if (state.attrName) {
    state.attrName = false;
    return 'attribute';
  }

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
    state.pendingForm = CATALOG_FORMS.has(w) ? w : null;
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
    // `#[server]`, `#[on(…)]`: a rose of its own — the olive of the shared "annotation" read as grey next to the code, and
    // nothing else in the language is red-ish. A theme without `--syntax-attribute` gets the shared annotation colour.
    { tag: attribute, color: 'var(--syntax-attribute, var(--syntax-annotation, #bbb529))' },
    // A type a line declares, as a `fn` name is: the type colour, bolder.
    { tag: t.definition(t.typeName), color: 'var(--syntax-type, #4d9be6)', fontWeight: '600' },
    { tag: t.labelName, color: 'var(--syntax-label, var(--text-secondary))', fontStyle: 'italic' },
    // The id in `t!("ui:key")` and in `block!("tnt")`: a string, but one the game checks — apart from the text around it. A theme without
    // `--syntax-text-key` / `--syntax-catalog-id` gets the string colour pulled toward the type's / the constant's.
    { tag: textKey, color: 'var(--syntax-text-key, color-mix(in srgb, var(--syntax-string, #6a9956) 55%, var(--syntax-type, #4d78cc)))' },
    { tag: catalogId, color: 'var(--syntax-catalog-id, color-mix(in srgb, var(--syntax-string, #6a9956) 55%, var(--syntax-constant, #9876aa)))' },
    // An interpolation hole `"{p.x}"`: the name reads as code, the braces as the keyword colour.
    { tag: holePath, color: 'var(--syntax-variable, var(--text-primary))' },
    { tag: holeBrace, color: 'var(--syntax-keyword, #cc7832)', fontWeight: '600' },
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

/**
 * Completion for `.ptn`: the backend answers *what*, this chooses *how the token is read* — which depends on where the caret is.
 *
 * - in the text of a string: an id (`ui:avvio.solo`) is one token with its `:` and `.`, asked everywhere inside the quotes;
 * - in an interpolation hole `"{p.x}"`: a plain identifier, asked everywhere (a `{` is the trigger);
 * - anywhere else: the usual source.
 *
 * Without this the default source read `con` as the word in `t!("ui:con`, replaced only that, and left `ui:ui:conta`.
 */
const patinaCompletionSource: CompletionSource = (ctx: CompletionContext): Promise<CompletionResult | null> | CompletionResult | null => {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const where = stringContext(line.text.slice(0, ctx.pos - line.from));
  if (where === 'text') return keyBackendCompletionSource(ctx);
  if (where === 'hole') return eagerBackendCompletionSource(ctx);
  return backendCompletionSource(ctx);
};

/** The `.ptn` language: shape from here, truth from the `language-intel` extension. */
export const patinaLanguage: LanguageDescriptor = {
  id: 'patina',
  createParser: () => Promise.reject(new Error('patina: highlighted by a stream mode')),
  classify: () => null,
  cmExtension: [StreamLanguage.define(patinaMode), patinaHighlight, foldService.of(patinaExtraFold), braceFold, patinaIndent],
  // `use …` collassa da solo all'apertura (impostazione "fold imports"), come gli import di Java.
  importFold: useBlockRange,
  // The gutter: the ranges come from `braceFold` above, not from a grammar.
  cmFold: true,
  serverFold: false,
  commentTokens: { line: '//' },
  // Ctrl/Cmd-hover: the one name under the pointer, not the whole `a.b` / `a::b` chain.
  linkRange: patinaLinkRange,
  // A Rust-shaped language: completion after a dot, a `::`, or a word — the default backend source's gate.
  intel: { completion: patinaCompletionSource, hover: backendHoverSource },
};
