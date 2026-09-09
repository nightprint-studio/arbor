/**
 * The hover card every code editor in Arbor shows — one shape, one file.
 *
 * A hover answers three questions and they are always the same three: *what is this*
 * (the signature), *which one is it* (where it comes from), and *what does it do* (the
 * documentation). So the card has a title, a meta line and a body, and the products fill
 * them in: Bennu with a Java symbol, Picus with a column's facts. It used to be built
 * twice — once in each product, against the same `.cm-hc-*` class names — which is a
 * shared stylesheet with no shared code, the arrangement where the two drift.
 *
 * ## Javadoc is rendered, not flattened
 *
 * A Javadoc body **is HTML** — that is what the language specifies, and real ones are full
 * of `<h3>`, `<pre class="code">`, `<ul>` and `&#064;`. Flattening it to text produced the
 * worst of both: the markup was neither removed nor honoured, so the reader got `<h3>Overview</h3>`
 * and a `&#064;Bean` sitting in the middle of a sentence. Here the markup is **parsed into
 * DOM**: headings become headings, `<pre>` becomes a scrollable code block, lists become
 * lists, entities become the characters they name. The inline forms — `{@link Foo#bar}`,
 * `{@code x}` — become the reference or the code they name.
 *
 * The `@param` / `@return` / `@throws` blocks are lifted out first and rendered as a
 * definition list, because they are a table and prose is not. `@deprecated` is the one tag
 * that gets a colour, because it is the one that changes what you do next.
 *
 * ### A tag we do not know is not a tag
 *
 * The element names below are a **closed list**, and that is load-bearing rather than
 * cautious. Documentation is full of angle brackets that are not markup — `Vec<T>`,
 * `Map<K, V>`, `List<? extends Number>` — and a parser that treats every `<name>` as an
 * element deletes exactly the part the reader needed. Anything not on the list stays in the
 * text as the characters the author typed.
 *
 * ### Nothing here is markup that runs
 *
 * No `innerHTML`, no `DOMParser`, no attributes carried over — every element is created by
 * name from the closed list and every string arrives through `textContent`. A doc string is
 * text out of somebody's project or somebody's jar; it is data. `<a>` in particular renders
 * as styled text and **not** as a link: a click inside a Tauri webview navigates the app
 * itself, and a tooltip is not a place from which the editor should be able to leave.
 */

import { highlightToDom } from '$lib/utils/highlight';

/** What the card renders. A view type, deliberately not any product's wire shape. */
export interface HoverCard {
  /** The signature line — the card's title, monospaced. */
  signature: string;
  /** Owning type / namespace, for the muted meta line. */
  container?: string | null;
  /** What kind of thing it is — rendered as a small tag before the meta. */
  kind?: string | null;
  /** The explanation body. Javadoc (or plain prose) — parsed, see the module doc. */
  doc?: string | null;
  /** Where it came from, when it came from somewhere with a name — `groupId:artifactId:version`
   *  for a type out of a dependency jar. Absent for the project's own code and for the JDK. */
  artifact?: string | null;
  /**
   * The language the `<pre>` blocks inside {@link doc} are written in — a Prism id (`java`,
   * `rust`, `sql`), the same vocabulary a fenced block in a rendered `.md` uses.
   *
   * The language of the thing being **documented**, not of the file the pointer is in: a Javadoc
   * read from a JSP still documents Java. Absent means the block is shown uncoloured, which is
   * the honest answer when nobody knows.
   */
  codeLanguage?: string | null;
}

/** One `@tag` block of a Javadoc comment. */
interface DocTag {
  /** `param`, `return`, `throws`, `deprecated`, … */
  tag: string;
  /** The subject a `@param` / `@throws` names, when it has one. */
  subject?: string;
  /** The tag's body, still as written — rendered through the same pass as the prose. */
  text: string;
}

/** Prose + tags, split out of a Javadoc body. Both halves are **unrendered**: they are the
 *  author's markup, and {@link renderDoc} is what turns either into DOM. */
interface ParsedDoc {
  prose: string;
  tags: DocTag[];
}

// ── What a Javadoc body may contain ──────────────────────────────────────────────

/**
 * The block-level elements honoured. Closed on purpose — see the module doc on `Vec<T>`.
 */
const BLOCK_ELEMENTS = new Set([
  'p', 'div', 'blockquote', 'pre', 'ul', 'ol', 'li', 'dl', 'dt', 'dd',
  'table', 'thead', 'tbody', 'tfoot', 'tr', 'td', 'th', 'caption',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
]);

/** The inline elements honoured. `a` is here so its **text** survives; it never becomes a link. */
const INLINE_ELEMENTS = new Set([
  'code', 'tt', 'samp', 'kbd', 'var', 'b', 'strong', 'i', 'em', 'u', 's',
  'sub', 'sup', 'span', 'a', 'small', 'cite', 'q', 'font',
]);

/** Elements that carry no content and close themselves. */
const VOID_ELEMENTS = new Set(['br', 'hr']);

/**
 * What opening one element implicitly closes.
 *
 * Javadoc is written by hand and hand-written HTML leaves `<p>` and `<li>` open — that is
 * legal HTML and it is what real doc comments look like. Without this a page of `<p>`s
 * nests forty levels deep and every paragraph indents further than the last.
 */
const AUTO_CLOSE: Record<string, Set<string>> = {
  p: new Set(['p']),
  li: new Set(['li', 'p']),
  dt: new Set(['dt', 'dd', 'p']),
  dd: new Set(['dt', 'dd', 'p']),
  tr: new Set(['tr', 'td', 'th', 'p']),
  td: new Set(['td', 'th', 'p']),
  th: new Set(['td', 'th', 'p']),
};

/** What every *other* block element closes. A `<h3>` or a `<pre>` after a blank line cannot live
 *  inside the paragraph that blank line opened — nested, it inherits the paragraph's spacing and
 *  the card grows a gap above every heading. */
const CLOSES_PARAGRAPH = new Set(['p']);

/**
 * The Javadoc **block** tags — the ones that end the prose and start a labelled row.
 *
 * A closed list, and for the same reason the element list is closed: a code example inside
 * the comment has lines starting with `@Override` or `@Bean`, and treating any `@word` as a
 * block tag tears the example apart and files half of it under a tag named "Override".
 */
const BLOCK_TAGS = new Set([
  'param', 'return', 'returns', 'throws', 'exception', 'see', 'since', 'author',
  'version', 'deprecated', 'serial', 'serialfield', 'serialdata', 'apinote',
  'implspec', 'implnote', 'hidden', 'provides', 'uses', 'category', 'spec',
]);

/** Tags whose first word is a subject (`@param name …`) rather than part of the text. */
const SUBJECT_TAGS = new Set(['param', 'throws', 'exception']);

// ── Entities ─────────────────────────────────────────────────────────────────────

/**
 * The named entities a doc comment actually uses. Not the full HTML set: a doc author
 * reaches for these and little else, and the numeric forms below cover the rest.
 *
 * `&#064;` earns its own mention — it is how every Javadoc author writes an `@` inside an
 * example so the tool does not read it as a tag, which is why an unrendered card is full of
 * them.
 */
const NAMED_ENTITIES: Record<string, string> = {
  lt: '<', gt: '>', amp: '&', quot: '"', apos: "'", nbsp: ' ',
  hellip: '…', mdash: '—', ndash: '–', minus: '−',
  ldquo: '“', rdquo: '”', lsquo: '‘', rsquo: '’',
  laquo: '«', raquo: '»', bull: '•', middot: '·',
  copy: '©', reg: '®', trade: '™', deg: '°', times: '×',
  larr: '←', rarr: '→', harr: '↔', rArr: '⇒',
  le: '≤', ge: '≥', ne: '≠', infin: '∞', sum: '∑',
  alpha: 'α', beta: 'β', gamma: 'γ', delta: 'δ',
  lambda: 'λ', mu: 'μ', pi: 'π', sigma: 'σ', tau: 'τ',
  ensp: ' ', emsp: ' ', thinsp: ' ', shy: '',
};

/** Turn `&#064;` / `&#x40;` / `&nbsp;` into the characters they name, leaving anything
 *  unrecognised exactly as written — an `&` in prose is an `&`. */
function decodeEntities(s: string): string {
  return s.replace(/&(#[0-9]+|#[xX][0-9a-fA-F]+|[a-zA-Z][a-zA-Z0-9]*);/g, (whole, body: string) => {
    if (body[0] === '#') {
      const code = body[1] === 'x' || body[1] === 'X'
        ? parseInt(body.slice(2), 16)
        : parseInt(body.slice(1), 10);
      // Reject the ranges that cannot be a character: a malformed entity stays as typed.
      if (!Number.isFinite(code) || code <= 0 || code > 0x10ffff) return whole;
      try {
        return String.fromCodePoint(code);
      } catch {
        return whole;
      }
    }
    const named = NAMED_ENTITIES[body];
    return named !== undefined ? named : whole;
  });
}

// ── Tokenising ───────────────────────────────────────────────────────────────────

type Token =
  | { kind: 'text'; text: string }
  | { kind: 'open' | 'close' | 'void'; name: string };

/** Matches one HTML tag, tolerating quoted attribute values that contain `>`. */
const TAG = /<(\/?)([a-zA-Z][a-zA-Z0-9]*)((?:"[^"]*"|'[^']*'|[^>'"])*)>/g;

/** Whether an element name is one this card renders. */
function known(name: string): boolean {
  return BLOCK_ELEMENTS.has(name) || INLINE_ELEMENTS.has(name) || VOID_ELEMENTS.has(name);
}

/**
 * Split a doc body into text runs and the tags this card knows.
 *
 * A `<name>` that is not on the closed list is **not** consumed: it stays inside the
 * surrounding text run, which is what keeps `Map<K, V>` intact.
 */
function tokenize(src: string): Token[] {
  const out: Token[] = [];
  const cleaned = src.replace(/<!--[\s\S]*?-->/g, '');
  let last = 0;
  TAG.lastIndex = 0;
  for (let m = TAG.exec(cleaned); m; m = TAG.exec(cleaned)) {
    const name = m[2].toLowerCase();
    if (!known(name)) continue; // not a tag here — leave the characters in the text
    if (m.index > last) out.push({ kind: 'text', text: cleaned.slice(last, m.index) });
    last = m.index + m[0].length;
    if (VOID_ELEMENTS.has(name)) out.push({ kind: 'void', name });
    else out.push({ kind: m[1] ? 'close' : 'open', name });
  }
  if (last < cleaned.length) out.push({ kind: 'text', text: cleaned.slice(last) });
  return out;
}

/**
 * Turn the blank lines **outside** `<pre>` into paragraph breaks.
 *
 * A doc comment separates paragraphs either with `<p>` or with an empty line, and both are
 * common in the same file. Inside a `<pre>` a blank line is a blank line of code, so the
 * substitution has to know where it is — which is the whole reason this is a scan and not a
 * regex over the string.
 */
function paragraphize(src: string): string {
  const parts: string[] = [];
  const openPre = /<pre\b[^>]*>/i;
  const closePre = /<\/pre\s*>/i;
  let rest = src;
  for (;;) {
    const open = openPre.exec(rest);
    if (!open) {
      parts.push(rest.replace(/\n[ \t]*\n[\s]*/g, '<p>'));
      break;
    }
    parts.push(rest.slice(0, open.index).replace(/\n[ \t]*\n[\s]*/g, '<p>'));
    const after = rest.slice(open.index);
    const close = closePre.exec(after);
    if (!close) {
      parts.push(after); // unterminated: everything left is code
      break;
    }
    const end = close.index + close[0].length;
    parts.push(after.slice(0, end));
    rest = after.slice(end);
  }
  return parts.join('');
}

// ── Inline taglets ───────────────────────────────────────────────────────────────

/** `{@link …}`, `{@code …}` and friends. Brace-free bodies only — a taglet whose body has
 *  its own braces is left as written rather than half-eaten. */
const TAGLET = /\{@(link|linkplain|code|literal|value|index|systemProperty)\s*([^{}]*)\}/g;

/** What a `{@link}`-family taglet names: the author's label when they wrote one, else the
 *  target with its package and its `#` stripped down to what a reader recognises. */
function linkText(body: string): string {
  const trimmed = body.trim();
  const space = trimmed.search(/\s/);
  if (space > 0) return trimmed.slice(space + 1).trim(); // `{@link Foo#bar() the label}`
  return trimmed.replace(/^.*?#/, '#').replace(/^#/, '') || trimmed;
}

// ── Building the DOM ─────────────────────────────────────────────────────────────

/** The element a source tag becomes. Several names collapse onto one so the card has one
 *  heading size and one code style rather than six of each. */
function elementFor(name: string): HTMLElement {
  switch (name) {
    case 'h1': case 'h2': case 'h3': case 'h4': case 'h5': case 'h6':
      return document.createElement('h4');
    case 'tt': case 'samp': case 'kbd': case 'var': case 'code':
      return document.createElement('code');
    case 'b': case 'strong':
      return document.createElement('strong');
    case 'i': case 'em': case 'cite': case 'q':
      return document.createElement('em');
    case 'a': {
      // Text, never a link — see the module doc. Styled like one so the author's emphasis
      // survives; inert so a tooltip cannot navigate the app.
      const anchor = document.createElement('span');
      anchor.className = 'cm-hc-anchor';
      return anchor;
    }
    case 'font': case 'small': case 'u': case 's':
      return document.createElement('span');
    default:
      return document.createElement(name);
  }
}

/** Append one text run, honouring `{@code}` / `{@link}` and collapsing whitespace outside
 *  a `<pre>`. Everything lands through `textContent`. */
function appendText(parent: HTMLElement, raw: string, inPre: boolean): void {
  let at = 0;
  TAGLET.lastIndex = 0;
  for (let m = TAGLET.exec(raw); m; m = TAGLET.exec(raw)) {
    appendPlain(parent, raw.slice(at, m.index), inPre);
    at = m.index + m[0].length;
    const kind = m[1];
    const body = m[2] ?? '';
    if (kind === 'literal') {
      appendPlain(parent, body, inPre);
      continue;
    }
    const code = document.createElement('code');
    if (kind === 'link' || kind === 'linkplain') code.className = 'cm-hc-ref';
    code.textContent = decodeEntities(
      kind === 'code' ? body : linkText(body),
    ).trim();
    if (code.textContent) parent.appendChild(code);
  }
  appendPlain(parent, raw.slice(at), inPre);
}

/** A run with no taglets left in it. */
function appendPlain(parent: HTMLElement, raw: string, inPre: boolean): void {
  if (!raw) return;
  let text = decodeEntities(raw);
  if (!inPre) {
    text = text.replace(/\s+/g, ' ');
    // No leading space where a block starts, and none doubled onto one already there.
    if (text === ' ' && !parent.lastChild) return;
    if (!parent.lastChild) text = text.replace(/^ /, '');
  }
  if (!text) return;
  parent.appendChild(document.createTextNode(text));
}

/**
 * Render a doc body into `root`, as DOM.
 *
 * Exported because the tag rows use it too: a `@param` description is written in the same
 * markup as the prose above it and there is no reason it should read worse.
 */
export function renderDoc(root: HTMLElement, doc: string, codeLanguage?: string | null): void {
  const stack: { name: string; el: HTMLElement }[] = [];
  const top = () => (stack.length ? stack[stack.length - 1].el : root);
  const inPre = () => stack.some((f) => f.name === 'pre');

  for (const tok of tokenize(paragraphize(doc))) {
    if (tok.kind === 'text') {
      appendText(top(), tok.text, inPre());
      continue;
    }
    if (tok.kind === 'void') {
      // A `<br>` inside a `<pre>` would double the break the newline already made.
      if (tok.name === 'br' && inPre()) continue;
      if (tok.name === 'hr') {
        while (stack.length && stack[stack.length - 1].name === 'p') stack.pop();
      }
      top().appendChild(document.createElement(tok.name));
      continue;
    }
    if (tok.kind === 'open') {
      const closes = AUTO_CLOSE[tok.name]
        ?? (BLOCK_ELEMENTS.has(tok.name) ? CLOSES_PARAGRAPH : undefined);
      while (closes && stack.length && closes.has(stack[stack.length - 1].name)) stack.pop();
      const el = elementFor(tok.name);
      if (tok.name === 'pre') el.className = 'cm-hc-pre';
      top().appendChild(el);
      stack.push({ name: tok.name, el });
      continue;
    }
    // A close with no opener is noise, not an instruction to unwind the whole document.
    const at = stack.map((f) => f.name).lastIndexOf(tok.name);
    if (at >= 0) stack.length = at;
  }
  pruneEmpty(root);
  tidyPre(root);
  colourCode(root, codeLanguage);
}

/**
 * Colour the code blocks, with the same grammars a fenced block in a rendered `.md` gets.
 *
 * Run as a pass over the finished blocks rather than during the walk, because a `<pre>` is a
 * **listing** and not prose: whatever inline markup the author put inside it — a `<b>` around the
 * interesting line, a `{@code}` — describes text, and the thing that should decide what is a
 * keyword and what is a string is a grammar, not the doc comment's own emphasis. So the block is
 * flattened to its characters and tokenised.
 *
 * Nothing here builds markup: {@link highlightToDom} walks Prism's token tree into elements. That
 * is the one reason this card can have Prism's colours without giving up the rule that a doc
 * comment out of somebody's jar is never something the page runs.
 */
function colourCode(root: HTMLElement, codeLanguage?: string | null): void {
  if (!codeLanguage) return;
  for (const pre of [...root.querySelectorAll('pre')]) {
    const text = pre.textContent ?? '';
    if (!text.trim()) continue;
    pre.textContent = '';
    pre.appendChild(highlightToDom(text, codeLanguage));
  }
}

/**
 * Take the blank first line off every code block.
 *
 * `<pre>` is almost always written with its opening tag on its own line, and an HTML *parser*
 * silently eats that first newline. This card does not go through one — it builds text nodes —
 * so without this every example in the tooltip opens with an empty line.
 */
function tidyPre(root: HTMLElement): void {
  for (const pre of [...root.querySelectorAll('pre')]) {
    const first = pre.firstChild;
    if (first && first.nodeType === 3) first.textContent = (first.textContent ?? '').replace(/^\r?\n/, '');
    const last = pre.lastChild;
    if (last && last.nodeType === 3) last.textContent = (last.textContent ?? '').replace(/\s+$/, '');
  }
}

/**
 * Drop the paragraphs that ended up with nothing in them.
 *
 * Auto-closing leaves them behind: a blank line opens a `<p>` and the `<h3>` on the next line
 * closes it again before a word has landed in it. Empty, it still carries a paragraph's spacing,
 * so the card grows a blank line above every heading in the document.
 */
function pruneEmpty(root: HTMLElement): void {
  for (const el of [...root.querySelectorAll('p, div')]) {
    if (!el.firstChild) el.remove();
  }
}

// ── Splitting prose from tags ────────────────────────────────────────────────────

/**
 * Split a Javadoc body into its prose and its `@tag` blocks, joining continuation lines.
 *
 * Both halves come back **as the author wrote them** — markup and all. Rendering is
 * {@link renderDoc}'s job, and keeping the two apart is what lets a `@param` description
 * carry a `{@code}` without this function needing to know what one is.
 */
export function parseDoc(doc: string): ParsedDoc {
  const prose: string[] = [];
  const tags: DocTag[] = [];
  let current: DocTag | null = null;
  // A line inside a code example is code, `@Override` included.
  let preDepth = 0;

  for (const raw of doc.split('\n')) {
    const line = raw.trim();
    const opens = (line.match(/<pre\b/gi) ?? []).length;
    const shuts = (line.match(/<\/pre\b/gi) ?? []).length;
    const start = preDepth === 0 ? /^@(\w+)\s*(.*)$/.exec(line) : null;
    preDepth = Math.max(0, preDepth + opens - shuts);

    if (start && BLOCK_TAGS.has(start[1].toLowerCase())) {
      const tag = start[1];
      let text = start[2] ?? '';
      let subject: string | undefined;
      if (SUBJECT_TAGS.has(tag.toLowerCase())) {
        const m = /^(\S+)\s*(.*)$/.exec(text);
        if (m) {
          subject = decodeEntities(m[1]);
          text = m[2] ?? '';
        }
      }
      current = { tag, subject, text };
      tags.push(current);
      continue;
    }
    // A continuation of whatever came last — a wrapped `@param`, or more prose.
    if (current) current.text = `${current.text}\n${line}`;
    else prose.push(raw);
  }

  return {
    prose: prose.join('\n').trim(),
    tags: tags
      .map((t) => ({ ...t, text: t.text.trim() }))
      .filter((t) => t.subject || t.text || t.tag.toLowerCase() === 'deprecated'),
  };
}

/** How a tag heads its row. `@param x` keeps its subject; the rest are the tag alone. */
function tagLabel(t: DocTag): string {
  const name = t.tag === 'exception' ? 'throws' : t.tag;
  return t.subject ? `${name} ${t.subject}` : name;
}

// ── The card ─────────────────────────────────────────────────────────────────────

/**
 * Build the shared `.cm-hover-card` DOM (styled in the editor theme).
 *
 * Two parts, and the split is what makes a long card usable: a **headline** that never
 * scrolls — the signature, the package, the coordinate — and a **body** that does. A tooltip
 * whose title scrolls away is a tooltip you have to scroll back up to identify.
 */
export function hoverCardDom(info: HoverCard): HTMLElement {
  const dom = document.createElement('div');
  dom.className = 'cm-hover-card';

  const headline = document.createElement('div');
  headline.className = 'cm-hc-headline';
  dom.appendChild(headline);

  const head = document.createElement('div');
  head.className = 'cm-hc-head';
  if (info.kind) {
    const k = document.createElement('span');
    k.className = 'cm-hc-kind';
    k.textContent = info.kind;
    head.appendChild(k);
  }
  const sig = document.createElement('span');
  sig.className = 'cm-hc-title';
  sig.textContent = info.signature;
  head.appendChild(sig);
  headline.appendChild(head);

  if (info.container) {
    const m = document.createElement('div');
    m.className = 'cm-hc-meta';
    m.textContent = info.container;
    headline.appendChild(m);
  }

  // The dependency it came out of, on its own line under the package.
  //
  // Its own line rather than appended to the package, because they answer different questions and
  // one of them is asked far less often: the package says *which type*, the coordinate says *whose
  // type*. On a legacy classpath that second one is the whole game — two jars declaring the same
  // simple name, or a class from a starter nobody remembers adding — and the package cannot say it.
  if (info.artifact) {
    const a = document.createElement('div');
    a.className = 'cm-hc-artifact';
    a.textContent = info.artifact;
    headline.appendChild(a);
  }

  if (!info.doc) return dom;

  const { prose, tags } = parseDoc(info.doc);
  if (!prose && tags.length === 0) return dom;

  const body = document.createElement('div');
  body.className = 'cm-hc-body';
  // Scrollable, therefore focusable: a pointer can reach it, and a keyboard should be able to
  // as well. `tabindex="-1"` is what lets the browser scroll it without putting it in tab order.
  body.tabIndex = -1;
  dom.appendChild(body);

  if (prose) {
    const d = document.createElement('div');
    d.className = 'cm-hc-doc';
    renderDoc(d, prose, info.codeLanguage);
    body.appendChild(d);
  }
  if (tags.length) {
    const dl = document.createElement('dl');
    dl.className = 'cm-hc-tags';
    for (const t of tags) {
      const dt = document.createElement('dt');
      dt.textContent = tagLabel(t);
      if (t.tag.toLowerCase() === 'deprecated') dt.className = 'cm-hc-deprecated';
      const dd = document.createElement('dd');
      renderDoc(dd, t.text, info.codeLanguage);
      dl.appendChild(dt);
      dl.appendChild(dd);
    }
    body.appendChild(dl);
  }
  return dom;
}
