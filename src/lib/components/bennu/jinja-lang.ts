/**
 * Jinja templates — `*.jinja`, `*.jinja2`, `*.j2` — coloured as the language they generate, with the
 * template tags on top.
 *
 * The language comes from the name: `OrderTest.java.jinja` is Java with Jinja in it, so the text
 * outside `{{ }}`, `{% %}` and `{# #}` goes to a Java tokenizer and the tags to a Jinja one. One stream
 * parser does both, handing each stretch of a line to the tokenizer it belongs to — what CodeMirror
 * 5 called a multiplexing mode — and keeping the inner language's state across a tag, so a Java
 * string or comment that a tag interrupts is still one when the tag closes.
 *
 * Why a stream parser and not a grammar: a template is valid in neither language, and it is the
 * template that gets edited. A Java parser reads every `{%` as an error, a Jinja one every line of
 * Java as opaque text; handing off token by token reads both as what they are.
 */

import { LanguageDescription, LanguageSupport, StreamLanguage, type StreamParser, type StringStream } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { Decoration, EditorView, MatchDecorator, ViewPlugin, type DecorationSet, type ViewUpdate } from '@codemirror/view';
import { java, kotlin } from '@codemirror/legacy-modes/mode/clike';
import { html, xml } from '@codemirror/legacy-modes/mode/xml';
import { yaml } from '@codemirror/legacy-modes/mode/yaml';
import { properties } from '@codemirror/legacy-modes/mode/properties';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { python } from '@codemirror/legacy-modes/mode/python';
import { rust } from '@codemirror/legacy-modes/mode/rust';
import { standardSQL } from '@codemirror/legacy-modes/mode/sql';
import { shell } from '@codemirror/legacy-modes/mode/shell';
import { go } from '@codemirror/legacy-modes/mode/go';
import { lua } from '@codemirror/legacy-modes/mode/lua';
import { css } from '@codemirror/legacy-modes/mode/css';
import { javascriptStream } from '$lib/components/shared/ui/code-editor';
import {
  JINJA_CONSTANTS,
  JINJA_TAGS,
  JINJA_WORD_OPERATORS,
  jinjaFences,
} from '$lib/utils/jinja-words';

const TAGS = new Set<string>(JINJA_TAGS);
const WORD_OPERATORS = new Set<string>(JINJA_WORD_OPERATORS);
const CONSTANTS = new Set<string>(JINJA_CONSTANTS);
/** Statements whose next names are being defined: `{% set x %}`, `{% for a, b in … %}`, `{% macro m() %}`. */
const DEFINING = new Set(['set', 'for', 'macro', 'call', 'with']);

const INNER_PARSERS: Record<string, StreamParser<unknown>> = {
  java, kotlin, xml, html, yaml, properties, toml, python, rust, shell, go, lua, css,
  sql: standardSQL,
  javascript: javascriptStream as unknown as StreamParser<unknown>,
  // JSON is JavaScript as far as a tokenizer is concerned, and the JS mode is already here.
  json: javascriptStream as unknown as StreamParser<unknown>,
};

type Opener = '{{' | '{%' | '{#';

interface JinjaState {
  /** Inside a tag, and which kind. */
  block: Opener | null;
  /** The inner language's own state. */
  inner: unknown;
  /** The last token was `|`, so a name is a filter. */
  afterPipe: boolean;
  /** The last word was `is`, so a name is a test. */
  afterIs: boolean;
  /** Nothing has been read since `{%`, so a name is the statement. */
  tagStart: boolean;
  /** Names being defined by the statement. */
  defining: boolean;
}

/**
 * Jinja as the language of a fenced code block in Markdown, under the names `jinjaFences` gives.
 *
 * Only the tokens: a fence is coloured by its language alone, so the tag marks the editor lays over a
 * template tab stay with the tab.
 */
export function jinjaFenceLanguages(): LanguageDescription[] {
  return jinjaFences().map(({ inner, names: [name, ...alias] }) =>
    LanguageDescription.of({
      name,
      alias,
      support: new LanguageSupport(StreamLanguage.define(jinjaStream(INNER_PARSERS[inner] ?? null))),
    }),
  );
}

/** The CodeMirror extension for a template generating `inner` (a key of the table above, or `''`). */
export function jinjaExtension(inner: string): Extension {
  return [StreamLanguage.define(jinjaStream(INNER_PARSERS[inner] ?? null)), tagMarks, tagTheme];
}

function jinjaStream(inner: StreamParser<unknown> | null): StreamParser<JinjaState> {
  return {
    name: 'jinja',
    startState: (indentUnit) => ({
      block: null,
      inner: inner?.startState?.(indentUnit) ?? null,
      afterPipe: false,
      afterIs: false,
      tagStart: false,
      defining: false,
    }),
    copyState: (state) => ({ ...state, inner: copyInner(inner, state.inner) }),
    token: (stream, state) => (state.block ? tokenInTag(stream, state) : tokenOutside(stream, state, inner)),
    blankLine: (state, indentUnit) => {
      if (!state.block) inner?.blankLine?.(state.inner, indentUnit);
    },
    indent: (state, textAfter, context) =>
      state.block || !inner?.indent ? null : inner.indent(state.inner, textAfter, context),
    languageData: { commentTokens: { block: { open: '{#', close: '#}' } } },
  };
}

/** The same copy CodeMirror makes of a stream state that brings no `copyState` of its own. */
function copyInner(parser: StreamParser<unknown> | null, state: unknown): unknown {
  if (!parser || state === null || typeof state !== 'object') return state;
  if (parser.copyState) return parser.copyState(state);
  const copy: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(state as Record<string, unknown>)) {
    copy[key] = Array.isArray(value) ? value.slice() : value;
  }
  return copy;
}

function tokenOutside(stream: StringStream, state: JinjaState, inner: StreamParser<unknown> | null): string | null {
  if (stream.match(/^\{[{%#]-?/)) {
    const opener = stream.current().slice(0, 2) as Opener;
    state.block = opener;
    reset(state);
    state.tagStart = opener === '{%';
    return opener === '{#' ? 'comment' : 'meta';
  }
  const ahead = stream.string.slice(stream.pos).search(/\{[{%#]/);
  if (!inner) {
    if (ahead < 0) stream.skipToEnd();
    else stream.pos += Math.max(ahead, 1);
    return null;
  }
  // The inner tokenizer sees the line only up to the next tag, so no token of its can run into one.
  const line = stream.string;
  if (ahead > 0) stream.string = line.slice(0, stream.pos + ahead);
  let style: string | null = null;
  try {
    style = inner.token(stream, state.inner);
  } finally {
    stream.string = line;
  }
  if (stream.pos === stream.start) stream.next();
  return style;
}

function tokenInTag(stream: StringStream, state: JinjaState): string | null {
  if (state.block === '{#') {
    const end = stream.string.indexOf('#}', stream.pos);
    if (end < 0) {
      stream.skipToEnd();
    } else {
      stream.pos = end + 2;
      state.block = null;
    }
    return 'comment';
  }
  if (stream.eatSpace()) return null;
  const close = state.block === '{{' ? '}}' : '%}';
  if (stream.match(`-${close}`) || stream.match(close)) {
    state.block = null;
    reset(state);
    return 'meta';
  }
  if (stream.match(/^"(?:[^"\\]|\\.)*"?/) || stream.match(/^'(?:[^'\\]|\\.)*'?/)) {
    reset(state);
    return 'string';
  }
  if (stream.match(/^\d+(?:\.\d+)?/)) {
    reset(state);
    return 'number';
  }
  if (stream.match(/^[A-Za-z_]\w*/)) return word(stream, state);
  const ch = stream.next() ?? '';
  state.tagStart = false;
  if (ch === '|') {
    state.afterPipe = true;
    state.defining = false;
    return 'operator';
  }
  // `for a, b in` defines both names; anything else ends the names being defined.
  if (ch !== ',') state.defining = false;
  return '+-*/%~=<>!'.includes(ch) ? 'operator' : 'punctuation';
}

function word(stream: StringStream, state: JinjaState): string {
  const text = stream.current();
  const { afterPipe, afterIs, tagStart, defining } = state;
  reset(state);
  if (afterPipe) return 'variableName.function';
  if (afterIs && text !== 'not') return 'variableName.standard';
  if (tagStart && TAGS.has(text)) {
    state.defining = DEFINING.has(text);
    return 'keyword';
  }
  if (text === 'is' || (text === 'not' && afterIs)) {
    state.afterIs = true;
    return 'keyword';
  }
  if (WORD_OPERATORS.has(text)) return 'keyword';
  if (CONSTANTS.has(text)) return 'atom';
  if (defining) {
    state.defining = true;
    return 'variableName.definition';
  }
  if (stream.string.charAt(stream.start - 1) === '.') return 'propertyName';
  if (stream.peek() === '(') return 'variableName.function';
  return 'variableName';
}

function reset(state: JinjaState) {
  state.afterPipe = false;
  state.afterIs = false;
  state.tagStart = false;
  state.defining = false;
}

// A tinted ground under each tag, so the template reads as two layers rather than one soup.
const tagMatcher = new MatchDecorator({
  regexp: /\{\{.*?\}\}|\{%.*?%\}|\{#.*?#\}/g,
  decoration: Decoration.mark({ class: 'cm-jinja-tag' }),
});

const tagMarks = ViewPlugin.fromClass(
  class {
    marks: DecorationSet;
    constructor(view: EditorView) {
      this.marks = tagMatcher.createDeco(view);
    }
    update(update: ViewUpdate) {
      this.marks = tagMatcher.updateDeco(update, this.marks);
    }
  },
  { decorations: (plugin) => plugin.marks },
);

const tagTheme = EditorView.baseTheme({
  '.cm-jinja-tag': {
    backgroundColor: 'color-mix(in srgb, var(--syntax-annotation, #bbb529) 9%, transparent)',
    borderRadius: '3px',
  },
});
