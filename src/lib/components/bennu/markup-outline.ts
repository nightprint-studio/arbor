/**
 * Lightweight markup outline — a nested TAG tree for JSP / JSPF / TAG / XML / XSD /
 * WSDL / TLD / POM files, for the Structure / Outline panels. Same philosophy as
 * `java-outline.ts`: NOT a real XML parser (the editor owns highlighting), just a
 * cheap, linear tag scan that's plenty for a scannable element tree + jump-to-line.
 *
 * It is intentionally forgiving — legacy JSP is rarely well-formed XML (unclosed
 * `<br>`, `<%… %>` scriptlets, mismatched tags). We track a tag stack and self-heal
 * on mismatched closers rather than bailing, so a messy Struts JSP still yields a
 * usable outline.
 *
 * SEAM — when a real markup language service lands (or the BE serves an element
 * model), replace `markupOutline`; the `MarkupNode[]` shape can stay and be fed from
 * the backend instead.
 */

export interface MarkupNode {
  /** Stable id for the Tree widget: tag name + line + a running index (a document can
   *  legitimately repeat `<div>` many times, so name+line alone isn't unique). */
  id: string;
  /** Raw element tag (`action`, `interceptor-ref`), used ONLY to match closing tags when
   *  popping the ancestry stack. MUST stay the bare tag — the display `name` carries the
   *  `keyValue:tag` label, which would never match a `</tag>` closer. */
  tag: string;
  /** Display label, IntelliJ-style: `<keyValue>:<tag>` when the element carries a key
   *  attribute (id → name → …), else just the tag. So 20 `<action>`s read as
   *  `stepOpenCategorie:action`, `detail:action`, … instead of 20 identical `action`s. */
  name: string;
  /** A secondary attribute (`class` / `type`) surfaced muted after the label — e.g. the
   *  Struts/Spring implementation class. */
  detail?: string;
  /** 1-based line of the element's opening tag. */
  line: number;
  children?: MarkupNode[];
}

/** Key attributes that name an element, in priority order (id wins, then name, …) — the
 *  first present becomes the label prefix. Disambiguates the tags that repeat most in
 *  JSP/XML (Struts/JSTL `var`/`property`, Maven `artifactId`, XSD `name`, generic `id`). */
const KEY_ATTRS = ['id', 'name', 'var', 'property', 'artifactId', 'ref', 'value', 'test', 'bean'];

/** Secondary attributes shown muted after the label (the impl class of a Struts action /
 *  Spring bean, etc.). First present wins. */
const DETAIL_ATTRS = ['class', 'type'];

/** A very long line is a data blob / minified doc — skip it (perf + a hard backstop
 *  against pathological regex input, mirroring java-outline's `MAX_DECL_LINE`). */
const MAX_TAG_LINE = 2000;

/** Tags that are void/self-closing by convention in loose HTML/JSP even without a
 *  trailing `/` — treating them as leaves keeps the stack from drifting on legacy
 *  markup that never closes them. */
const VOID_TAGS = new Set([
  'br', 'hr', 'img', 'input', 'meta', 'link', 'col', 'area', 'base', 'source', 'track', 'wbr', 'embed',
]);

/** One tag occurrence the linear scanner produced. */
interface TagToken {
  name: string;
  line: number;
  kind: 'open' | 'close' | 'selfclose';
  /** The priority key-attribute value (id → name → …), or undefined. */
  key?: string;
  /** The secondary attribute value (class / type), or undefined. */
  detail?: string;
}

/** Parse `name="v"` / `name='v'` pairs off a raw opening-tag body (the text between
 *  `<name` and the closing `>`) into a map. Linear per-attribute regex, capped input. */
const ATTR_RE = /([\w:.-]+)\s*=\s*"([^"]*)"|([\w:.-]+)\s*=\s*'([^']*)'/g;
function parseAttrs(tagBody: string): Record<string, string> {
  const found: Record<string, string> = {};
  ATTR_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = ATTR_RE.exec(tagBody)) !== null) {
    const key = m[1] ?? m[3];
    const val = m[2] ?? m[4] ?? '';
    if (key && !(key in found)) found[key] = val;
  }
  return found;
}
function pick(attrs: Record<string, string>, order: string[]): string | undefined {
  for (const k of order) if (k in attrs) return attrs[k];
  return undefined;
}

/**
 * Tokenize the source into a flat, ordered list of tag opens / closes / self-closes.
 * A single regex sweep finds `<…>` runs; comments (`<!-- … -->`), processing
 * instructions / declarations (`<? … ?>`, `<!DOCTYPE …>`), CDATA and JSP scriptlets
 * (`<% … %>`) are skipped as non-structural. Linear over the source length.
 */
const TAG_RE = /<(\/?)([A-Za-z_][\w:.-]*)((?:[^<>"']|"[^"]*"|'[^']*')*?)(\/?)>/g;
function tokenize(source: string): TagToken[] {
  const tokens: TagToken[] = [];
  // Strip comments / CDATA / PIs / scriptlets first so their inner `<…>` can't be
  // mistaken for tags. Replace with equal-newline-count blanks so line numbers survive.
  const cleaned = source
    .replace(/<!--[\s\S]*?-->/g, blankPreserveLines)
    .replace(/<!\[CDATA\[[\s\S]*?\]\]>/g, blankPreserveLines)
    .replace(/<%[\s\S]*?%>/g, blankPreserveLines)
    .replace(/<\?[\s\S]*?\?>/g, blankPreserveLines)
    .replace(/<!DOCTYPE[^>]*>/gi, blankPreserveLines);

  // Precompute line starts for O(1) line lookup from a char offset.
  const lineStarts = lineStartOffsets(cleaned);

  TAG_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TAG_RE.exec(cleaned)) !== null) {
    const full = m[0];
    if (full.length > MAX_TAG_LINE) continue;
    const isClose = m[1] === '/';
    const name = m[2];
    const body = m[3] ?? '';
    const selfClose = m[4] === '/' || VOID_TAGS.has(name.toLowerCase());
    const line = offsetToLine(lineStarts, m.index);
    if (isClose) {
      tokens.push({ name, line, kind: 'close' });
    } else {
      const attrs = parseAttrs(body);
      tokens.push({
        name, line, kind: selfClose ? 'selfclose' : 'open',
        key: pick(attrs, KEY_ATTRS), detail: pick(attrs, DETAIL_ATTRS),
      });
    }
  }
  return tokens;
}

/** Replace a matched span with blanks but keep its newlines, so downstream line
 *  numbers stay correct after comment/scriptlet stripping. */
function blankPreserveLines(match: string): string {
  return match.replace(/[^\n]/g, ' ');
}

function lineStartOffsets(s: string): number[] {
  const starts = [0];
  for (let i = 0; i < s.length; i++) if (s[i] === '\n') starts.push(i + 1);
  return starts;
}

/** Binary search the 0-based offset → 1-based line number. */
function offsetToLine(starts: number[], offset: number): number {
  let lo = 0, hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= offset) lo = mid; else hi = mid - 1;
  }
  return lo + 1;
}

/**
 * Build the nested element tree from the token stream. A stack of open elements holds
 * the current ancestry; a close pops back to the matching open (self-healing: if the
 * closer doesn't match the top, we pop the nearest matching ancestor if one exists,
 * else ignore the stray closer). Self-closing / void tags are leaves.
 */
export function markupOutline(source: string): MarkupNode[] {
  const tokens = tokenize(source);
  const roots: MarkupNode[] = [];
  const stack: MarkupNode[] = [];
  let seq = 0;

  const attach = (node: MarkupNode) => {
    const top = stack[stack.length - 1];
    if (top) (top.children ??= []).push(node);
    else roots.push(node);
  };

  for (const t of tokens) {
    if (t.kind === 'close') {
      // Pop to the nearest matching ancestor by RAW tag (not the display label); tolerate
      // mismatches (legacy JSP). Matching on `tag` is what keeps siblings from over-nesting.
      for (let k = stack.length - 1; k >= 0; k--) {
        if (stack[k].tag === t.name) { stack.length = k; break; }
      }
      continue;
    }
    // IntelliJ-style label: `<keyValue>:<tag>` when the element is named (id → name → …),
    // else just the tag — so repeated `<action>`/`<bean>` rows stay distinguishable.
    const label = t.key ? `${t.key}:${t.name}` : t.name;
    const node: MarkupNode = { id: `${t.name}:${t.line}:${seq++}`, tag: t.name, name: label, detail: t.detail, line: t.line };
    attach(node);
    if (t.kind === 'open') stack.push(node);
  }

  return roots;
}
