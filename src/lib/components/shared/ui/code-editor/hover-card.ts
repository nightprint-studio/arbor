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
 * ## Javadoc is parsed, not dumped
 *
 * A raw Javadoc block pasted into a tooltip is a wall: the prose and the `@param` /
 * `@return` / `@throws` lines run together, and `{@link Foo#bar}` reads as markup noise.
 * Here the prose becomes a paragraph, the tags become a definition list, and the inline
 * `{@link}` / `{@code}` forms are unwrapped to what they name. `@deprecated` is the one
 * tag that gets a colour, because it is the one that changes what you do next.
 *
 * Everything goes in through `textContent`. A doc string is text from somebody's project;
 * it is never markup to run.
 */

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
}

/** One `@tag` block of a Javadoc comment. */
interface DocTag {
  /** `param`, `return`, `throws`, `deprecated`, … */
  tag: string;
  /** The subject a `@param` / `@throws` names, when it has one. */
  subject?: string;
  text: string;
}

/** Prose + tags, split out of a Javadoc body. */
interface ParsedDoc {
  prose: string;
  tags: DocTag[];
}

/** Tags whose first word is a subject (`@param name …`) rather than part of the text. */
const SUBJECT_TAGS = new Set(['param', 'throws', 'exception']);

/**
 * Unwrap the inline forms and the handful of HTML entities/tags Javadoc bodies carry, so
 * the text reads as text. `{@link Foo#bar}` and `{@code x}` become what they name — the
 * label after a `|`-less link target, when the author wrote one.
 */
function plainText(s: string): string {
  return s
    .replace(/\{@(?:link|linkplain|code|literal|value)\s+([^}]*)\}/g, (_m, body: string) => {
      const parts = String(body).trim().split(/\s+/);
      // `{@link Foo#bar(int) a label}` → the label when present, else the target.
      return parts.length > 1 ? parts.slice(1).join(' ') : parts[0] ?? '';
    })
    .replace(/<\/?(?:p|br|li|ul|ol|pre|code|b|i|em|strong|tt)\s*\/?>/gi, ' ')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
    .replace(/&nbsp;/g, ' ')
    .replace(/[ \t]+/g, ' ')
    .trim();
}

/** Split a Javadoc body into its prose and its `@tag` blocks, joining continuation lines. */
export function parseDoc(doc: string): ParsedDoc {
  const prose: string[] = [];
  const tags: DocTag[] = [];
  let current: DocTag | null = null;

  for (const raw of doc.split('\n')) {
    const line = raw.trim();
    const start = /^@(\w+)\s*(.*)$/.exec(line);
    if (start) {
      const tag = start[1];
      let text = start[2] ?? '';
      let subject: string | undefined;
      if (SUBJECT_TAGS.has(tag)) {
        const m = /^(\S+)\s*(.*)$/.exec(text);
        if (m) {
          subject = m[1];
          text = m[2] ?? '';
        }
      }
      current = { tag, subject, text };
      tags.push(current);
      continue;
    }
    // A continuation of whatever came last — a wrapped `@param`, or more prose.
    if (current) current.text = `${current.text} ${line}`.trim();
    else prose.push(line);
  }

  return {
    prose: plainText(prose.join(' ')),
    tags: tags
      .map((t) => ({ ...t, text: plainText(t.text) }))
      .filter((t) => t.subject || t.text || t.tag === 'deprecated'),
  };
}

/** How a tag heads its row. `@param x` keeps its subject; the rest are the tag alone. */
function tagLabel(t: DocTag): string {
  const name = t.tag === 'exception' ? 'throws' : t.tag;
  return t.subject ? `${name} ${t.subject}` : name;
}

/** Build the shared `.cm-hover-card` DOM (styled in the editor theme). */
export function hoverCardDom(info: HoverCard): HTMLElement {
  const dom = document.createElement('div');
  dom.className = 'cm-hover-card';

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
  dom.appendChild(head);

  if (info.container) {
    const m = document.createElement('div');
    m.className = 'cm-hc-meta';
    m.textContent = info.container;
    dom.appendChild(m);
  }

  if (info.doc) {
    const { prose, tags } = parseDoc(info.doc);
    if (prose) {
      const d = document.createElement('div');
      d.className = 'cm-hc-doc';
      d.textContent = prose;
      dom.appendChild(d);
    }
    if (tags.length) {
      const dl = document.createElement('dl');
      dl.className = 'cm-hc-tags';
      for (const t of tags) {
        const dt = document.createElement('dt');
        dt.textContent = tagLabel(t);
        if (t.tag === 'deprecated') dt.className = 'cm-hc-deprecated';
        const dd = document.createElement('dd');
        dd.textContent = t.text;
        dl.appendChild(dt);
        dl.appendChild(dd);
      }
      dom.appendChild(dl);
    }
  }
  return dom;
}
