/**
 * Where a line of a YAML or `.properties` file sits — the chain of keys above it.
 *
 * A configuration file's meaning is its path (`spring.datasource.url`), and in YAML that path is
 * spread over indentation: at the bottom of a screen-long `spring:` block nothing on the line says
 * what it belongs to. This reads the structure back out of the text, pure and without a parser:
 * a key is a parent of the line when it sits to the left of it, and a list item counts as a level of
 * its own (`[2]`), because it is one.
 *
 * Deliberately a reader of the common shapes (block mappings, block sequences, `key: value` with
 * quotes), not a YAML implementation: flow collections (`{a: 1}`) and multi-line scalars are not
 * followed, and a line inside one simply reports the keys it can see.
 */

/** One level of the path, and the 1-based line it is written on. */
export interface PathSegment {
  label: string;
  line: number;
}

interface Entry {
  /** Column of the line's first character (the dash, for an item). */
  indent: number;
  /** Column of a leading `-`, or `null` when the line is not a sequence item. */
  dash: number | null;
  /** The key written on the line, quotes removed; `null` for a scalar item or a bare value. */
  key: string | null;
  /** Column the key (or the item's scalar) starts at. */
  keyIndent: number;
}

function entryOf(text: string): Entry | null {
  if (!text.trim() || /^\s*#/.test(text)) return null;
  const m = /^(\s*)(-\s+)?(.*)$/.exec(text);
  if (!m) return null;
  const lead = m[1].length;
  const dashLen = m[2]?.length ?? 0;
  const key = /^("[^"]*"|'[^']*'|[^\s:#'"-][^:#]*?|-[^\s:#][^:#]*?)\s*:(?:\s|$)/.exec(m[3]);
  return {
    indent: lead,
    dash: m[2] !== undefined ? lead : null,
    key: key ? key[1].replace(/^["']|["']$/g, '') : null,
    keyIndent: lead + dashLen,
  };
}

/** How many items of the same sequence precede the one at line index `at` (dash column `dash`). */
function itemIndex(entries: (Entry | null)[], at: number, dash: number): number {
  let index = 0;
  for (let k = at - 1; k >= 0; k--) {
    const g = entries[k];
    if (!g) continue;
    if (g.indent < dash) break;
    // A key at the sequence's own column ends it: `key:` followed by `- a` at the same indent.
    if (g.indent === dash && g.dash === null) break;
    if (g.dash === dash) index++;
  }
  return index;
}

/**
 * The path of the key at `line` (1-based) of a YAML document given as its lines. A blank or comment
 * line answers for the nearest line above it that has content, which is what "where am I" means
 * between two entries.
 */
export function yamlPath(lines: string[], line: number): PathSegment[] {
  const entries = lines.map(entryOf);
  let i = Math.min(line, lines.length) - 1;
  while (i >= 0 && !entries[i]) i--;
  if (i < 0) return [];

  const rev: PathSegment[] = [];
  const own = entries[i]!;
  if (own.key !== null) rev.push({ label: own.key, line: i + 1 });
  let ref = own.keyIndent;
  if (own.dash !== null) {
    rev.push({ label: `[${itemIndex(entries, i, own.dash)}]`, line: i + 1 });
    ref = own.dash;
  }

  for (let j = i - 1; j >= 0 && ref > 0; j--) {
    if (/^(---|\.\.\.)/.test(lines[j])) break; // another document
    const f = entries[j];
    if (!f) continue;
    if (f.dash !== null) {
      if (f.dash >= ref) continue;
      // `- parent:` is an item AND the key that owns what is deeper than it.
      if (f.key !== null && f.keyIndent < ref) rev.push({ label: f.key, line: j + 1 });
      rev.push({ label: `[${itemIndex(entries, j, f.dash)}]`, line: j + 1 });
      ref = f.dash;
    } else if (f.key !== null && f.indent < ref) {
      rev.push({ label: f.key, line: j + 1 });
      ref = f.indent;
    }
  }
  return rev.reverse();
}

/** The key of a `.properties` line, up to its separator (`=`, `:` or whitespace), or `null` for a
 *  comment, a blank line or a continuation. */
export function propertyKeyOf(text: string): string | null {
  const t = text.trimStart();
  if (!t || t.startsWith('#') || t.startsWith('!')) return null;
  const m = /^((?:\\.|[^=:\s\\])+)/.exec(t);
  return m ? m[1] : null;
}

/** The dotted path of the `.properties` key at `line` (1-based): `spring.datasource.url` → three levels. */
export function propertiesPath(lines: string[], line: number): PathSegment[] {
  const key = propertyKeyOf(lines[line - 1] ?? '');
  return key ? key.split('.').filter(Boolean).map((label) => ({ label, line })) : [];
}
