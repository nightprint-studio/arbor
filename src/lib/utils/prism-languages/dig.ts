/**
 * Prism grammar for **`.dig`** — geode's mole-scripting language.
 *
 * Bennu highlights a `.dig` buffer from the real tree-sitter grammar; a fenced
 * ```dig block in a markdown document has no parser (and no wasm) behind it, so it needs a
 * Prism grammar like every other fence. This is that second, lexical view of the same
 * language — approximate by construction, and only ever a colour.
 *
 * **The vocabulary is not copied.** Keywords, builtins and namespaces come from
 * {@link DIG_CATALOG}, the file generated out of geode's own `lang.toml` (a test over there pins
 * the two together). A hand-written list here would be a second vocabulary to keep in step with
 * a language that is still growing, and it would drift on the first builtin added.
 *
 * Python-shaped: `#` comments, `"` strings, indentation blocks, `fn` / `struct` headers.
 */

import Prism from 'prismjs';

import { DIG_CATALOG } from '$lib/components/bennu/dig/catalog';

/** `a|b|c`, longest first so `else` cannot be eaten by `el`. */
function alternation(words: string[]): string {
  return [...words].sort((a, b) => b.length - a.length).join('|');
}

const KEYWORDS = alternation(Object.keys(DIG_CATALOG.keywords));
const BUILTINS = alternation(Object.keys(DIG_CATALOG.builtins));
const NAMESPACES = alternation(Object.keys(DIG_CATALOG.namespaces));

Prism.languages.dig = {
  comment: { pattern: /#.*/, greedy: true },
  string: { pattern: /"(?:\\.|[^"\\\r\n])*"/, greedy: true },
  'class-name': [
    // `Crystal.Amethyst`, `Tool.Pick` — the namespace and the member read as one thing, because
    // that is how the language uses them: a bare `Amethyst` means nothing.
    {
      pattern: new RegExp(`\\b(?:${NAMESPACES})\\.[A-Za-z_]\\w*`),
      alias: 'constant',
    },
    // A bare capitalised name is a type — `crystal: Crystal`, `-> Crystal`. After the dotted
    // rule, which would otherwise lose its member half to this one.
    { pattern: /\b[A-Z]\w*\b/ },
  ],
  // The declaration's own name, so `fn crystal_to_plant` reads as a definition rather than as
  // two words.
  function: [
    { pattern: /\b(?:fn|struct)\s+[A-Za-z_]\w*/, inside: { keyword: /^\w+/ } },
    { pattern: /\b[A-Za-z_]\w*(?=\s*\()/ },
  ],
  keyword: new RegExp(`\\b(?:${KEYWORDS})\\b`),
  builtin: new RegExp(`\\b(?:${BUILTINS})\\b`),
  boolean: /\b(?:true|false|none)\b/,
  number: /\b\d+(?:\.\d+)?\b/,
  operator: /->|[=!<>]=|[-+*/%<>=]/,
  punctuation: /[(){}[\],:.]/,
};
