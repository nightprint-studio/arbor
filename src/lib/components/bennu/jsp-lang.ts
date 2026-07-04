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
 * If the wasm is missing the parser factory rejects and the editor stays plain text
 * (graceful — no crash), like the Java + merula descriptors.
 */

import { Parser, Language, type Node } from 'web-tree-sitter';
import { javascript } from '@codemirror/legacy-modes/mode/javascript';
import { css } from '@codemirror/legacy-modes/mode/css';
import type { StreamParser } from '@codemirror/language';
import type { LanguageDescriptor, TokenClass } from '$lib/components/shared/ui/code-editor';
import { elOgnlStream } from './jsp-el';

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

/** The whole `<% … %>` scriptlet / directive / declaration / expression family — the
 *  JSP "meta" colour (olive), matching the previous overlay. */
const SCRIPTLET_TYPES = new Set([
  'jsp_scriptlet', 'jsp_directive', 'jsp_declaration', 'jsp_expression',
]);

/** Punctuation leaves (anonymous). */
const PUNCT = new Set(['<', '>', '</', '/>', '=', '/']);

function classify(
  node: Node,
  named: boolean,
  _field: string | null,
  _parentType: string | null,
): TokenClass | null {
  const type = node.type;

  // Named leaves (grammar rule names).
  if (type === 'jsp_comment' || type === 'html_comment') return 'comment';
  if (SCRIPTLET_TYPES.has(type)) return 'annotation';
  // `el_expression` / `ognl_expression` are tokenized INSIDE by the EL/OGNL injection
  // (below), not flattened to one `field` colour — leaving a classify here would win only
  // if the injection were removed, so it's intentionally omitted.
  if (type === 'tag_name' || type === 'script_tag' || type === 'style_tag') return 'keyword';
  // `script_content` / `style_content` are highlighted by the JS/CSS injection (below),
  // not classify — leaving them here would flatten them to one colour.
  if (type === 'attribute_name') return 'field';
  if (type === 'attribute_fragment' || type === 'attribute_fragment_sq') return 'string';
  if (type === 'doctype' || type === 'cdata') return 'comment';

  // Anonymous leaves (quotes, brackets, `=`).
  if (!named) {
    if (type === '"' || type === "'") return 'string';
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
  // Embedded highlighting via legacy-mode stream parsers (highlight only, no autocomplete):
  //  - `<script>` body → JavaScript, `<style>` body → CSS;
  //  - EL `${…}` / `#{…}` and OGNL `%{…}` bodies → a small EL/OGNL lexer, so identifiers,
  //    property accesses, strings, numbers, operators and delimiters each get their own
  //    colour instead of a single flat purple blob.
  injections: {
    script_content: javascript as unknown as StreamParser<unknown>,
    style_content: css as unknown as StreamParser<unknown>,
    el_expression: elOgnlStream as unknown as StreamParser<unknown>,
    ognl_expression: elOgnlStream as unknown as StreamParser<unknown>,
  },
  // intel: EL / action / taglib completion + hover — reserved for a later wave.
};
