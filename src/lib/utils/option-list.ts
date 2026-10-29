/**
 * Filtering and grouping of long option lists — app-agnostic, behind the shared `OptionPicker`.
 */

import { fuzzyMatch } from '$lib/utils/fuzzy';

export interface ListOption {
  value: string;
  label: string;
  description?: string;
  group?: string;
}

export interface OptionGroup {
  group: string;
  items: ListOption[];
}

/**
 * Options matching `query`, best first. An empty query keeps the service's own order — for
 * Spring and Quarkus that order is meaningful (groups, then popularity).
 *
 * The haystack is label + id + group + description: someone looking for "orm" should find
 * Hibernate whether the word is in its name, its id or its blurb. Name hits outrank the rest.
 */
export function filterOptions(options: readonly ListOption[], query: string): ListOption[] {
  const q = query.trim();
  if (!q) return [...options];
  const scored: { opt: ListOption; score: number; index: number }[] = [];
  options.forEach((opt, index) => {
    const byName = fuzzyMatch(`${opt.label} ${opt.value}`, q);
    const byRest = byName ? null : fuzzyMatch(`${opt.group ?? ''} ${opt.description ?? ''}`, q);
    const hit = byName ?? byRest;
    // A match in the blurb alone is a weaker signal than one in the name.
    if (hit) scored.push({ opt, score: hit.score - (byName ? 0 : 100), index });
  });
  scored.sort((a, b) => b.score - a.score || a.index - b.index);
  return scored.map((s) => s.opt);
}

/** Consecutive options under the same heading, in order; ungrouped ones go under "". */
export function groupOptions(options: readonly ListOption[]): OptionGroup[] {
  const groups: OptionGroup[] = [];
  const byName = new Map<string, OptionGroup>();
  for (const opt of options) {
    const name = opt.group ?? '';
    let g = byName.get(name);
    if (!g) {
      g = { group: name, items: [] };
      byName.set(name, g);
      groups.push(g);
    }
    g.items.push(opt);
  }
  return groups;
}

/** `ids` with `id` added or removed. */
export function toggled(ids: readonly string[], id: string): string[] {
  return ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id];
}
