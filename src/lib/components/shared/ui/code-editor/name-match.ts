/**
 * Whether a name answers what was typed — the browser half of one rule.
 *
 * ## Why there are two copies
 *
 * The engine matches candidates when it builds the list; CodeMirror re-matches that list, locally,
 * on every further keystroke while the popup stays open (`validFor`). One round trip, then a
 * client-side filter — which is what makes the popup feel instant, and which means the rule has to
 * exist on both sides or the two disagree about the same list.
 *
 * They already did. The backend matched with a case-sensitive `startsWith`, this side re-filtered
 * with another one, and CodeMirror's own fuzzy matcher sat in between doing something else again:
 * `aah` reached `addAllowedHeader` only because the popup happened to have been opened on the `a`
 * and never asked again.
 *
 * The Rust original is `bennu-complete`'s `name_match`, and it is the one to change first — the
 * tests there are the specification. This is a transcription of it, deliberately small: the tiers
 * and the case rule and nothing else.
 */

/** How strictly the typed text's **case** must agree with the name's. */
export type MatchCase =
  /** Case is not consulted at all. */
  | 'ignore'
  /** Only the first letter, which in Java is the one that carries information. The default. */
  | 'first-letter'
  /** Every typed letter, humps included. */
  | 'all';

/**
 * How well `name` answers `typed` — `0` an exact prefix, `1` a prefix ignoring case, `2` the camel
 * humps, `null` no match at all. Lower is better, and the number is a ranking input as much as a
 * filter.
 */
export function matchTier(name: string, typed: string, mode: MatchCase = 'first-letter'): number | null {
  if (!typed) return 0;
  if (name.length >= typed.length) {
    if (name.startsWith(typed)) return 0;
    if (
      mode !== 'all' &&
      firstLetterOk(name, typed, mode) &&
      name.slice(0, typed.length).toLowerCase() === typed.toLowerCase()
    ) {
      return 1;
    }
  }
  return firstLetterOk(name, typed, mode) && humpsMatch(name, typed, mode) ? 2 : null;
}

/** Whether `name` answers `typed` at all. */
export function matchesName(name: string, typed: string, mode: MatchCase = 'first-letter'): boolean {
  return matchTier(name, typed, mode) !== null;
}

/** Whether the first letters agree. Checked case-insensitively at minimum: a name starting with a
 *  different letter entirely is not a candidate under any setting. */
function firstLetterOk(name: string, typed: string, mode: MatchCase): boolean {
  if (!name || !typed) return false;
  return mode === 'ignore'
    ? name[0].toLowerCase() === typed[0].toLowerCase()
    : name[0] === typed[0];
}

/**
 * Whether the typed letters walk `name`'s word boundaries: after the first character, each one
 * either continues the word it is in or jumps to the next hump that matches it.
 *
 * A **hump** is an uppercase letter or the character after a `_`, so `MAX_V` reaching `MAX_VALUE`
 * is the same gesture as `aAE` reaching `addAllElements`. Greedy — it is the last tier, and a rare
 * miss costs a suggestion rather than producing a wrong one.
 */
function humpsMatch(name: string, typed: string, mode: MatchCase): boolean {
  const same = (a: string, b: string) => (mode === 'all' ? a === b : a.toLowerCase() === b.toLowerCase());
  let at = 1; // the first letter is settled by `firstLetterOk`
  for (let k = 1; k < typed.length; k++) {
    const want = typed[k];
    if (at < name.length && same(name[at], want)) {
      at++;
      continue;
    }
    let found = -1;
    for (let i = at; i < name.length; i++) {
      if (isHump(name, i) && same(name[i], want)) {
        found = i;
        break;
      }
    }
    if (found < 0) return false;
    at = found + 1;
  }
  return true;
}

/** Whether `name[i]` starts a word: an uppercase letter, or the character after a `_`. */
function isHump(name: string, i: number): boolean {
  if (i === 0) return true;
  return name[i] !== name[i].toLowerCase() || name[i - 1] === '_';
}
