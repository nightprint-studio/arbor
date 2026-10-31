/**
 * The process monitor's memory breakdown, sorted into a handful of headings so a backend's twenty lines read as four groups.
 *
 * A backend names what it holds in words (`Reference index (use sites)`), and the monitor has no field to say which part of
 * the product a line belongs to, so the heading is read off the label. The rules are the labels the backends write today;
 * a line none of them recognises lands under "Other" rather than being hidden. A label of the form `Group · text` names its
 * own heading and the text after the dot is what is shown.
 */

import type { MemoryItem } from '$lib/ipc/processes';

/** Headings in the order they are shown; a heading with no lines is not drawn. */
const ORDER = ['What Bennu learned', 'Classes and libraries', 'Code index and checks', 'Language servers', 'Catalogues', 'Other'] as const;

const RULES: [heading: (typeof ORDER)[number], test: RegExp][] = [
  [ 'Classes and libraries', /class names|classes decoded|types kept|annotation lookups|library|go to class|jar/i ],
  [ 'Code index and checks', /reference index|parsed symbols|files tracked|symbol index|cached validation|include graphs|incremental/i ],
  [ 'Language servers', /server|diagnostics|completion list|open files/i ],
  [ 'Catalogues', /shader|crate|catalogue/i ],
];

/** `Learned · x` → `What Bennu learned` / `x`; anything else is read by its words. */
export function categoryOf(item: Pick<MemoryItem, 'label'>): { heading: string; text: string } {
  const dot = item.label.indexOf(' · ');
  if (item.label.startsWith('Learned') && dot > 0) return { heading: 'What Bennu learned', text: item.label.slice(dot + 3) };
  const hit = RULES.find(([, test]) => test.test(item.label));
  return { heading: hit ? hit[0] : 'Other', text: item.label };
}

export interface MemoryCategory {
  heading: string;
  bytes: number;
  items: (MemoryItem & { text: string })[];
}

/** The items of one scope, under their headings: headings in a fixed order, lines largest first inside each. */
export function groupByCategory(items: MemoryItem[]): MemoryCategory[] {
  const by = new Map<string, MemoryCategory>();
  for (const item of items) {
    const { heading, text } = categoryOf(item);
    const group = by.get(heading) ?? { heading, bytes: 0, items: [] };
    group.items.push({ ...item, text });
    if (!item.mapped) group.bytes += item.bytes ?? 0;
    by.set(heading, group);
  }
  const rank = (h: string) => ORDER.indexOf(h as (typeof ORDER)[number]);
  return [...by.values()]
    .map((g) => ({ ...g, items: g.items.sort((a, b) => (b.bytes ?? -1) - (a.bytes ?? -1)) }))
    .sort((a, b) => rank(a.heading) - rank(b.heading));
}
