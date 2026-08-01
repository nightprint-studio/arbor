//! Subsequence scorer for the quick switcher (`Ctrl+O`) and for ranking
//! full-text hits by how well the query also matches the title.
//!
//! Deliberately hand-written rather than pulled from a crate: the ranking is a
//! product decision that gets tuned against real vault titles, and the whole
//! thing is a hundred lines of dynamic programming.
//!
//! The recurrence is the classic two-matrix one (fzy / fzf):
//!
//! * `d[i][j]` — best score for `needle[0..=i]` where `needle[i]` is matched at
//!   `haystack[j]`. Carrying this separately is what lets a *consecutive* run be
//!   rewarded: it is the only state that knows the previous needle char landed
//!   on the previous haystack char.
//! * `m[i][j]` — best score for `needle[0..=i]` anywhere in `haystack[0..=j]`.
//!
//! The final score is `max_j d[last][j]`: trailing characters after the last
//! match are *not* penalised, otherwise a long title with a perfect prefix would
//! lose to a short title with a scattered match. Length is used as a tie-break
//! by the caller (see `Index::quick_switch`), not baked into the score.

use serde::{Deserialize, Serialize};

/// Base reward for landing a needle character at all.
const SCORE_MATCH: i32 = 16;
/// Per-character cost of skipping haystack characters between two matches.
const SCORE_GAP: i32 = -1;
/// First character of the haystack — a prefix match should always win.
const BONUS_FIRST_CHAR: i32 = 10;
/// Match right after a separator (`Nota di lavoro` matched at `l`).
const BONUS_BOUNDARY: i32 = 8;
/// Match at a camelCase hump (`fooBar` matched at `B`).
const BONUS_CAMEL: i32 = 6;
/// Match directly after the previous needle character.
const BONUS_CONSECUTIVE: i32 = 12;

/// Sentinel for "unreachable". Half of `i32::MIN` so that adding a penalty to it
/// cannot overflow.
const NEG: i32 = i32::MIN / 2;

/// A successful subsequence match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzyMatch {
    /// Higher is better. Only comparable between matches of the *same* needle.
    pub score: i32,
    /// Byte offsets into the haystack of the matched characters, ascending.
    /// The UI underlines exactly these.
    pub positions: Vec<usize>,
}

/// Characters that start a new "word" for bonus purposes.
fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '\t' | '-' | '_' | '/' | '\\' | '.' | ',' | ':' | ';' | '(' | ')' | '[' | ']')
}

/// Lowercase a char to a single char. Multi-char lowercase expansions (`İ`) are
/// truncated on purpose: the haystack index has to stay aligned with byte
/// offsets, and a one-character approximation only ever costs a missed match on
/// an exotic title.
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Score `needle` against `haystack`, or `None` if `needle` is not a
/// subsequence of it.
///
/// An empty needle matches everything with score `0` and no positions, which is
/// what makes an empty quick-switch box list the whole vault.
pub fn score(needle: &str, haystack: &str) -> Option<FuzzyMatch> {
    let n: Vec<char> = needle.chars().filter(|c| !c.is_whitespace()).map(lower).collect();
    if n.is_empty() {
        return Some(FuzzyMatch { score: 0, positions: Vec::new() });
    }
    let h: Vec<HayChar> = haystack
        .char_indices()
        .map(|(offset, raw)| HayChar { offset, raw, low: lower(raw) })
        .collect();
    if n.len() > h.len() || !is_subsequence(&n, &h) {
        return None;
    }

    let bonuses = position_bonuses(&h);
    let (d, m) = fill(&n, &h, &bonuses);
    let width = h.len();
    let last = (n.len() - 1) * width;

    let (end, best) = (0..width)
        .map(|j| (j, d[last + j]))
        .filter(|&(_, s)| s > NEG)
        .max_by_key(|&(j, s)| (s, std::cmp::Reverse(j)))?;

    Some(FuzzyMatch { score: best, positions: backtrack(&n, &h, &d, &m, end) })
}

/// Whether `needle` is a subsequence of `haystack` at all, ignoring quality.
///
/// Skips the matrices entirely, so this is the right filter to run over a whole
/// vault before scoring the survivors.
pub fn matches(needle: &str, haystack: &str) -> bool {
    let n: Vec<char> = needle.chars().filter(|c| !c.is_whitespace()).map(lower).collect();
    let mut haystack = haystack.chars().map(lower);
    n.iter().all(|&nc| haystack.any(|hc| hc == nc))
}

/// One haystack character with everything the scorer needs about it.
struct HayChar {
    offset: usize,
    raw: char,
    low: char,
}

fn is_subsequence(n: &[char], h: &[HayChar]) -> bool {
    let mut it = h.iter();
    n.iter().all(|&nc| it.any(|hc| hc.low == nc))
}

/// Positional bonus for landing a match on each haystack character.
fn position_bonuses(h: &[HayChar]) -> Vec<i32> {
    h.iter()
        .enumerate()
        .map(|(j, c)| {
            if j == 0 {
                BONUS_FIRST_CHAR + BONUS_BOUNDARY
            } else {
                let prev = h[j - 1].raw;
                if is_separator(prev) {
                    BONUS_BOUNDARY
                } else if prev.is_lowercase() && c.raw.is_uppercase() {
                    BONUS_CAMEL
                } else {
                    0
                }
            }
        })
        .collect()
}

/// Fill the `d` and `m` matrices, row-major, `h.len()` wide.
fn fill(n: &[char], h: &[HayChar], bonuses: &[i32]) -> (Vec<i32>, Vec<i32>) {
    let width = h.len();
    let mut d = vec![NEG; n.len() * width];
    let mut m = vec![NEG; n.len() * width];

    for i in 0..n.len() {
        let row = i * width;
        let mut prev_m = NEG; // m[i][j - 1]
        for j in 0..width {
            let cell = if n[i] == h[j].low {
                // Score of "everything before this match", and of "the previous
                // needle char matched at j-1" (which unlocks the run bonus).
                let (before, adjacent) = if i == 0 {
                    (j as i32 * SCORE_GAP, NEG)
                } else if j == 0 {
                    (NEG, NEG)
                } else {
                    (m[row - width + j - 1], d[row - width + j - 1])
                };
                let start_run = add(before, bonuses[j] + SCORE_MATCH);
                let extend_run = add(adjacent, bonuses[j].max(BONUS_CONSECUTIVE) + SCORE_MATCH);
                start_run.max(extend_run)
            } else {
                NEG
            };
            d[row + j] = cell;
            let carried = add(prev_m, SCORE_GAP);
            prev_m = cell.max(carried);
            m[row + j] = prev_m;
        }
    }
    (d, m)
}

/// Saturating add that keeps [`NEG`] absorbing.
fn add(base: i32, delta: i32) -> i32 {
    if base <= NEG {
        NEG
    } else {
        base + delta
    }
}

/// Walk the matrices backwards from the winning end position to recover which
/// haystack characters were matched.
fn backtrack(n: &[char], h: &[HayChar], d: &[i32], m: &[i32], end: usize) -> Vec<usize> {
    let width = h.len();
    let mut positions = Vec::with_capacity(n.len());
    let mut j = end;
    for i in (0..n.len()).rev() {
        positions.push(h[j].offset);
        if i == 0 {
            break;
        }
        // Step back to the cell the winning score came from: either the
        // adjacent one (a run) or the best earlier one.
        let row = (i - 1) * width;
        let mut best = j; // j > 0 always holds here: i chars still need slots.
        let mut best_score = NEG;
        for k in (0..j).rev() {
            let candidate = d[row + k].max(m[row + k]);
            if candidate > best_score {
                best_score = candidate;
                best = k;
            }
            // A run is strictly better than any earlier start of equal score.
            if k + 1 == j && d[row + k] > NEG {
                best = k;
                break;
            }
        }
        j = best;
    }
    positions.reverse();
    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(needle: &str, haystack: &str) -> i32 {
        score(needle, haystack).unwrap_or_else(|| panic!("{needle:?} should match {haystack:?}")).score
    }

    #[test]
    fn a_non_subsequence_does_not_match() {
        assert!(score("abc", "acb").is_none());
        assert!(score("abcd", "abc").is_none());
        assert!(score("z", "abc").is_none());
    }

    #[test]
    fn an_empty_needle_matches_everything() {
        let m = score("", "qualsiasi cosa").unwrap();
        assert_eq!(m.score, 0);
        assert!(m.positions.is_empty());
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(matches("ABC", "abcdef"));
        assert!(matches("abc", "ABCDEF"));
    }

    #[test]
    fn ranking_prefers_exact_then_boundary_then_scattered() {
        let exact = s("abc", "abc");
        let boundary = s("abc", "a-b-c");
        let scattered = s("abc", "axbxc");
        assert!(exact > boundary, "{exact} !> {boundary}");
        assert!(boundary > scattered, "{boundary} !> {scattered}");
    }

    #[test]
    fn ranking_prefers_a_prefix_over_a_late_run() {
        assert!(s("abc", "abcdef") > s("abc", "xxabc"));
    }

    #[test]
    fn a_consecutive_run_beats_the_same_chars_spread_out() {
        assert!(s("abc", "xxabc") > s("abc", "xaxbxc"));
    }

    #[test]
    fn camel_and_word_boundaries_are_rewarded() {
        assert!(s("nl", "note lavoro") > s("nl", "nolavoro"));
        assert!(s("fb", "fooBar") > s("fb", "foobar"));
    }

    #[test]
    fn trailing_text_is_not_penalised() {
        // Same quality of match, only the tail differs: the caller breaks the
        // tie on length, the scorer must not.
        assert_eq!(s("abc", "abc"), s("abc", "abcdefghijklmnop"));
    }

    #[test]
    fn positions_are_byte_offsets_of_the_matched_characters() {
        let m = score("ac", "abc").unwrap();
        assert_eq!(m.positions, vec![0, 2]);
    }

    #[test]
    fn positions_survive_multibyte_haystacks() {
        // "però nota": 'ò' is two bytes, so 'n' does not sit at char index 5.
        let m = score("n", "però nota").unwrap();
        let offset = m.positions[0];
        assert_eq!(&"però nota"[offset..offset + 1], "n");
    }

    #[test]
    fn a_whitespace_needle_is_treated_as_empty() {
        assert_eq!(score("  ", "abc").unwrap().score, 0);
    }
}
