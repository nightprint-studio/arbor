/**
 * The navigator's query language — app-agnostic, no Arbor concepts.
 *
 * A "go to file" box is mostly a fuzzy match, and mostly that is enough. The
 * cases it is *not* enough for are the ones that send people back to a file tree:
 *
 *  • "the newest one" — `sort:new`, because the file you want after a colleague's
 *    pull is almost always the one that just arrived;
 *  • "the SQL ones" — `ext:sql`, when a name is shared by a script, a backup and
 *    a `.orig` left behind by a merge;
 *  • "under this directory" — `in:AGGIORNAMENTO`, which is how you say *which* of
 *    the eleven `4_13.sql` you meant.
 *
 * Each is a directive typed inline, `key:value`, anywhere in the query; what is
 * left over is the fuzzy text. That keeps one input rather than growing a row of
 * controls beside it, and it means a query is a string — shareable, repeatable,
 * and something the caller can seed.
 *
 * Unknown keys are deliberately **not** directives: `TODO:` and `http://` occur in
 * real searches, and swallowing them would make the box refuse to find things for
 * a reason nobody could see. Anything not recognised stays fuzzy text.
 */

/** How a result list is ordered once it has been filtered. */
export type SortKey =
  /** Best fuzzy score first, then name. What you want while typing. */
  | 'relevance'
  | 'name'
  | 'name-desc'
  | 'path'
  | 'path-desc'
  /** Most recently modified first, where the host supplies a timestamp. */
  | 'new'
  | 'old';

export interface ParsedQuery {
  /** What is left after the directives — what gets fuzzy-matched. */
  text: string;
  sort: SortKey;
  /** Lower-cased extensions, without the dot. Empty means every extension. */
  extensions: string[];
  /** Lower-cased path fragments; an item's path must contain **all** of them. */
  within: string[];
  /** The directives that were recognised, for showing them back as chips. */
  directives: Directive[];
}

export interface Directive {
  key: string;
  value: string;
}

/** `sort:` values, and the spellings each accepts. */
const SORTS: Record<string, SortKey> = {
  relevance: 'relevance',
  best: 'relevance',
  name: 'name',
  'name-asc': 'name',
  'name-desc': 'name-desc',
  namedesc: 'name-desc',
  path: 'path',
  'path-asc': 'path',
  'path-desc': 'path-desc',
  new: 'new',
  newest: 'new',
  recent: 'new',
  old: 'old',
  oldest: 'old',
};

const KEYS = new Set(['sort', 'ext', 'in']);

/**
 * Read a query into its directives and its leftover text.
 *
 * Total: an unparseable directive value (`sort:sideways`) leaves the default in
 * place and the token stays as fuzzy text, so the box degrades to searching for
 * what was typed rather than to finding nothing.
 */
export function parseQuery(raw: string): ParsedQuery {
  const parsed: ParsedQuery = {
    text: '',
    sort: 'relevance',
    extensions: [],
    within: [],
    directives: [],
  };
  const rest: string[] = [];

  for (const token of raw.split(/\s+/)) {
    if (!token) continue;
    const colon = token.indexOf(':');
    const key = colon > 0 ? token.slice(0, colon).toLowerCase() : '';
    const value = colon > 0 ? token.slice(colon + 1) : '';
    if (!KEYS.has(key) || !value) {
      rest.push(token);
      continue;
    }

    if (key === 'sort') {
      const sort = SORTS[value.toLowerCase()];
      if (!sort) {
        rest.push(token);
        continue;
      }
      parsed.sort = sort;
    } else if (key === 'ext') {
      parsed.extensions.push(value.replace(/^\./, '').toLowerCase());
    } else {
      parsed.within.push(value.toLowerCase());
    }
    parsed.directives.push({ key, value });
  }

  parsed.text = rest.join(' ');
  return parsed;
}

/** What the user can be shown when they have not typed anything yet. */
export const QUERY_HELP: { syntax: string; means: string }[] = [
  { syntax: 'agg pos', means: 'every term has to match — in the name or in the path' },
  { syntax: 'sort:name-desc', means: 'order by name, Z to A' },
  { syntax: 'sort:new', means: 'most recently changed first' },
  { syntax: 'ext:sql', means: 'only files with that extension' },
  { syntax: 'in:AGGIORNAMENTO', means: 'only under a path containing that' },
];

/** Does an item's path satisfy the `ext:` and `in:` directives? */
export function passesFilters(path: string, parsed: ParsedQuery): boolean {
  const lower = path.toLowerCase();
  if (parsed.within.some((fragment) => !lower.includes(fragment))) return false;
  if (!parsed.extensions.length) return true;
  const dot = lower.lastIndexOf('.');
  const ext = dot === -1 ? '' : lower.slice(dot + 1);
  return parsed.extensions.includes(ext);
}

/** Something the navigator can order. Hosts supply whichever fields they have. */
export interface Sortable {
  name: string;
  path: string;
  /** Epoch milliseconds. Absent items sort last under `sort:new` / `sort:old`. */
  modified?: number;
  /** Fuzzy score for the current query; higher is better. */
  score: number;
}

/**
 * Compare two results under one sort key.
 *
 * Name is always the final tie-break, including under `relevance`: two files with
 * identical scores must not swap places between keystrokes, and a stable visible
 * order is what makes "second row, press Enter" a thing muscle memory can do.
 */
export function compareBy(key: SortKey): (a: Sortable, b: Sortable) => number {
  const byName = (a: Sortable, b: Sortable) => a.name.localeCompare(b.name);
  switch (key) {
    case 'name':
      return (a, b) => byName(a, b) || a.path.localeCompare(b.path);
    case 'name-desc':
      return (a, b) => byName(b, a) || a.path.localeCompare(b.path);
    case 'path':
      return (a, b) => a.path.localeCompare(b.path);
    case 'path-desc':
      return (a, b) => b.path.localeCompare(a.path);
    case 'new':
      return (a, b) => (b.modified ?? -Infinity) - (a.modified ?? -Infinity) || byName(a, b);
    case 'old':
      return (a, b) => (a.modified ?? Infinity) - (b.modified ?? Infinity) || byName(a, b);
    default:
      return (a, b) => b.score - a.score || byName(a, b);
  }
}
