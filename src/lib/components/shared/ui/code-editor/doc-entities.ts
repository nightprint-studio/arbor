/**
 * The HTML entities a documentation string actually uses, decoded to the characters they name.
 *
 * Shared by both doc renderers of the hover card: Javadoc is HTML and is full of them, and
 * markdown allows them too (rustdoc writes `&lt;` where a bare `<` would read as a tag).
 */

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
export function decodeEntities(s: string): string {
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
