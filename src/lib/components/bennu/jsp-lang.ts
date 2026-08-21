/**
 * Bennu ↔ tree-sitter-jsp bridge: the JSP {@link LanguageDescriptor} for the shared
 * code-editor core. Replaces the old `@codemirror/lang-html` + regex overlay, which
 * mis-tagged namespaced taglib CLOSING tags (`</s:iterator>`, `</c:if>`) as invalid /
 * untagged (the dark-red / white bug).
 *
 * A small custom tree-sitter grammar (`crates/products/bennu/jsp-grammar`, compiled to
 * `static/bennu/tree-sitter-jsp.wasm`) parses JSP natively: namespaced tags, scriptlets
 * `<% … %>` / directives `<%@ … %>` / declarations / expressions, JSP comments, EL
 * `${…}` / `#{…}` and Struts OGNL `%{…}` — including inside attribute values. Highlighting
 * is leaf-driven by {@link classify} (no `.scm` query), exactly like `java-lang.ts`.
 *
 * The expression languages are **decomposed by the grammar** rather than lexed here: a path is
 * a subtree, and operators, literals and whitespace are its siblings. That is what lets a
 * structural search say `%{#session.$prop$}`, and it is why the EL/OGNL stream parser this file
 * used to inject is gone.
 *
 * Taglib tags are coloured **per library** rather than all alike, each matching its own
 * `<%@ taglib %>` declaration — see `jsp-taglibs.ts` for why and how.
 *
 * If the wasm is missing the parser factory rejects and the editor stays plain text
 * (graceful — no crash), like the Java + merula descriptors.
 */

import { Parser, Language, type Node } from 'web-tree-sitter';
import { css } from '@codemirror/legacy-modes/mode/css';
import type { StreamParser } from '@codemirror/language';
import type { LanguageDescriptor, TokenClassName } from '$lib/components/shared/ui/code-editor';
import { javascriptStream, namespaceTokenClass } from '$lib/components/shared/ui/code-editor';
import { directiveSlot, tagSlot } from './jsp-taglibs';
import { makeHoverSource } from './bennu-hover';
import { markupCompletionSource, markupExtHover } from './markup-intel';
import { jspScriptCompletion, jspScriptHover } from './jsp-script-intel';
import { actionPropertyHover } from '$lib/ipc/bennu/nav';

const RUNTIME_WASM = '/bennu/tree-sitter.wasm';
const GRAMMAR_WASM = '/bennu/tree-sitter-jsp.wasm';

let langPromise: Promise<Language> | null = null;

/** Load the JSP grammar once per window (shared {@link Language}, cheap per-editor
 *  parsers). Rejects if either `.wasm` is missing. */
function initJspLang(): Promise<Language> {
  if (!langPromise) {
    langPromise = Parser.init({
      locateFile: (file: string) =>
        file.endsWith('tree-sitter.wasm') ? RUNTIME_WASM : file,
    }).then(() => Language.load(GRAMMAR_WASM));
  }
  return langPromise;
}

async function createJspParser(): Promise<Parser> {
  const lang = await initJspLang();
  const parser = new Parser();
  parser.setLanguage(lang);
  return parser;
}

// ── Leaf classification (tree-sitter-jsp node type → highlight class) ───────────
//
// The highlighter recurses containers and calls this per LEAF. Named leaves carry the
// grammar rule name as `type`; anonymous leaves (quotes, brackets, `=`) carry their
// literal text. The `<% … %>` family + comments are single leaf tokens, so their whole
// span colours at once.

/** The whole `<% … %>` scriptlet / declaration / expression family — the JSP "meta"
 *  colour (olive), matching the previous overlay. `jsp_directive` is handled apart: a
 *  `taglib` one wears its prefix's colour instead (see `jsp-taglibs.ts`). */
const SCRIPTLET_TYPES = new Set([
  'jsp_scriptlet', 'jsp_declaration', 'jsp_expression',
]);

/** Punctuation leaves (anonymous). */
const PUNCT = new Set([
  '<', '>', '</', '/>', '=', '/',
  // The expression languages: their delimiters, their grouping and their separators.
  '${', '#{', '%{', '}', '[', ']', '(', ')', ',',
]);

/** Anonymous leaves inside an expression that join rather than separate. */
const EL_OPERATOR_PUNCT = new Set(['.', ':', '@']);

/** The words EL and OGNL reserve. They are `el_identifier` nodes to the grammar — telling a
 *  keyword from a name is a job for a vocabulary, not for a parser, and putting the vocabulary
 *  here keeps the grammar from having to guess whether `${empty}` is a name. */
const EL_KEYWORDS = new Set([
  'and', 'or', 'not', 'eq', 'ne', 'lt', 'gt', 'le', 'ge', 'div', 'mod', 'empty', 'instanceof',
  'in', 'new',
]);

/** …and the ones that are values. */
const EL_ATOMS = new Set(['true', 'false', 'null']);

function classify(
  node: Node,
  named: boolean,
  _field: string | null,
  _parentType: string | null,
): TokenClassName | null {
  const type = node.type;

  // Named leaves (grammar rule names).
  if (type === 'jsp_comment' || type === 'html_comment') return 'comment';
  if (SCRIPTLET_TYPES.has(type)) return 'annotation';
  // A `taglib` directive is the legend for every `<prefix:…>` below it, so it wears the
  // prefix's own colour; every other directive (`page`, `include`) stays JSP-meta olive.
  if (type === 'jsp_directive') {
    const slot = directiveSlot(node);
    return slot === undefined ? 'annotation' : namespaceTokenClass(slot);
  }
  // ── inside `${…}` / `#{…}` / `%{…}` ──────────────────────────────────────────
  //
  // The grammar decomposes an expression now (a path is a subtree; operators, literals and
  // whitespace are its siblings), so these are real nodes rather than one token handed to a
  // stream lexer. Same colours as that lexer produced — the point of the change is what can be
  // *asked* of the tree, not a new palette.
  if (type === 'el_identifier') {
    const word = node.text;
    if (EL_ATOMS.has(word)) return 'constant';
    if (EL_KEYWORDS.has(word.toLowerCase())) return 'keyword';
    return 'ident';
  }
  if (type === 'el_property') return 'field';
  if (type === 'el_number') return 'number';
  if (type === 'el_string') return 'string';
  if (type === 'el_operator') return 'operator';
  // A character the expression language has no meaning for. Left plain rather than guessed at —
  // it is what a half-typed line is made of.
  if (type === 'el_other') return null;

  // A namespaced tag whose prefix this page declares is coloured by LIBRARY (`<s:…>`
  // apart from `<c:…>` apart from `<wp:…>`), matching its own `<%@ taglib %>` line.
  // Plain HTML — and a prefix nobody declared — keeps the ordinary tag colour.
  if (type === 'tag_name') {
    const slot = tagSlot(node);
    return slot === undefined ? 'keyword' : namespaceTokenClass(slot);
  }
  if (type === 'script_tag' || type === 'style_tag') return 'keyword';
  // `script_content` / `style_content` are highlighted by the JS/CSS injection (below),
  // not classify — leaving them here would flatten them to one colour.
  if (type === 'attribute_name') return 'field';
  if (type === 'attribute_fragment' || type === 'attribute_fragment_sq') return 'string';
  if (type === 'doctype' || type === 'cdata') return 'comment';

  // Anonymous leaves (quotes, brackets, `=`).
  if (!named) {
    if (type === '"' || type === "'") return 'string';
    // The OGNL context sigil. Its own colour, so `#session` reads as "not a property of the
    // action" at a glance — which is exactly what the `#` means.
    if (type === '#') return 'annotation';
    if (EL_OPERATOR_PUNCT.has(type)) return 'operator';
    if (PUNCT.has(type)) return 'punctuation';
  }

  return null; // text / whitespace / stray → plain
}

// ── Folding (JSP block families + HTML comments) ────────────────────────────────
//
// Flat tag model (no element nesting), so tag BODIES don't fold; we fold the multi-line
// block constructs — scriptlets, declarations, JSP + HTML comments — from the end of
// their opener to just before their closer.

const FOLD_TYPES = new Set(['jsp_scriptlet', 'jsp_declaration', 'jsp_comment', 'html_comment']);

function foldNode(node: Node): { from: number; to: number } | null {
  if (!FOLD_TYPES.has(node.type)) return null;
  // Keep the opener on-screen; fold from just after it to just before the closer.
  const open = node.type === 'jsp_comment' ? 4 : node.type === 'html_comment' ? 4 : 2; // `<%--`/`<!--`/`<%`
  const close = node.type === 'jsp_comment' ? 4 : node.type === 'html_comment' ? 3 : 2; // `--%>`/`-->`/`%>`
  const from = node.startIndex + open;
  const to = node.endIndex - close;
  return to > from ? { from, to } : null;
}

/** The JSP {@link LanguageDescriptor} handed to the shared `CodeEditor`. */
export const jspLanguage: LanguageDescriptor = {
  id: 'jsp',
  createParser: createJspParser,
  classify,
  foldNode,
  // JSP comments (`<%-- … --%>`) are the safe universal toggle: unlike an HTML comment
  // they're stripped server-side, so commenting a line never ships markup to the client.
  commentTokens: { block: { open: '<%--', close: '--%>' } },
  // Embedded highlighting via stream parsers, for the two bodies the grammar hands over as raw
  // text: `<script>` → JavaScript, `<style>` → CSS.
  //
  // EL and OGNL are **not** here any more. They used to be, because they were single tokens and
  // a lexer was the only way to see inside one; the grammar decomposes them now, so `classify`
  // colours their real nodes. Which is not merely tidier: an injection fires on a **leaf**, so
  // leaving these listed would have been an injection that never runs and an expression that
  // renders plain.
  injections: {
    script_content: javascriptStream as unknown as StreamParser<unknown>,
    style_content: css as unknown as StreamParser<unknown>,
  },
  // Completion + hover come from the page's own tag libraries — the TLDs its `<%@ taglib %>`
  // directives resolve to, read out of the project and out of the dependency jars. The
  // mechanics are shared with the XML descriptor (`markup-intel.ts`); the vocabulary is not.
  //
  // Hover asks the libraries first and falls back to the action-property resolver: the two
  // answer disjoint positions (a tag or attribute NAME versus an OGNL root / form field), so
  // the order only decides which of them gets asked about a token neither knows.
  //
  // A `<script>` body is answered before either of them, by `jsp-script-intel.ts`: it is
  // JavaScript, no language server will ever serve a JSP, and the taglib vocabulary has nothing
  // to say about it. First and not last, because a `<` in JavaScript — `if (a < b)` — reads to
  // the markup tokenizer as an unclosed tag, so the taglib list used to appear in the middle of
  // a comparison.
  intel: {
    completion: (ctx) => jspScriptCompletion(ctx) ?? markupCompletionSource(ctx),
    hover: (view, pos, side) =>
      jspScriptHover(view, pos)
      ?? makeHoverSource(async (path, src, byteOffset) =>
        (await markupExtHover(path, src, byteOffset))
        ?? (await actionPropertyHover(path, src, byteOffset)))(view, pos, side),
  },
};
