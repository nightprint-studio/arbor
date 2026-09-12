/**
 * Prism grammars for Jinja code templates — `jinja` for a template on its own, `jinja-java` (and
 * `java.jinja`, …) for one that writes another language. The names are `jinjaFences`'.
 *
 * The editor colours a template with a stream parser that hands each stretch of a line to the tokenizer
 * it belongs to. A template shown anywhere else — a page of the docs, a fenced block — has only Prism,
 * which tokenizes with one grammar at a time, so the two layers are fitted to it twice:
 *
 * - **as a grammar**, the tags are the first rules and the generated language's follow. That is what a
 *   bare `Prism.tokenize` gets, and it has one blind spot: a Java comment or string with a tag in it is
 *   cut in pieces, because the tag was taken first.
 * - **through `Prism.highlight`**, a hook does better: each tag becomes a one-character placeholder, the
 *   text is tokenized as the generated language alone — so `// {{ name }} is the key` is one comment —
 *   and the placeholders are put back as tags wherever they landed, inside a comment or a string too.
 *   It is Prism's own `markup-templating` idea, which cannot be used as it is: its placeholder spells the
 *   language id, and Java's `-` splits `___JINJA-JAVA0___` into three tokens it never finds again.
 */

import Prism from 'prismjs';
// Loaded here rather than left to `prism-shared`: `diff-formatter` imports this folder first, and a
// grammar built from one that is not registered yet has no rules. `markup` and `javascript` are core.
import 'prismjs/components/prism-java';
import 'prismjs/components/prism-kotlin';
import 'prismjs/components/prism-yaml';
import 'prismjs/components/prism-properties';
import 'prismjs/components/prism-toml';
import 'prismjs/components/prism-python';
import { JINJA_CONSTANTS, JINJA_TAGS, JINJA_WORD_OPERATORS, jinjaFences } from '../jinja-words';

/** Each generated language's Prism grammar. */
const PRISM_OF: Record<string, string> = {
  java: 'java', kotlin: 'kotlin', xml: 'markup', html: 'markup', yaml: 'yaml',
  properties: 'properties', toml: 'toml', python: 'python', javascript: 'javascript',
  rust: 'rust', sql: 'sql', shell: 'bash', go: 'go', lua: 'lua', css: 'css', json: 'json',
};

const words = (list: readonly string[]) => new RegExp(`\\b(?:${list.join('|')})\\b`);

/** Inside `{{ }}` and `{% %}` — the classes the editor's own tokenizer gives. */
const TAG: Prism.Grammar = {
  // Before `delimiter`, which would otherwise take the `{%` this lookbehind needs.
  statement: {
    pattern: new RegExp(`(^\\{%[-+]?\\s*)(?:${JINJA_TAGS.join('|')})\\b`),
    lookbehind: true,
    alias: 'keyword',
  },
  delimiter: { pattern: /^\{[{%][-+]?|[-+]?[}%]\}$/, alias: 'punctuation' },
  string: { pattern: /(["'])(?:\\.|(?!\1)[^\\\r\n])*\1/, greedy: true },
  filter: { pattern: /(\|\s*)[A-Za-z_]\w*/, lookbehind: true, alias: 'function' },
  test: { pattern: /(\bis\s+(?:not\s+)?)(?!not\b)[A-Za-z_]\w*/, lookbehind: true, alias: 'builtin' },
  function: /\b[A-Za-z_]\w*(?=\s*\()/,
  keyword: words(JINJA_WORD_OPERATORS),
  boolean: words(JINJA_CONSTANTS),
  property: { pattern: /(\.)[A-Za-z_]\w*/, lookbehind: true },
  number: /\b\d+(?:\.\d+)?\b/,
  operator: /\*\*|\/\/|[=!<>]=|[-+*/%~<>=|]/,
  punctuation: /[{}[\](),.:]/,
};

const COMMENT = /\{#[\s\S]*?#\}/;
const EXPRESSION_OR_STATEMENT = /\{\{[\s\S]*?\}\}|\{%[\s\S]*?%\}/;
const ANY_TAG = /\{#[\s\S]*?#\}|\{\{[\s\S]*?\}\}|\{%[\s\S]*?%\}/g;

/** Private-use code points: no grammar matches one, and a single character is something no rule can split. */
const FIRST_PLACEHOLDER = 0xe000;
const PLACEHOLDERS = 0xf8ff - FIRST_PLACEHOLDER + 1;
const PLACEHOLDER = /[\uE000-\uF8FF]/;

function tagToken(text: string): Prism.Token {
  return text.startsWith('{#')
    ? new Prism.Token('jinja-comment', text, 'comment', text)
    : new Prism.Token('jinja-tag', Prism.tokenize(text, TAG), undefined, text);
}

function templated(inner: Prism.Grammar): Prism.Grammar {
  return {
    'jinja-comment': { pattern: COMMENT, greedy: true, alias: 'comment' },
    'jinja-tag': { pattern: EXPRESSION_OR_STATEMENT, greedy: true, inside: TAG },
    ...inner,
  };
}

/** Each template grammar, to the generated language's own that the hook tokenizes the text with. */
const INNER = new Map<Prism.Grammar, Prism.Grammar>();

for (const { inner, names } of jinjaFences()) {
  const own = (inner && Prism.languages[PRISM_OF[inner]]) || {};
  const grammar = templated(own);
  INNER.set(grammar, own);
  for (const name of names) Prism.languages[name] = grammar;
}

Prism.hooks.add('before-tokenize', (env) => {
  const inner = INNER.get(env.grammar);
  // Text that already holds a private-use character keeps the plain grammar: a placeholder could not be
  // told from it.
  if (!inner || PLACEHOLDER.test(env.code)) return;
  const tags = env.code.match(ANY_TAG) ?? [];
  if (tags.length === 0 || tags.length > PLACEHOLDERS) return;
  let next = 0;
  env.code = env.code.replace(ANY_TAG, () => String.fromCharCode(FIRST_PLACEHOLDER + next++));
  env.grammar = inner;
  env.jinjaTags = tags;
});

Prism.hooks.add('after-tokenize', (env) => {
  const tags: string[] | undefined = env.jinjaTags;
  if (tags) env.tokens = withTags(env.tokens, tags);
});

/** The tokens with every placeholder turned back into its tag, at whatever depth it landed. */
function withTags(stream: Prism.TokenStream, tags: string[]): Prism.TokenStream {
  if (typeof stream === 'string') return split(stream, tags);
  if (!Array.isArray(stream)) {
    stream.content = withTags(stream.content, tags);
    return stream;
  }
  return stream.flatMap((token) => {
    if (typeof token !== 'string') {
      token.content = withTags(token.content, tags);
      return [token];
    }
    const parts = split(token, tags);
    return typeof parts === 'string' ? [parts] : parts;
  });
}

function split(text: string, tags: string[]): string | Array<string | Prism.Token> {
  if (!PLACEHOLDER.test(text)) return text;
  return text
    .split(/([\uE000-\uF8FF])/)
    .filter((part) => part !== '')
    .map((part) => (PLACEHOLDER.test(part) ? tagToken(tags[part.charCodeAt(0) - FIRST_PLACEHOLDER]) : part));
}
