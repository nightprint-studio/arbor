/**
 * JSP highlight overlay — a thin CodeMirror decoration layer that sits **on top of
 * `@codemirror/lang-html`** (which owns the real HTML/JS/CSS tree, so those three read
 * distinctly and consistently, and tag bodies fold). This overlay re-colours the JSP
 * constructs the HTML grammar doesn't know about:
 *
 *   • `<%-- … --%>`                 → comment
 *   • `<%@ … %>` / `<%! … %>` /
 *     `<%= … %>` / `<%  … %>`       → scriptlet (meta)
 *   • `${ … }` / `#{ … }`           → EL expression
 *
 * Why an overlay and not a parser: in the HTML content model `<%` is ordinary character
 * data (a `<` not followed by a name is text), so lang-html does NOT choke on scriptlets
 * — it just leaves them as text. We paint over that text (and over the odd `${…}` inside
 * an attribute value) with mark decorations at the HIGHEST precedence, so the JSP colour
 * wins over whatever lang-html assigned. JSP custom tags (`<c:if>`, `<s:iterator>`) are
 * plain elements to lang-html and highlight as tags for free — no handling needed here.
 *
 * Decorations are rebuilt from the whole document on each edit (JSP files are small); the
 * scan is a single linear regex, so it's cheap.
 */

import { Decoration, ViewPlugin, type DecorationSet, type EditorView, type ViewUpdate } from '@codemirror/view';
import { RangeSetBuilder, Prec } from '@codemirror/state';

const D_COMMENT = Decoration.mark({ class: 'cm-jsp-comment' });
const D_SCRIPTLET = Decoration.mark({ class: 'cm-jsp-scriptlet' });
const D_EL = Decoration.mark({ class: 'cm-jsp-el' });

// One left-to-right scan. Comment alternative first (it also starts with `<%`); then any
// scriptlet/directive/declaration/expression up to the first `%>`; then EL. Non-greedy so
// each block ends at its own terminator; unterminated blocks simply don't match (left to
// lang-html).
const JSP_RE = /<%--[\s\S]*?--%>|<%[@!=]?[\s\S]*?%>|[$#]\{[^}]*\}/g;

function buildDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const text = view.state.doc.toString();
  JSP_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = JSP_RE.exec(text)) !== null) {
    const from = m.index;
    const to = from + m[0].length;
    if (to === from) { JSP_RE.lastIndex++; continue; } // guard against a zero-width match
    const deco = m[0].startsWith('<%--') ? D_COMMENT : m[0].startsWith('<%') ? D_SCRIPTLET : D_EL;
    builder.add(from, to, deco);
  }
  return builder.finish();
}

/** The JSP overlay extension — install AFTER `html()` in a descriptor's `cmExtension`.
 *  Highest precedence so its colours win over lang-html's for the same span. */
export const jspOverlay = Prec.highest(
  ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = buildDecorations(view);
      }
      update(u: ViewUpdate) {
        if (u.docChanged) this.decorations = buildDecorations(u.view);
      }
    },
    { decorations: (v) => v.decorations },
  ),
);
