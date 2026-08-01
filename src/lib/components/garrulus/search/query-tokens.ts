/**
 * The search bar's structured tokens, as the chips the query box draws.
 *
 * **The backend owns the grammar.** `garrulus-index`'s `parse_query` is the
 * contract (`type:bug stato:aperto free text`, `#tag`, `key:!value`, `sort:-title`),
 * it is total — nothing it fails to read is an error, it simply becomes free text
 * — and it is tested there. This module is a *mirror* whose only job is deciding
 * what looks like a filter so it can be rendered as a chip instead of as prose.
 *
 * That distinction is what keeps the mirror harmless. Everything typed is sent to
 * the backend as one reassembled string, so if this file and the Rust one ever
 * disagree the result is a token drawn as text rather than as a chip — never a
 * query that means something different from what was typed. Nothing here decides
 * what matches.
 */

/** What a chip turned out to be. `field` is the open case: the query language has
 *  no type registry, so any unreserved key is a frontmatter field filter. */
export type TokenKind = 'type' | 'tag' | 'field' | 'sort';

/** One structured term of a query, in the form the chips render. */
export interface QueryToken {
  kind: TokenKind;
  /** The key as written, lowercased — `''` for a `#tag`, which has none. */
  key: string;
  /** The comparison operator as written: `''`, `!`, `~`, `>`, `>=`, `<`, `<=`. */
  op: string;
  /** The right-hand side, unquoted. */
  value: string;
}

/** A query split into the part that renders as chips and the part still being
 *  typed. */
export interface ParsedQuery {
  tokens: QueryToken[];
  /** Everything that was not a filter, space-joined. */
  text: string;
}

/** Operators, longest first so `>=` is not read as `>` followed by `=`. */
const OPERATORS = ['>=', '<=', '!', '~', '>', '<'] as const;

/** A key must look like an identifier. Unicode-aware, like the Rust side: a
 *  vault written in Italian has `gravità:` as a legitimate field. */
const KEY_RE = /^[\p{L}_][\p{L}\p{N}_.-]*$/u;

/**
 * Split on whitespace, keeping `"quoted runs"` together.
 *
 * The quotes stay on the token, exactly as in the Rust splitter, so a token that
 * turns out not to be a filter is unquoted once at the end rather than twice on
 * two paths.
 */
function splitTokens(input: string): string[] {
  const out: string[] = [];
  let current = '';
  let quote: string | null = null;

  for (const c of input) {
    if (quote) {
      current += c;
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'") {
      quote = c;
      current += c;
      continue;
    }
    if (/\s/.test(c)) {
      if (current) out.push(current);
      current = '';
      continue;
    }
    current += c;
  }
  if (current) out.push(current);
  return out;
}

/** Drop one matching pair of surrounding quotes. */
function unquote(s: string): string {
  for (const q of ['"', "'"]) {
    if (s.length >= 2 && s.startsWith(q) && s.endsWith(q)) return s.slice(1, -1);
  }
  return s;
}

function splitOp(value: string): { op: string; rest: string } {
  for (const op of OPERATORS) {
    if (value.startsWith(op)) return { op, rest: value.slice(op.length) };
  }
  return { op: '', rest: value };
}

/** Read one whitespace-delimited token, or `null` when it is free text. */
export function classifyToken(token: string): QueryToken | null {
  if (token.startsWith('#')) {
    const tag = unquote(token.slice(1));
    return tag ? { kind: 'tag', key: '', op: '', value: tag } : null;
  }

  const at = token.indexOf(':');
  if (at === -1) return null;

  const key = token.slice(0, at).toLowerCase();
  const { op, rest } = splitOp(token.slice(at + 1));
  const value = unquote(rest);

  // `12:30`, `sort:` and `https://example.com` all look like filters and are not.
  if (!KEY_RE.test(key) || !value || value.startsWith('//')) return null;

  if (key === 'sort') return { kind: 'sort', key, op: '', value };
  if (key === 'type') return { kind: 'type', key, op: '', value };
  if (key === 'tag') return { kind: 'tag', key: '', op: '', value };
  return { kind: 'field', key, op, value };
}

/** Split a raw query into its chips and the free text that is left. */
export function parseQuery(raw: string): ParsedQuery {
  const tokens: QueryToken[] = [];
  const words: string[] = [];

  for (const token of splitTokens(raw)) {
    const parsed = classifyToken(token);
    if (parsed) tokens.push(parsed);
    else words.push(unquote(token));
  }

  return { tokens, text: words.join(' ') };
}

/** Quote a value the splitter would otherwise tear in half. */
function quoteIfNeeded(value: string): string {
  return /\s/.test(value) ? `"${value}"` : value;
}

/** One chip, back in the spelling the backend parses. */
export function formatToken(token: QueryToken): string {
  if (token.kind === 'tag') return `#${quoteIfNeeded(token.value)}`;
  return `${token.key}:${token.op}${quoteIfNeeded(token.value)}`;
}

/** The whole query, as the one string that crosses the seam. */
export function buildQuery(tokens: readonly QueryToken[], text: string): string {
  return [...tokens.map(formatToken), text.trim()].filter(Boolean).join(' ');
}

/** What the chip shows before the value — the coloured half. `#` for a tag, so a
 *  tag chip reads as the tag it filters on rather than as a bare word. */
export function tokenPrefix(token: QueryToken): string {
  return token.kind === 'tag' ? '#' : `${token.key}:${token.op}`;
}

/** Whether two chips are the same constraint — so adding a filter twice from the
 *  sidebar toggles it rather than stacking a duplicate the backend would apply
 *  twice for nothing. */
export function sameToken(a: QueryToken, b: QueryToken): boolean {
  return (
    a.kind === b.kind &&
    a.key === b.key &&
    a.op === b.op &&
    a.value.toLowerCase() === b.value.toLowerCase()
  );
}

/** A `key:value` filter chip, for the sidebar rows that push one into search. */
export function fieldToken(key: string, value: string): QueryToken {
  return { kind: 'field', key: key.toLowerCase(), op: '', value };
}

/** A `type:` chip. */
export function typeToken(typeId: string): QueryToken {
  return { kind: 'type', key: 'type', op: '', value: typeId };
}

/** A `#tag` chip. */
export function tagToken(tag: string): QueryToken {
  return { kind: 'tag', key: '', op: '', value: tag.replace(/^#/, '') };
}
