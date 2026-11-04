/**
 * The test text cut into runs that share one look, for the highlighted mirror behind the text box.
 *
 * Every boundary of every match and group becomes a cut point; each run between two cuts then knows
 * which match it is in (alternating tones so adjacent matches stay distinguishable) and the innermost
 * group covering it (so a capture shows inside its match).
 */

import type { RegexMatch } from './types';

export interface Segment {
  text: string;
  /** Index of the match covering the run, or −1. */
  match: number;
  /** Number of the innermost group covering it (1-based), or 0 for match-only text. */
  group: number;
  /** The run lies inside the range being emphasised (a match hovered in the list). */
  active: boolean;
}

export function segmentsOf(
  text: string, matches: RegexMatch[], emphasis: { from: number; to: number } | null = null,
): Segment[] {
  if (matches.length === 0) return [{ text, match: -1, group: 0, active: false }];
  const cuts = new Set<number>([0, text.length]);
  if (emphasis) { cuts.add(emphasis.from); cuts.add(emphasis.to); }
  for (const m of matches) {
    cuts.add(m.start);
    cuts.add(m.end);
    for (const g of m.groups) if (g) { cuts.add(g.start); cuts.add(g.end); }
  }
  const points = [...cuts].filter((p) => p >= 0 && p <= text.length).sort((a, b) => a - b);
  const out: Segment[] = [];
  let mi = 0;
  for (let i = 0; i + 1 < points.length; i++) {
    const from = points[i];
    const to = points[i + 1];
    // Matches are ascending and disjoint; an empty one covers no run, so it is passed over here.
    while (mi < matches.length && matches[mi].end <= from) mi++;
    const m = mi < matches.length && matches[mi].start <= from && matches[mi].end >= to ? matches[mi] : null;
    let group = 0;
    if (m) {
      let narrowest = Infinity;
      m.groups.forEach((g, n) => {
        if (n === 0 || !g || g.start > from || g.end < to) return;
        const width = g.end - g.start;
        if (width <= narrowest) { narrowest = width; group = n; }
      });
    }
    const active = !!emphasis && from >= emphasis.from && to <= emphasis.to;
    out.push({ text: text.slice(from, to), match: m ? mi : -1, group, active });
  }
  return out;
}
