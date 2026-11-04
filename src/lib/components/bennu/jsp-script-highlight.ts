/**
 * Highlighting for the body of a JSP `<script>` — JavaScript with JSP woven through it.
 *
 * ```jsp
 * <script>
 *   <c:if test="${noteVisibili}">
 *     $(this).attr('title', '<wp:i18n key="LABEL" />');   // taglib tag inside a JS string
 *   </c:if>
 * </script>
 * ```
 *
 * The tree-sitter grammar hands the whole body over as one raw-text leaf, so what colours it is
 * this file. Two languages share the text, and the rule that keeps them apart is a single
 * substitution: **JSP constructs are blanked out for the JavaScript pass** (same length, newlines
 * kept), then painted on top by a JSP pass. JavaScript therefore never sees `<c:if …>` as a
 * comparison, and JSP never has to guess where a JS string ends — a `'(<wp:i18n/>)'` is one string
 * to the JS lexer, with a tag drawn over its middle.
 *
 * Also owns {@link maskScriptBodies}: the grammar's raw-text token stops at the first `</`, so a
 * `</c:if>` inside the script would end the element early and tear the rest of the page. The
 * parser is shown a copy in which those two characters are replaced by `< ` (same length, so every
 * offset still lines up with the real document).
 */

import type { LeafSpan } from '$lib/components/shared/ui/code-editor';
import {
  javascriptStream,
  namespaceTokenClass,
  streamTokenSpans,
} from '$lib/components/shared/ui/code-editor';
import type { StreamParser } from '@codemirror/language';

/** The words EL and OGNL reserve. They are `el_identifier` nodes to the grammar — telling a
 *  keyword from a name is a job for a vocabulary, not for a parser. Shared with `jsp-lang.ts`,
 *  which colours the same words in markup. */
export const EL_KEYWORDS = new Set([
  'and', 'or', 'not', 'eq', 'ne', 'lt', 'gt', 'le', 'ge', 'div', 'mod', 'empty', 'instanceof',
  'in', 'new',
]);

/** …and the ones that are values. */
export const EL_ATOMS = new Set(['true', 'false', 'null']);

// ── Parser input mask ───────────────────────────────────────────────────────────

/** Comments first, so a `<script>` mentioned inside one is skipped rather than masked. */
const SCRIPT_OPEN_OR_COMMENT = /<%--[\s\S]*?--%>|<!--[\s\S]*?-->|<script\b[^>]*>/gi;

/**
 * `text` with every `</` inside a `<script>` body replaced by `< ` — same length, so the parse
 * tree's offsets are the document's offsets. The body runs up to the first `</script`.
 */
export function maskScriptBodies(text: string): string {
  if (!/<script\b/i.test(text)) return text;
  const lower = text.toLowerCase();
  let out = '';
  let copied = 0;
  SCRIPT_OPEN_OR_COMMENT.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = SCRIPT_OPEN_OR_COMMENT.exec(text)) !== null) {
    if (!m[0].toLowerCase().startsWith('<script')) continue; // a comment: skipped whole
    const bodyStart = m.index + m[0].length;
    const close = lower.indexOf('</script', bodyStart);
    const bodyEnd = close < 0 ? text.length : close;
    out += text.slice(copied, bodyStart) + text.slice(bodyStart, bodyEnd).replace(/<\//g, '< ');
    copied = bodyEnd;
    SCRIPT_OPEN_OR_COMMENT.lastIndex = bodyEnd;
  }
  return out + text.slice(copied);
}

// ── JSP constructs inside the body ──────────────────────────────────────────────

type SegmentKind = 'comment' | 'scriptlet' | 'tag' | 'el';
interface Segment { from: number; to: number; kind: SegmentKind }

/**
 * One match per: JSP comment | scriptlet family | namespaced tag (attribute values may hold a `>`,
 * e.g. `test="${a > b}"`) | EL / deferred EL / OGNL expression. The tag needs a `prefix:` so a
 * plain `a<b` in JavaScript is never mistaken for one.
 */
const JSP_CONSTRUCT =
  /(<%--[\s\S]*?--%>)|(<%[\s\S]*?%>)|(<\/?[A-Za-z][\w-]*:[\w.-]+(?:"[^"]*"|'[^']*'|[^>"'])*>)|([$#%]\{[^}\n]*\})/g;

function findSegments(text: string): Segment[] {
  const out: Segment[] = [];
  JSP_CONSTRUCT.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = JSP_CONSTRUCT.exec(text)) !== null) {
    const kind: SegmentKind = m[1] ? 'comment' : m[2] ? 'scriptlet' : m[3] ? 'tag' : 'el';
    out.push({ from: m.index, to: m.index + m[0].length, kind });
  }
  return out;
}

/** `text` with every segment replaced by spaces (newlines kept), so JavaScript lexes what is left. */
function blank(text: string, segments: Segment[]): string {
  let out = '';
  let at = 0;
  for (const s of segments) {
    out += text.slice(at, s.from) + text.slice(s.from, s.to).replace(/[^\n]/g, ' ');
    at = s.to;
  }
  return out + text.slice(at);
}

// ── Painting the JSP side ───────────────────────────────────────────────────────

const EL_TOKEN =
  /("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|(\d[\w.]*)|([A-Za-z_][\w]*)|([.:@])|([-+*/%<>=!&|?])|([[\](),])/g;

/** The inside of `${…}` — the same palette `classify` gives the grammar's `el_*` nodes. */
function elSpans(text: string, from: number, to: number, out: LeafSpan[]): void {
  out.push({ from, to: from + 2, cls: 'punctuation' });
  out.push({ from: to - 1, to, cls: 'punctuation' });
  const inner = text.slice(from + 2, to - 1);
  const base = from + 2;
  EL_TOKEN.lastIndex = 0;
  let m: RegExpExecArray | null;
  let prev = '';
  while ((m = EL_TOKEN.exec(inner)) !== null) {
    const [tok, str, num, word, join, op, punct] = m;
    const span = (cls: string) => out.push({ from: base + m!.index, to: base + m!.index + tok.length, cls });
    if (str !== undefined) span('string');
    else if (num !== undefined) span('number');
    else if (word !== undefined) {
      span(EL_ATOMS.has(word) ? 'constant'
        : EL_KEYWORDS.has(word.toLowerCase()) ? 'keyword'
        : prev === '.' ? 'field' : 'ident');
    } else if (join !== undefined || op !== undefined) span('operator');
    else if (punct !== undefined) span('punctuation');
    prev = tok;
  }
}

const TAG_PARTS = /^(<\/?)([A-Za-z][\w-]*):([\w.-]+)/;
const ATTR = /([\w:.-]+)|(=)|("[^"]*"|'[^']*')|(\/?>)/g;

/** A namespaced tag: delimiters, the library-coloured name, then its attributes and EL values. */
function tagSpans(
  text: string,
  seg: Segment,
  slotFor: (prefix: string) => number | undefined,
  out: LeafSpan[],
): void {
  const src = text.slice(seg.from, seg.to);
  const head = TAG_PARTS.exec(src);
  if (!head) return;
  const [whole, open, prefix] = head;
  const slot = slotFor(prefix);
  out.push({ from: seg.from, to: seg.from + open.length, cls: 'punctuation' });
  out.push({
    from: seg.from + open.length,
    to: seg.from + whole.length,
    cls: slot === undefined ? 'keyword' : namespaceTokenClass(slot),
  });
  ATTR.lastIndex = whole.length;
  let m: RegExpExecArray | null;
  while ((m = ATTR.exec(src)) !== null) {
    const at = seg.from + m.index;
    const end = at + m[0].length;
    if (m[1] !== undefined) out.push({ from: at, to: end, cls: 'field' });
    else if (m[2] !== undefined) out.push({ from: at, to: end, cls: 'punctuation' });
    else if (m[3] !== undefined) {
      out.push({ from: at, to: end, cls: 'string' });
      for (const el of findSegments(m[3])) {
        if (el.kind === 'el') elSpans(text, at + el.from, at + el.to, out);
      }
    } else out.push({ from: at, to: end, cls: 'punctuation' });
  }
}

/**
 * Spans (relative to `text`) for a `<script>` body: JavaScript wherever it is JavaScript, JSP
 * wherever it is JSP. `slotFor` resolves a taglib prefix to its declared colour slot, so
 * `<c:…>` here wears the same hue as `<c:…>` in the markup; an undeclared prefix keeps the plain
 * tag colour, exactly as it does outside the script.
 */
export function jspScriptSpans(
  text: string,
  slotFor: (prefix: string) => number | undefined = () => undefined,
  js: StreamParser<unknown> = javascriptStream as unknown as StreamParser<unknown>,
): LeafSpan[] {
  const segments = findSegments(text);
  const out: LeafSpan[] = streamTokenSpans(js, segments.length ? blank(text, segments) : text);
  for (const seg of segments) {
    if (seg.kind === 'comment') out.push({ from: seg.from, to: seg.to, cls: 'comment' });
    else if (seg.kind === 'scriptlet') out.push({ from: seg.from, to: seg.to, cls: 'annotation' });
    else if (seg.kind === 'el') elSpans(text, seg.from, seg.to, out);
    else tagSpans(text, seg, slotFor, out);
  }
  // The highlighter's builder wants ascending starts; at a shared start the wider span goes first
  // so the narrower one nests inside it and wins the colour.
  return out
    .filter((s) => s.to > s.from)
    .sort((a, b) => a.from - b.from || b.to - a.to);
}
