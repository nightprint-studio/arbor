/**
 * Counting notes per filter, for the sidebar's Tags and Types sections.
 *
 * **There is no facet endpoint, and this does not pretend there is one.** What the
 * backend answers is `garrulus_search`, and a query made only of filters selects
 * every note that satisfies them — so "how many notes are of type bug" is
 * `search('type:bug').length`, asked of the index in memory. That is one round
 * trip per row, which is why the panels ask for a group's counts when the group is
 * opened rather than for every count the moment the section appears.
 *
 * The vocabulary itself — which fields exist, and which values they take — comes
 * from the note types, which are already loaded (`garrulusVaultStore.types`) and
 * are the vault's own declaration of what its notes are made of. A field whose
 * `values` list is empty is an *open* enum by design (`vault/src/builtin.rs`: the
 * dropdown offers what the vault already mentions and accepts a new value), and
 * enumerating one would need an index call that does not exist. Those fields are
 * left out rather than shown with a wrong or empty list.
 */

import { search, type FieldSpec, type NoteType } from '$lib/ipc/garrulus';

/**
 * Run `fn` over `items` with at most `limit` in flight.
 *
 * Every one of these is a framed-stdio round trip: firing forty at once buys
 * nothing over eight and makes the backend's queue the thing that decides when
 * the panel paints. Order is preserved, and a rejection is left to the caller's
 * `fn` — this never swallows one.
 */
export async function mapWithLimit<T, R>(
  items: readonly T[],
  limit: number,
  fn: (item: T, index: number) => Promise<R>,
): Promise<R[]> {
  const out = new Array<R>(items.length);
  let next = 0;

  const workers = Array.from({ length: Math.max(1, Math.min(limit, items.length)) }, async () => {
    for (;;) {
      const i = next++;
      if (i >= items.length) return;
      out[i] = await fn(items[i], i);
    }
  });

  await Promise.all(workers);
  return out;
}

/** How many notes a query selects. A failed count is `null`, never 0: "the
 *  backend did not answer" and "no note matches" are different facts and a row
 *  that shows the second when the first is true is a row that lies. */
export async function countMatching(query: string): Promise<number | null> {
  try {
    return (await search(query)).length;
  } catch {
    return null;
  }
}

/** One frontmatter field the vault can be filtered by, with the values its types
 *  declare for it. */
export interface FieldFacet {
  /** The frontmatter key — what goes to the left of the colon in a query. */
  key: string;
  /** What the types call it, in the user's language. */
  label: string;
  /** The declared values, deduplicated across every type that declares the key. */
  values: string[];
}

/** Fields worth a group: enumerable ones, in a stable order. */
function isEnumerable(field: FieldSpec): boolean {
  return field.values.length > 0;
}

/**
 * The vault's filterable field vocabulary, merged across its note types.
 *
 * Two types declaring `status` are one axis with one set of values, not two
 * groups the user has to read as one: a note is matched by `status:aperto`
 * whichever type declared it, so the panel says so once.
 */
export function fieldFacets(types: readonly NoteType[]): FieldFacet[] {
  const byKey = new Map<string, FieldFacet>();

  for (const type of types) {
    for (const field of type.fields) {
      if (!isEnumerable(field)) continue;
      const key = field.key.toLowerCase();
      const facet = byKey.get(key) ?? { key, label: field.label || field.key, values: [] };
      for (const value of field.values) {
        if (!facet.values.some((v) => v.toLowerCase() === value.toLowerCase())) {
          facet.values.push(value);
        }
      }
      byKey.set(key, facet);
    }
  }

  return [...byKey.values()].sort((a, b) => a.label.localeCompare(b.label));
}
