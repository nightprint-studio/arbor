/**
 * How a provider's relevance ordering survives CodeMirror's.
 *
 * CodeMirror scores completions by fuzzy-matching the label against what was typed and adds
 * `boost` to the result. There is no "already sorted, leave it alone" — so a provider that worked
 * out which member you meant has exactly one lever, and every consumer that reaches for it has to
 * reach for the same curve or the two lists rank differently for no reason anyone can see.
 *
 * ## A nudge, not an override
 *
 * The spread is deliberately narrower than CodeMirror's own match scores can be. The top of the
 * provider's list gets a strong lift and the tail a small penalty, so between two items the
 * provider ranked near each other CodeMirror's match quality still decides — which is the thing it
 * is genuinely better at, because it is the only one of the two that saw the keystrokes.
 *
 * What the provider wins is the argument it should win: `list.` opening on `add` and `addAll`
 * rather than on `clone`, `equals` and `getClass`.
 */

/** The band a set of completions is ranked within — see {@link boostForRank}. */
export interface RankBand {
  /** Boost for the first item. */
  top: number;
  /** The floor the tail cannot sink below. */
  floor: number;
}

/**
 * The default band: what a provider that knows what it is talking about gets.
 *
 * Chosen so a semantically-resolved member is above anything guessed from the buffer even when
 * the guess matches the typed prefix better.
 */
export const RESOLVED: RankBand = { top: 60, floor: -50 };

/**
 * The band for **templates** — postfix expansions and the like.
 *
 * Below every resolved member and above the guesses, which is exactly where they belong: after
 * `order.` the thing you almost always want is a member of `order`, and a template that outranked
 * them would put `if` where `getId` belongs. A template still wins the moment you have typed enough
 * of its name to beat the members on match quality — which is the moment you meant it.
 *
 * Sitting between the two bands rather than inside `RESOLVED` is the whole point. Left in
 * `RESOLVED`, templates and members alternated down the popup on equal boosts, and a list where
 * every other row is an `if` reads as a list that has lost the members it should be showing.
 */
export const TEMPLATE: RankBand = { top: -20, floor: -45 };

/**
 * The band for candidates offered because nothing better was available — language keywords, words
 * scraped out of the buffer. Entirely below {@link RESOLVED}'s floor: they are worth offering and
 * never worth putting first, and a fixed band says so once instead of each consumer inventing a
 * number.
 */
export const FALLBACK: RankBand = { top: -60, floor: -90 };

/**
 * The `boost` for the item at `rank` (0-based) in a provider's own ordering.
 *
 * `preselect` is the provider saying "this one" outright, and it wins over any rank.
 */
export function boostForRank(rank: number, band: RankBand = RESOLVED, preselect = false): number {
  if (preselect) return 99;
  return Math.max(band.floor, band.top - rank);
}
