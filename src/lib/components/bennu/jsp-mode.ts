/**
 * A JSP-aware CodeMirror stream parser.
 *
 * `@codemirror/legacy-modes` has no JSP mode, and the plain HTML mode leaves JSP
 * scriptlets uncolored (rendering `<%-- … --%>` comments as markup) and does NOT
 * switch into JavaScript / CSS inside `<script>` / `<style>`. This wraps the legacy
 * HTML parser and adds:
 *
 *   • JSP `<% … %>` family before delegating the surrounding markup to HTML:
 *       `<%-- … --%>`     → comment
 *       `<%@ … %>`        → directive    (meta)
 *       `<%! … %>`        → declaration  (meta)
 *       `<%= … %>`        → expression   (meta)
 *       `<%  … %>`        → scriptlet    (meta)
 *   • `<![CDATA[ … ]]>` → delimiters as meta, body opaque (the `<!--//--><![CDATA[…]]>`
 *       guard's `//`-commented lines then read as embedded-JS comments).
 *   • Embedded `<script> … </script>` → the legacy JavaScript mode.
 *   • Embedded `<style> … </style>`   → the legacy CSS mode.
 *
 * A block / embedded region may span lines: its kind is latched in the state until the
 * terminator. Everything else is handed verbatim to the HTML mode. No new dependency —
 * the HTML/JS/CSS legacy modes are already used by `languages.ts`.
 */

import type { StreamParser, StringStream } from '@codemirror/language';
import { html } from '@codemirror/legacy-modes/mode/xml';
import { javascript } from '@codemirror/legacy-modes/mode/javascript';
import { css } from '@codemirror/legacy-modes/mode/css';

const htmlMode = html as StreamParser<unknown>;
const jsMode = javascript as StreamParser<unknown>;
const cssMode = css as StreamParser<unknown>;

// Non-greedy scan to a block terminator on the current line (StringStream.match only
// sees the remaining line, so a missing terminator falls through to the multi-line path).
const RE_COMMENT_END = /^[\s\S]*?--%>/;
const RE_SCRIPTLET_END = /^[\s\S]*?%>/;
const RE_SCRIPT_OPEN = /^<script\b[^>]*>/i;
const RE_STYLE_OPEN = /^<style\b[^>]*>/i;
const RE_SCRIPT_CLOSE = /^<\/script\s*>/i;
const RE_STYLE_CLOSE = /^<\/style\s*>/i;

/** An active embedded region (its sub-mode + that mode's own state). */
interface Sub { kind: 'js' | 'css'; state: unknown; }

interface JspState {
  /** The wrapped HTML mode's own state (frozen while inside a block / embedded region). */
  inner: unknown;
  /** An open, possibly multi-line JSP block — or null while parsing markup. */
  block: null | 'comment' | 'scriptlet' | 'cdata';
  /** An open `<script>`/`<style>` region delegated to JS/CSS — or null. */
  sub: Sub | null;
}

function subMode(kind: Sub['kind']): StreamParser<unknown> {
  return kind === 'js' ? jsMode : cssMode;
}
function startSub(kind: Sub['kind']): Sub {
  const m = subMode(kind);
  return { kind, state: m.startState ? m.startState(2) : {} };
}
function copySub(sub: Sub): Sub {
  const m = subMode(sub.kind);
  return { kind: sub.kind, state: m.copyState ? m.copyState(sub.state) : sub.state };
}

/** Consume the rest of an open JSP block; returns its token style. */
function tokenBlock(stream: StringStream, state: JspState): string | null {
  if (state.block === 'cdata') {
    if (stream.match(/^\]\]>/)) { state.block = null; return 'meta'; }
    if (!stream.skipTo(']]>')) stream.skipToEnd();
    return null;
  }
  const isComment = state.block === 'comment';
  if (stream.match(isComment ? RE_COMMENT_END : RE_SCRIPTLET_END)) {
    state.block = null;
    return isComment ? 'comment' : 'meta';
  }
  stream.skipToEnd();
  return isComment ? 'comment' : 'meta';
}

export const jsp: StreamParser<JspState> = {
  name: 'jsp',

  startState(indentUnit) {
    return {
      inner: htmlMode.startState ? htmlMode.startState(indentUnit) : {},
      block: null,
      sub: null,
    };
  },

  copyState(state) {
    return {
      inner: htmlMode.copyState ? htmlMode.copyState(state.inner) : state.inner,
      block: state.block,
      sub: state.sub ? copySub(state.sub) : null,
    };
  },

  token(stream, state) {
    // 1. Continuing an open JSP block (comment / scriptlet / cdata) — highest priority.
    if (state.block) return tokenBlock(stream, state);

    // 2. Inside an embedded <script>/<style> — delegate to JS/CSS until the close tag.
    if (state.sub) {
      const closeRe = state.sub.kind === 'js' ? RE_SCRIPT_CLOSE : RE_STYLE_CLOSE;
      if (stream.match(closeRe)) { state.sub = null; return 'tag'; }
      return subMode(state.sub.kind).token(stream, state.sub.state);
    }

    // 3. Markup mode. JSP block openers (comment first — `<%--` also starts with `<%`).
    if (stream.match('<%--')) { state.block = 'comment'; return 'comment'; }
    if (stream.match(/^<%[@!=]?/)) { state.block = 'scriptlet'; return 'meta'; }
    if (stream.match('<![CDATA[')) { state.block = 'cdata'; return 'meta'; }
    // Embedded language regions (the whole open tag reads as a tag).
    if (stream.match(RE_SCRIPT_OPEN)) { state.sub = startSub('js'); return 'tag'; }
    if (stream.match(RE_STYLE_OPEN)) { state.sub = startSub('css'); return 'tag'; }
    // Otherwise hand this token to the HTML mode.
    return htmlMode.token(stream, state.inner);
  },

  blankLine(state, indentUnit) {
    if (state.block) return;
    if (state.sub) { subMode(state.sub.kind).blankLine?.(state.sub.state, indentUnit); return; }
    htmlMode.blankLine?.(state.inner, indentUnit);
  },

  indent(state, textAfter, context) {
    if (state.block) return null;
    if (state.sub) {
      const m = subMode(state.sub.kind);
      return m.indent ? m.indent(state.sub.state, textAfter, context) : null;
    }
    return htmlMode.indent ? htmlMode.indent(state.inner, textAfter, context) : null;
  },

  languageData: htmlMode.languageData,
  tokenTable: htmlMode.tokenTable,
};
