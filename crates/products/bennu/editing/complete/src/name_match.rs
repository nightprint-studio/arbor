//! Whether a name answers what was typed — the one rule, for every kind of candidate.
//!
//! ## Why one rule
//!
//! It was two, and only one of them was any good. Type names were matched in three tiers — exact
//! prefix, then ignoring case, then the **camel humps**, so `SBA` reaches `SpringBootApplication`
//! — while every member, local and static import was matched with a literal, case-sensitive
//! `starts_with`. Which meant `s.tolc` found nothing where `s.toLowerCase` was the answer, and
//! `list.aAE` found nothing where `addAllElements` was.
//!
//! That the popup *appeared* to jump humps anyway is an accident worth naming, because it hid
//! this for a long time: the editor asks the backend once, when the popup opens on the first
//! letter, and then filters that cached list itself as you keep typing. So the humps you saw were
//! CodeMirror's, over whatever the first keystroke happened to fetch — and they stopped working
//! the moment you asked explicitly in the middle of a word, or turned case-sensitive matching on.
//!
//! ## The tiers
//!
//! Lower is better, and the tier is a **ranking** input as much as a filter: a name that starts
//! with what you typed should be offered above one that merely spells its humps that way.
//!
//! 0. **Exact prefix** — `toLo` → `toLowerCase`.
//! 1. **Prefix ignoring case** — `TOLO`, or a shift key held one letter too long.
//! 2. **Camel humps** — `tolc` → `toLowerCase`, `aAE` → `addAllElements`: the letters walk the
//!    name's word boundaries. This is how anyone who already knows a name reaches for it.
//!
//! ## The case rule is a separate axis
//!
//! "Match case" says which of those tiers are admissible, not which of them exist. IntelliJ's
//! three settings are the three that make sense, and they are [`MatchCase`]. The default is
//! [`MatchCase::FirstLetter`] for the same reason it is IntelliJ's: a Java name's first letter
//! carries real information — capital means a type — and the rest does not.

/// How strictly the typed text's **case** must agree with the name's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchCase {
    /// Case is not consulted at all: `springboot` reaches `SpringBootApplication`.
    Ignore,
    /// Only the first letter must agree. A capital means a type in Java and nothing else does, so
    /// this is the setting that keeps `Order` from offering the local `order` while still letting
    /// `tolc` reach `toLowerCase`. The default.
    #[default]
    FirstLetter,
    /// Every typed letter must agree, humps included: `aAE` reaches `addAllElements` and `aae`
    /// does not.
    All,
}

impl MatchCase {
    /// The mode a boolean "case-sensitive matching" setting means. `true` is [`Self::All`]; `false`
    /// is [`Self::FirstLetter`] rather than [`Self::Ignore`], because a setting that is off should
    /// leave the editor at its sensible default, not at its most permissive one.
    pub fn from_flag(strict: bool) -> Self {
        if strict {
            Self::All
        } else {
            Self::FirstLetter
        }
    }
}

/// What has been typed, and how strictly it is to be read. Carried instead of a bare `&str` so
/// that adding the case rule did not add an argument to nine call sites that each pass a prefix
/// along — and so that "does this name match" is asked of one thing rather than assembled at
/// every site.
#[derive(Debug, Clone, Copy)]
pub struct Typed<'a> {
    pub text: &'a str,
    pub case: MatchCase,
}

impl<'a> Typed<'a> {
    pub fn new(text: &'a str, case: MatchCase) -> Self {
        Self { text, case }
    }

    /// The default reading of a prefix — [`MatchCase::FirstLetter`].
    pub fn lenient(text: &'a str) -> Self {
        Self { text, case: MatchCase::default() }
    }

    /// An empty prefix admits everything, which is what `receiver.` means.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// How well `name` answers this, lower being better, or `None` for no match.
    pub fn tier(&self, name: &str) -> Option<u8> {
        tier(name, self.text, self.case)
    }

    /// Whether `name` answers this at all.
    pub fn matches(&self, name: &str) -> bool {
        self.tier(name).is_some()
    }
}

/// How well `name` answers `typed` under `case` — see the module docs for the tiers.
pub fn tier(name: &str, typed: &str, case: MatchCase) -> Option<u8> {
    if typed.is_empty() {
        return Some(0);
    }
    let (n, t) = (name.as_bytes(), typed.as_bytes());
    if n.len() < t.len() {
        // Not a prefix of anything, and the humps cannot rescue a name shorter than what was
        // typed either.
        return hump_tier(n, t, case);
    }
    if name.starts_with(typed) {
        return Some(0);
    }
    if case != MatchCase::All && first_letter_ok(n, t, case) && n[..t.len()].eq_ignore_ascii_case(t)
    {
        return Some(1);
    }
    hump_tier(n, t, case)
}

/// The camel-hump tier, gated on the case rule.
fn hump_tier(name: &[u8], typed: &[u8], case: MatchCase) -> Option<u8> {
    (first_letter_ok(name, typed, case) && humps_match(name, typed, case)).then_some(2)
}

/// Whether the first letters agree under `case`. Always checked case-insensitively at minimum:
/// a name whose first letter is a different letter entirely is not a candidate under any setting.
fn first_letter_ok(name: &[u8], typed: &[u8], case: MatchCase) -> bool {
    let (Some(n), Some(t)) = (name.first(), typed.first()) else {
        return false;
    };
    match case {
        MatchCase::Ignore => n.eq_ignore_ascii_case(t),
        MatchCase::FirstLetter | MatchCase::All => n == t,
    }
}

/// Whether the typed letters walk `name`'s word boundaries: after the first character, each one
/// either continues the word it is in or jumps to the next hump that matches it.
///
/// A **hump** is an uppercase letter, or the letter after a `_` — which is what makes this work on
/// the `SCREAMING_SNAKE` constants that are half of what a type receiver offers: `MAX_V` reaching
/// `MAX_VALUE` is the same gesture as `mAE` reaching `addAllElements`.
///
/// Greedy, and deliberately so — it is the last tier, reached only when neither prefix test did,
/// and a rare miss on a pathological name costs a suggestion rather than producing a wrong one.
/// ASCII, because a Java identifier that is not is one this will simply not reach.
fn humps_match(name: &[u8], typed: &[u8], case: MatchCase) -> bool {
    let same = |a: u8, b: u8| match case {
        MatchCase::All => a == b,
        _ => a.eq_ignore_ascii_case(&b),
    };
    let mut at = 1usize; // the first letter is settled by `first_letter_ok`
    for &want in &typed[1..] {
        if name.get(at).is_some_and(|&c| same(c, want)) {
            at += 1;
            continue;
        }
        let Some(j) = (at..name.len()).position(|i| is_hump(name, at + i - at) && same(name[i], want))
        else {
            return false;
        };
        at += j + 1;
    }
    true
}

/// Whether `name[i]` starts a word: an uppercase letter, or the character after a `_`.
fn is_hump(name: &[u8], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    name[i].is_ascii_uppercase() || name[i - 1] == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, typed: &str) -> Option<u8> {
        tier(name, typed, MatchCase::FirstLetter)
    }

    #[test]
    fn an_exact_prefix_is_the_best_tier() {
        assert_eq!(t("toLowerCase", "toLo"), Some(0));
    }

    #[test]
    fn a_prefix_in_the_wrong_case_still_matches_below_it() {
        assert_eq!(t("toLowerCase", "toLO"), Some(1));
        assert!(t("toLowerCase", "toLO") > t("toLowerCase", "toLo"));
    }

    /// The two cases that had no answer at all before this existed.
    #[test]
    fn the_humps_are_reachable() {
        assert_eq!(t("toLowerCase", "tolc"), Some(2));
        assert_eq!(t("addAllElements", "aAE"), Some(2));
        assert_eq!(t("addAllowedHeader", "aah"), Some(2));
    }

    /// A constant's words are separated by `_`, and reaching for one is the same gesture.
    #[test]
    fn an_underscore_starts_a_word_too() {
        assert_eq!(t("MAX_VALUE", "MAXV"), Some(2));
        assert_eq!(t("SOME_LONG_NAME", "SLN"), Some(2));
    }

    #[test]
    fn a_name_that_does_not_answer_is_none() {
        assert_eq!(t("toLowerCase", "xyz"), None);
        assert_eq!(t("toLowerCase", "toX"), None);
        // Longer than the name, and no humps to walk.
        assert_eq!(t("size", "sizeOfEverything"), None);
    }

    /// `receiver.` — everything is admissible, and nothing is ranked below anything.
    #[test]
    fn an_empty_prefix_admits_everything_at_the_top_tier() {
        assert_eq!(t("anything", ""), Some(0));
    }

    #[test]
    fn ignore_takes_the_first_letter_in_either_case() {
        assert_eq!(tier("SpringApplication", "spring", MatchCase::Ignore), Some(1));
        assert_eq!(tier("SpringApplication", "spring", MatchCase::FirstLetter), None);
    }

    /// The whole point of the default: a capital means a type in Java, so `Order` must not offer
    /// the local `order` — while `tolc` still reaches `toLowerCase`.
    #[test]
    fn first_letter_separates_a_type_from_a_variable() {
        assert_eq!(t("order", "Order"), None);
        assert_eq!(t("Order", "order"), None);
        assert_eq!(t("toLowerCase", "tolc"), Some(2));
    }

    /// Strict means strict, humps included.
    #[test]
    fn all_requires_every_typed_letter_to_agree() {
        assert_eq!(tier("addAllElements", "aAE", MatchCase::All), Some(2));
        assert_eq!(tier("addAllElements", "aae", MatchCase::All), None);
        assert_eq!(tier("toLowerCase", "toLo", MatchCase::All), Some(0));
        assert_eq!(tier("toLowerCase", "tolo", MatchCase::All), None);
    }

    /// A `Typed` is the same rule asked of one value rather than assembled per call site.
    #[test]
    fn typed_carries_the_rule_with_the_text() {
        let p = Typed::lenient("tolc");
        assert!(p.matches("toLowerCase"));
        assert!(!p.matches("toUpperCase"));
    }
}
