/**
 * Studio editor — CodeMirror 6 glue.
 *
 * Shared editor extensions used by `StudioTextPane.svelte` for every Studio
 * format (RON today, JSON/TOML/YAML/.properties as their phases land) and
 * — in the future — by the conflict resolver's third pane.
 *
 * Design rules:
 *  - Theme/colours are pulled from arbor CSS variables so a theme overlay
 *    (`arbor.ui.set_theme_tokens`) re-skins the editor for free.
 *  - Languages are registered lazily per format: keeps the initial bundle
 *    small and lets a format ship its parser without touching this module.
 *  - One factory (`createStudioExtensions`) returns the full extension set
 *    so the Svelte host doesn't need to know about CM internals.
 */

import {
  EditorState,
  Compartment,
  type Extension,
} from '@codemirror/state';
import {
  EditorView,
  keymap,
  highlightActiveLine,
  highlightActiveLineGutter,
  lineNumbers,
  drawSelection,
  rectangularSelection,
  crosshairCursor,
} from '@codemirror/view';
import {
  history,
  defaultKeymap,
  historyKeymap,
  indentWithTab,
} from '@codemirror/commands';
import {
  HighlightStyle,
  StreamLanguage,
  type StreamParser,
  syntaxHighlighting,
  bracketMatching,
  indentOnInput,
  foldGutter,
  foldKeymap,
  LanguageDescription,
  LanguageSupport,
} from '@codemirror/language';
import { json as jsonLang } from '@codemirror/lang-json';
import { toml as legacyTomlParser } from '@codemirror/legacy-modes/mode/toml';
import { yaml as legacyYamlParser } from '@codemirror/legacy-modes/mode/yaml';
import {
  searchKeymap,
  highlightSelectionMatches,
} from '@codemirror/search';
import { tags as t } from '@lezer/highlight';

// ─── Language identifier ────────────────────────────────────────────────
export type StudioLanguage =
  | 'ron'
  | 'json'
  | 'toml'
  | 'yaml'
  | 'properties'
  | 'plain';

// ─── RON stream parser ──────────────────────────────────────────────────
// Lexical subset of Rust: line + block comments, double-quoted strings
// with escapes (and raw `r#"..."#`), char literals, numbers (with `_`
// digit separators and exponents), `true|false`, `Some|None`, identifiers
// (Capitalized → typeName, otherwise propertyName), structural punctuation.

interface RonState {
  inBlockComment: number;
  rawStringHashes: number; // 0 = not in raw, N = number of trailing #s expected
}

const ronParser: StreamParser<RonState> = {
  startState: () => ({ inBlockComment: 0, rawStringHashes: 0 }),

  token(stream, state) {
    // Continue block comment across lines
    if (state.inBlockComment > 0) {
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === '/' && stream.eat('*')) {
          state.inBlockComment++;
        } else if (ch === '*' && stream.eat('/')) {
          state.inBlockComment--;
          if (state.inBlockComment === 0) return 'comment';
        }
      }
      return 'comment';
    }

    // Continue raw string across lines
    if (state.rawStringHashes > 0) {
      const need = state.rawStringHashes;
      while (!stream.eol()) {
        if (stream.peek() === '"') {
          stream.next();
          let found = 0;
          while (found < need && stream.eat('#')) found++;
          if (found === need) {
            state.rawStringHashes = 0;
            return 'string';
          }
        } else {
          stream.next();
        }
      }
      return 'string';
    }

    if (stream.eatSpace()) return null;

    // Line comment
    if (stream.match('//')) {
      stream.skipToEnd();
      return 'comment';
    }

    // Block comment start
    if (stream.match('/*')) {
      state.inBlockComment = 1;
      // Try to close on the same line
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === '/' && stream.eat('*')) state.inBlockComment++;
        else if (ch === '*' && stream.eat('/')) {
          state.inBlockComment--;
          if (state.inBlockComment === 0) return 'comment';
        }
      }
      return 'comment';
    }

    // Raw string: r"..." / r#"..."# / r##"..."## …
    const rawMatch = stream.match(/^r(#*)"/) as RegExpMatchArray | null;
    if (rawMatch) {
      const hashes = rawMatch[1].length;
      while (!stream.eol()) {
        if (stream.peek() === '"') {
          stream.next();
          let found = 0;
          while (found < hashes && stream.eat('#')) found++;
          if (found === hashes) return 'string';
        } else {
          stream.next();
        }
      }
      // String continues on next line
      state.rawStringHashes = hashes;
      return 'string';
    }

    // Normal string
    if (stream.eat('"')) {
      let escaped = false;
      while (!stream.eol()) {
        const c = stream.next();
        if (escaped) { escaped = false; continue; }
        if (c === '\\') { escaped = true; continue; }
        if (c === '"') return 'string';
      }
      return 'string';
    }

    // Char literal
    if (stream.eat("'")) {
      let escaped = false;
      while (!stream.eol()) {
        const c = stream.next();
        if (escaped) { escaped = false; continue; }
        if (c === '\\') { escaped = true; continue; }
        if (c === "'") return 'string';
      }
      return 'string';
    }

    // Number — optional sign, integer or float, optional exponent, `_` separators
    if (stream.match(/^-?\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][-+]?\d+)?/)) {
      return 'number';
    }

    // Identifier / keyword
    if (stream.match(/^[A-Za-z_][A-Za-z0-9_]*/)) {
      const w = stream.current();
      if (w === 'true' || w === 'false') return 'atom';
      if (w === 'Some' || w === 'None') return 'keyword';
      if (/^[A-Z]/.test(w)) return 'typeName';
      return 'propertyName';
    }

    // Punctuation
    if (stream.match(/^[{}()\[\]:,;]/)) return 'punctuation';

    stream.next();
    return null;
  },

  languageData: {
    commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
    closeBrackets: { brackets: ['(', '[', '{', '"'] },
  },
};

// ─── `.properties` stream parser ────────────────────────────────────────
// Java/Spring `.properties` grammar: line-oriented `key=value`. Comments
// open with `#` or `!`; the separator is `=` / `:` / whitespace (first
// run); continuation backslashes at EOL extend the value to the next
// physical line. `\uXXXX` and standard escapes (`\n` / `\r` / `\t` /
// `\\`) are highlighted as escape tokens.

interface PropertiesState {
  /** Lexer phase for the current logical line. */
  phase: 'start' | 'key' | 'after_key' | 'value' | 'continued';
}

const propertiesParser: StreamParser<PropertiesState> = {
  startState: () => ({ phase: 'start' }),

  token(stream, state) {
    // Continuation marker — the previous physical line ended with `\`.
    if (state.phase === 'continued') {
      // Leading whitespace of a continuation line is dropped by the
      // Java spec, but we still tokenise the line as value content.
      if (stream.eatSpace()) return null;
      state.phase = 'value';
    }

    if (state.phase === 'start') {
      if (stream.eatSpace()) return null;
      if (stream.sol() && (stream.peek() === '#' || stream.peek() === '!')) {
        stream.skipToEnd();
        state.phase = 'start';
        return 'comment';
      }
      state.phase = 'key';
    }

    if (state.phase === 'key') {
      // Walk key characters.
      while (!stream.eol()) {
        const ch = stream.peek();
        if (ch === '\\') {
          stream.next();
          if (!stream.eol()) {
            const n = stream.peek();
            if (n === 'u' || n === 'U') {
              // Unicode escape inside a key.
              stream.next();
              for (let i = 0; i < 4 && !stream.eol(); i++) {
                const h = stream.peek();
                if (h && /[0-9a-fA-F]/.test(h)) stream.next(); else break;
              }
              return 'escape';
            }
            stream.next();
            continue;
          }
          continue;
        }
        if (ch === '=' || ch === ':' || ch === ' ' || ch === '\t') break;
        stream.next();
      }
      state.phase = 'after_key';
      return 'propertyName';
    }

    if (state.phase === 'after_key') {
      // Eat whitespace + at most one `=` / `:` separator.
      if (stream.eatSpace()) return null;
      if (stream.peek() === '=' || stream.peek() === ':') {
        stream.next();
        state.phase = 'value';
        return 'operator';
      }
      // No explicit separator — fall through into value.
      state.phase = 'value';
    }

    if (state.phase === 'value') {
      // Look for unicode escapes / backslash escapes.
      if (stream.peek() === '\\') {
        stream.next();
        if (stream.eol()) {
          // End-of-line continuation — mark for next physical line.
          state.phase = 'continued';
          return 'escape';
        }
        const n = stream.peek();
        if (n === 'u' || n === 'U') {
          stream.next();
          for (let i = 0; i < 4 && !stream.eol(); i++) {
            const h = stream.peek();
            if (h && /[0-9a-fA-F]/.test(h)) stream.next(); else break;
          }
          return 'escape';
        }
        if (n === 'n' || n === 'r' || n === 't' || n === '\\') {
          stream.next();
          return 'escape';
        }
        return 'escape';
      }
      // Eat a run of normal characters.
      while (!stream.eol() && stream.peek() !== '\\') {
        stream.next();
      }
      if (stream.eol()) state.phase = 'start';
      return 'string';
    }

    stream.next();
    return null;
  },

  blankLine(state) {
    state.phase = 'start';
  },

  languageData: {
    commentTokens: { line: '#' },
  },
};

// ─── Language registry ──────────────────────────────────────────────────
// Each entry creates the LanguageSupport on first use and caches it.
// Other formats register themselves here as their phases land.

const cache = new Map<StudioLanguage, LanguageSupport | null>();

function makeLanguage(lang: StudioLanguage): LanguageSupport | null {
  if (cache.has(lang)) return cache.get(lang)!;

  let support: LanguageSupport | null = null;
  if (lang === 'ron') {
    support = new LanguageSupport(StreamLanguage.define(ronParser));
  } else if (lang === 'json') {
    // `@codemirror/lang-json` ships the Lezer JSON parser + indent +
    // fold metadata. JSONC support (Phase 3.d) will keep using the
    // same language: JSON Lezer is permissive enough to parse comments
    // as ERROR nodes without disrupting highlighting downstream.
    support = jsonLang();
  } else if (lang === 'toml') {
    // Phase 4.b — TOML stream parser from `@codemirror/legacy-modes`.
    // Recognises tables `[section]` / `[[arr]]`, key=value, comments
    // (`#`), strings (basic + literal + multiline), numbers, booleans,
    // and datetimes. No Lezer parser ships for TOML so the stream
    // parser is the canonical choice — matches what most editors use.
    support = new LanguageSupport(StreamLanguage.define(legacyTomlParser));
  } else if (lang === 'yaml') {
    // Phase 5.a — YAML stream parser from `@codemirror/legacy-modes`.
    // Recognises block + flow scalars, anchors / aliases (`&` / `*`),
    // tags (`!!str`), comments (`#`), document separators (`---`).
    // No Lezer YAML parser ships in the @codemirror namespace today;
    // the stream parser is the standard choice and matches what every
    // CodeMirror-based editor uses.
    support = new LanguageSupport(StreamLanguage.define(legacyYamlParser));
  } else if (lang === 'properties') {
    // Phase 6 — hand-rolled `.properties` stream parser. The grammar
    // is line-oriented: `#` / `!` start a comment; key is everything
    // up to the first unescaped `=` / `:` / whitespace; value spans
    // to EOL with `\` at EOL marking a continuation onto the next
    // physical line. Java `\uXXXX` escapes highlight as escape tokens.
    support = new LanguageSupport(StreamLanguage.define(propertiesParser));
  }

  cache.set(lang, support);
  return support;
}

// Exported in case a caller wants to feed a custom language description
// into a multi-language Studio later (e.g. embedded JSON inside YAML).
export const studioLanguages: LanguageDescription[] = [
  LanguageDescription.of({
    name: 'ron',
    extensions: ['ron'],
    load: async () => new LanguageSupport(StreamLanguage.define(ronParser)),
  }),
  LanguageDescription.of({
    name: 'json',
    extensions: ['json', 'jsonc'],
    load: async () => jsonLang(),
  }),
  LanguageDescription.of({
    name: 'toml',
    extensions: ['toml'],
    load: async () => new LanguageSupport(StreamLanguage.define(legacyTomlParser)),
  }),
  LanguageDescription.of({
    name: 'yaml',
    extensions: ['yaml', 'yml'],
    load: async () => new LanguageSupport(StreamLanguage.define(legacyYamlParser)),
  }),
  LanguageDescription.of({
    name: 'properties',
    extensions: ['properties'],
    load: async () => new LanguageSupport(StreamLanguage.define(propertiesParser)),
  }),
];

// ─── Theme ──────────────────────────────────────────────────────────────
// Pulls every colour from arbor CSS variables so a theme overlay can
// re-skin the editor at runtime. Falls back to the values used in the old
// Prism overlay (`.token.*` rules in app.css) for parity.

export const studioTheme = EditorView.theme(
  {
    '&': {
      height: '100%',
      backgroundColor: 'var(--bg-base)',
      color: 'var(--text-primary)',
      fontFamily: 'var(--font-code)',
      fontSize: '12px',
    },
    '&.cm-focused': {
      outline: 'none',
    },
    '.cm-scroller': {
      fontFamily: 'var(--font-code)',
      lineHeight: '1.55',
      overflow: 'auto',
    },
    '.cm-content': {
      padding: '12px 4px 12px 4px',
      caretColor: 'var(--text-primary)',
    },
    '.cm-line': { padding: '0 8px' },
    '.cm-gutters': {
      backgroundColor: 'var(--bg-overlay)',
      color: 'var(--text-muted)',
      border: 'none',
      borderRight: '1px solid var(--border-subtle)',
      fontFamily: 'var(--font-code)',
    },
    '.cm-lineNumbers .cm-gutterElement': {
      padding: '0 8px 0 12px',
      minWidth: '32px',
    },
    '.cm-activeLineGutter': {
      backgroundColor: 'var(--bg-hover)',
      color: 'var(--text-secondary)',
    },
    '.cm-activeLine': {
      backgroundColor: 'rgba(255,255,255,0.025)',
    },
    '.cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: 'var(--accent-subtle) !important',
    },
    '&.cm-focused .cm-selectionBackground': {
      backgroundColor: 'var(--accent-subtle) !important',
    },
    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--text-primary)' },
    '.cm-matchingBracket, .cm-nonmatchingBracket': {
      outline: '1px solid var(--accent-strong, var(--accent))',
      borderRadius: '2px',
    },
    '.cm-foldPlaceholder': {
      backgroundColor: 'var(--bg-overlay)',
      color: 'var(--text-muted)',
      border: '1px solid var(--border-subtle)',
      borderRadius: '3px',
      padding: '0 4px',
    },
    '.cm-searchMatch': {
      backgroundColor: 'var(--accent-subtle)',
      outline: '1px solid var(--accent)',
    },
    '.cm-searchMatch.cm-searchMatch-selected': {
      backgroundColor: 'var(--accent)',
      color: 'var(--bg-base)',
    },
    '.cm-tooltip': {
      backgroundColor: 'var(--bg-elevated)',
      color: 'var(--text-primary)',
      border: '1px solid var(--border-subtle)',
      borderRadius: '4px',
    },
    '.cm-panels': {
      backgroundColor: 'var(--bg-overlay)',
      color: 'var(--text-primary)',
      borderTop: '1px solid var(--border-subtle)',
    },
    '.cm-panel.cm-search input': {
      backgroundColor: 'var(--bg-base)',
      color: 'var(--text-primary)',
      border: '1px solid var(--border-subtle)',
      borderRadius: '3px',
      padding: '2px 6px',
    },
  },
  { dark: true },
);

// Highlight style — Lezer tags → arbor syntax variables. Matches the
// `.token.*` rules in app.css so colours stay consistent when other parts
// of the app keep using Prism (DiffViewer, etc.).
export const studioHighlightStyle = HighlightStyle.define([
  { tag: t.comment, color: 'var(--syntax-comment, #7a7d85)', fontStyle: 'italic' },
  { tag: t.lineComment, color: 'var(--syntax-comment, #7a7d85)', fontStyle: 'italic' },
  { tag: t.blockComment, color: 'var(--syntax-comment, #7a7d85)', fontStyle: 'italic' },
  { tag: t.string, color: 'var(--syntax-string, #6a9956)' },
  { tag: t.special(t.string), color: 'var(--syntax-string, #6a9956)' },
  { tag: t.character, color: 'var(--syntax-string, #6a9956)' },
  { tag: t.number, color: 'var(--syntax-number, #9876aa)' },
  { tag: t.bool, color: 'var(--syntax-number, #9876aa)' },
  { tag: t.atom, color: 'var(--syntax-number, #9876aa)' },
  { tag: t.null, color: 'var(--text-muted)' },
  { tag: t.keyword, color: 'var(--syntax-keyword, #cc7832)' },
  { tag: t.controlKeyword, color: 'var(--syntax-keyword, #cc7832)' },
  { tag: t.operatorKeyword, color: 'var(--syntax-keyword, #cc7832)' },
  { tag: t.typeName, color: 'var(--syntax-type, #4d78cc)' },
  { tag: t.className, color: 'var(--syntax-type, #4d78cc)' },
  { tag: t.propertyName, color: 'var(--text-primary)' },
  { tag: t.variableName, color: 'var(--text-primary)' },
  { tag: t.function(t.variableName), color: 'var(--syntax-function, #ffc66d)' },
  { tag: t.punctuation, color: 'var(--text-muted)' },
  { tag: t.bracket, color: 'var(--text-muted)' },
  { tag: t.operator, color: 'var(--text-secondary)' },
]);

// ─── Public API ─────────────────────────────────────────────────────────
export interface StudioExtensionsOptions {
  language: StudioLanguage;
  readOnly?: boolean;
  showLineNumbers?: boolean;
  showActiveLine?: boolean;
}

/** Compartments exposed so callers can reconfigure language / readOnly
 *  without tearing the EditorView down. */
export interface StudioCompartments {
  language: Compartment;
  readOnly: Compartment;
}

export function makeStudioCompartments(): StudioCompartments {
  return {
    language: new Compartment(),
    readOnly: new Compartment(),
  };
}

/** Resolve a language to its CM extension (or empty for `plain`). */
export function languageExtension(lang: StudioLanguage): Extension {
  const support = makeLanguage(lang);
  return support ?? [];
}

/** Bundle the static (non-reconfigurable) extensions plus the two
 *  compartments holding language + readOnly. */
export function createStudioExtensions(
  opts: StudioExtensionsOptions,
  compartments: StudioCompartments,
): Extension {
  const {
    language,
    readOnly = false,
    showLineNumbers = true,
    showActiveLine = true,
  } = opts;

  const extensions: Extension[] = [
    studioTheme,
    syntaxHighlighting(studioHighlightStyle),
    history(),
    drawSelection(),
    indentOnInput(),
    bracketMatching(),
    highlightSelectionMatches(),
    rectangularSelection(),
    crosshairCursor(),
    keymap.of([
      ...defaultKeymap,
      ...historyKeymap,
      ...searchKeymap,
      ...foldKeymap,
      indentWithTab,
    ]),
    compartments.language.of(languageExtension(language)),
    compartments.readOnly.of(EditorState.readOnly.of(readOnly)),
  ];

  if (showLineNumbers) {
    extensions.unshift(lineNumbers(), foldGutter());
  }
  if (showActiveLine) {
    extensions.push(highlightActiveLine(), highlightActiveLineGutter());
  }

  return extensions;
}
