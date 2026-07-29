/**
 * Fuzzy matching for the navigators — app-agnostic, no Arbor concepts.
 *
 * Two properties are what make a "go to file" box feel like a tool rather than a
 * filter, and both are here rather than in each consumer:
 *
 *  • **Subsequence, not substring.** `agpo` finds `AGGIORNAMENTO/POS` because the
 *    letters appear in order, not because they appear together. Typing four
 *    characters to reach a file eight directories deep is the whole point.
 *  • **Scored, not merely accepted.** Everything matches something once you allow
 *    gaps, so the ranking *is* the feature: a hit at the start of a word beats one
 *    in the middle, a consecutive run beats a scattered one, and a short name beats
 *    a long one that happens to contain the same letters.
 *
 * Multi-term: whitespace splits the query and **every** term must match somewhere,
 * each independently. That is what makes `agg pos` work on
 * `AGGIORNAMENTO/2024/POS` — one term lands in the first segment and one in the
 * last, which no single subsequence pass over the whole string would rank well.
 *
 * Case-insensitive throughout, with a bonus for a case-exact hit so a typed
 * capital still means something without ever excluding a result.
 */

/** One matched span of the haystack, for highlighting. Half-open, `[from, to)`. */
export interface MatchRange {
  from: number;
  to: number;
}

export interface FuzzyMatch {
  /** Higher is better. Only comparable between candidates of the same query. */
  score: number;
  /** Where the query's characters landed, merged and in order. */
  ranges: MatchRange[];
}

/** Characters after which the next one counts as starting a word. */
const BOUNDARY = /[\s/\\._\-:>]/;

const SCORE = {
  /** Every matched character is worth having. */
  base: 1,
  /** Immediately after the previous match — the run everyone is aiming for. */
  consecutive: 8,
  /** First character of a word: `p` in `.../POS`. */
  wordStart: 10,
  /** The very first character of the haystack. */
  head: 12,
  /** Typed the same case as it appears. */
  exactCase: 2,
  /** Per character skipped before the first hit — a late match is a worse match. */
  leadingGap: -0.5,
  /** Per character skipped between hits. */
  gap: -0.2,
} as const;

/**
 * Score one candidate against one already-lowercased term.
 *
 * Greedy left-to-right rather than an optimal alignment: the optimum needs
 * quadratic work per candidate and these lists run to thousands of entries on
 * every keystroke. Greedy plus the word-start bonus gets the ordering right on
 * the shapes that actually occur — paths, identifiers, camelCase — and stays
 * linear.
 */
function scoreTerm(haystack: string, lowerHaystack: string, term: string): FuzzyMatch | null {
  if (!term) return { score: 0, ranges: [] };

  const ranges: MatchRange[] = [];
  let score = 0;
  let at = 0;
  let previous = -2;

  for (let i = 0; i < term.length; i++) {
    const found = lowerHaystack.indexOf(term[i], at);
    if (found === -1) return null;

    let points: number = SCORE.base;
    if (found === previous + 1) points += SCORE.consecutive;
    if (found === 0) points += SCORE.head;
    else if (BOUNDARY.test(haystack[found - 1])) points += SCORE.wordStart;
    if (haystack[found] === term[i]) points += SCORE.exactCase;

    const skipped = found - (previous + 1);
    if (skipped > 0) points += skipped * (previous === -2 ? SCORE.leadingGap : SCORE.gap);

    score += points;
    // Extend the previous range when adjacent, so a run highlights as one span.
    const last = ranges[ranges.length - 1];
    if (last && last.to === found) last.to = found + 1;
    else ranges.push({ from: found, to: found + 1 });

    previous = found;
    at = found + 1;
  }

  return { score, ranges };
}

/**
 * Match a whitespace-separated query against one string.
 *
 * `null` when any term fails. An empty query matches everything with score 0,
 * which is what lets a navigator show its whole list before anything is typed.
 */
export function fuzzyMatch(haystack: string, query: string): FuzzyMatch | null {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return { score: 0, ranges: [] };

  const lower = haystack.toLowerCase();
  const ranges: MatchRange[] = [];
  let score = 0;

  for (const term of terms) {
    const hit = scoreTerm(haystack, lower, term);
    if (!hit) return null;
    score += hit.score;
    ranges.push(...hit.ranges);
  }

  // Shorter is better when the score ties: a query that matches both `POS` and
  // `POSIZIONI_STORICHE` almost always meant the first.
  score -= haystack.length * 0.05;
  return { score, ranges: mergeRanges(ranges) };
}

/**
 * Match against a name and a longer context (a path), preferring the name.
 *
 * The two-field shape every navigator has: `Config.java` shown big, its directory
 * shown small. A hit in the name is worth far more than one in the directory —
 * otherwise typing `src` ranks every file in `src/` above the file actually
 * called `src`. Both are still searched, because "which of the four `index.ts`"
 * is answered by the path and nothing else.
 */
export function fuzzyMatchPair(
  name: string,
  detail: string,
  query: string,
): { score: number; nameRanges: MatchRange[]; detailRanges: MatchRange[] } | null {
  if (!query.trim()) return { score: 0, nameRanges: [], detailRanges: [] };

  const onName = fuzzyMatch(name, query);
  if (onName) return { score: onName.score * 2, nameRanges: onName.ranges, detailRanges: [] };

  // Fall back to the whole path. The name's own characters are in there too, so a
  // query spanning both — `pic gen view` — is matched here rather than nowhere.
  const whole = `${detail}/${name}`;
  const onWhole = fuzzyMatch(whole, query);
  if (!onWhole) return null;

  const split = detail.length + 1;
  const nameRanges: MatchRange[] = [];
  const detailRanges: MatchRange[] = [];
  for (const r of onWhole.ranges) {
    if (r.to <= split) detailRanges.push(r);
    else if (r.from >= split) nameRanges.push({ from: r.from - split, to: r.to - split });
    else {
      // A run straddling the separator — split it rather than dropping either half.
      detailRanges.push({ from: r.from, to: split });
      nameRanges.push({ from: 0, to: r.to - split });
    }
  }
  return { score: onWhole.score, nameRanges, detailRanges };
}

/** Merge overlapping and touching ranges so highlighting never nests. */
function mergeRanges(ranges: MatchRange[]): MatchRange[] {
  if (ranges.length < 2) return ranges;
  const sorted = [...ranges].sort((a, b) => a.from - b.from);
  const out: MatchRange[] = [sorted[0]];
  for (const r of sorted.slice(1)) {
    const last = out[out.length - 1];
    if (r.from <= last.to) last.to = Math.max(last.to, r.to);
    else out.push({ ...r });
  }
  return out;
}

/** One run of a string, flagged for highlighting. */
export interface Segment {
  text: string;
  hit: boolean;
}

/**
 * Cut a string into alternating plain and matched runs.
 *
 * Returned as data rather than as HTML on purpose: the caller renders it with
 * Svelte's own escaping, so a file called `<script>.sql` cannot become markup.
 */
export function segments(text: string, ranges: MatchRange[]): Segment[] {
  if (!ranges.length) return [{ text, hit: false }];
  const out: Segment[] = [];
  let at = 0;
  for (const r of ranges) {
    if (r.from > at) out.push({ text: text.slice(at, r.from), hit: false });
    out.push({ text: text.slice(r.from, r.to), hit: true });
    at = r.to;
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: false });
  return out;
}
