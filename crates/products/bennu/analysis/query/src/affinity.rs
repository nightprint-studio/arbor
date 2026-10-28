//! How much a candidate's NAME echoes the name of the thing being filled.
//!
//! In `Customer customer = |` the candidates `customer`, `getCustomer()` and `findCustomer(..)` all
//! say the same word as the hole, and nothing about their types needs to. It is the second-strongest
//! thing a person uses to pick from a list — after "can it be written here" — and it is the one
//! signal the ranking had no word for: an argument `findByCustomerId(|)` wants `customerId` over
//! `orderId` for no reason a type could ever give.
//!
//! ## What it reads, and what it ignores
//!
//! Names are cut into words (`customerId` → `customer`, `id`; `MAX_VALUE` → `max`, `value`), a
//! leading *verb* is dropped (`getCustomerId` is the thing `customerId`, read), the words that are
//! grammar rather than meaning (`by`, `of`, `for`) are dropped, and a plural is read as its singular
//! (`orders` and `order` are one word). What is compared is the words that remain — never the
//! spelling — so `customer_id` and `customerId` agree.
//!
//! The answer is a small number of points, graded by how much of the name is shared. It is a
//! tie-breaker with a voice, not a verdict: the caller scales it by the user's strength setting.

/// Points for a candidate that says exactly the words of the hole.
pub const EXACT: i32 = 30;
/// Points when one name is the tail of the other — `customerId` for a hole called `id`.
pub const TAIL: i32 = 18;
/// Points when one name begins with the other — `orderList` for a hole called `order`.
pub const HEAD: i32 = 14;
/// Points for sharing any word at all.
pub const SHARED: i32 = 8;

/// Verbs a name starts with that say HOW a value is reached, not WHAT it is.
const LEADING_VERBS: &[&str] = &[
    "get", "is", "has", "can", "find", "fetch", "load", "read", "to", "as", "new", "create", "build",
    "make", "with", "set", "of", "from",
];

/// Words that connect rather than mean.
const FILLER: &[&str] = &["by", "of", "for", "the", "and", "in", "with"];

/// How much `candidate` echoes `hole`, `0` when they share nothing. See the module docs.
pub fn name_affinity(candidate: &str, hole: &str) -> i32 {
    let c = significant(candidate);
    let h = significant(hole);
    if c.is_empty() || h.is_empty() {
        return 0;
    }
    if c == h {
        EXACT
    } else if c.ends_with(&h) || h.ends_with(&c) {
        TAIL
    } else if c.starts_with(&h) || h.starts_with(&c) {
        HEAD
    } else if c.iter().any(|w| h.contains(w)) {
        SHARED
    } else {
        0
    }
}

/// The meaningful, singular words of a name.
fn significant(name: &str) -> Vec<String> {
    let mut words = split_words(name);
    // Only LEADING verbs, and never the last word: `get` alone is a name, not a verb with nothing
    // after it.
    while words.len() > 1 && LEADING_VERBS.contains(&words[0].as_str()) {
        words.remove(0);
    }
    words
        .into_iter()
        .filter(|w| !FILLER.contains(&w.as_str()))
        .map(|w| singular(&w))
        .collect()
}

/// `customerId` → `[customer, id]`, `MAX_VALUE` → `[max, value]`, `HTTPServer` → `[http, server]`.
fn split_words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            push_word(&mut words, &mut current);
            continue;
        }
        let previous = i.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(i + 1).copied();
        let starts_word = c.is_uppercase()
            && previous.is_some_and(|p| {
                p.is_lowercase()
                    || p.is_ascii_digit()
                    // `HTTPServer`: the `S` ends an acronym when a lowercase letter follows it.
                    || (p.is_uppercase() && next.is_some_and(|n| n.is_lowercase()))
            });
        if starts_word {
            push_word(&mut words, &mut current);
        }
        current.extend(c.to_lowercase());
    }
    push_word(&mut words, &mut current);
    words
}

fn push_word(words: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        words.push(std::mem::take(current));
    }
}

/// A deliberately small singulariser: enough that `orders` meets `order`, and no more. A wrong
/// stem only costs a few points of a tie-breaker, so there is no table of irregulars.
fn singular(word: &str) -> String {
    if word.len() > 4 && word.ends_with("ies") {
        return format!("{}y", &word[..word.len() - 3]);
    }
    if word.len() > 3 && word.ends_with('s') && !word.ends_with("ss") {
        return word[..word.len() - 1].to_string();
    }
    word.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_split_on_case_underscores_and_acronyms() {
        assert_eq!(split_words("customerId"), ["customer", "id"]);
        assert_eq!(split_words("MAX_VALUE"), ["max", "value"]);
        assert_eq!(split_words("HTTPServer"), ["http", "server"]);
        assert_eq!(split_words("order2Items"), ["order2", "items"]);
    }

    #[test]
    fn the_same_words_are_an_exact_echo_whatever_the_spelling() {
        assert_eq!(name_affinity("customerId", "customer_id"), EXACT);
        assert_eq!(name_affinity("customer", "customer"), EXACT);
    }

    /// `Customer customer = getCustomer()` — the verb says how it is reached, not what it is.
    #[test]
    fn a_leading_verb_is_not_part_of_the_meaning() {
        assert_eq!(name_affinity("getCustomer", "customer"), EXACT);
        assert_eq!(name_affinity("findCustomerById", "customer"), HEAD);
        assert_eq!(name_affinity("isActive", "active"), EXACT);
    }

    #[test]
    fn a_plural_is_read_as_its_singular() {
        assert_eq!(name_affinity("orders", "order"), EXACT);
        assert_eq!(name_affinity("categories", "category"), EXACT);
        assert_eq!(name_affinity("class", "clas"), 0, "`ss` is not a plural");
    }

    #[test]
    fn a_shared_tail_or_head_scores_less_than_the_whole_name() {
        assert_eq!(name_affinity("customerId", "id"), TAIL);
        assert_eq!(name_affinity("orderList", "order"), HEAD);
        assert!(EXACT > TAIL && TAIL > HEAD && HEAD > SHARED && SHARED > 0);
    }

    #[test]
    fn any_shared_word_earns_the_smallest_credit() {
        assert_eq!(name_affinity("shippingAddress", "billingAddress"), SHARED);
        assert_eq!(name_affinity("addressLine", "mailingAddressBook"), SHARED);
    }

    #[test]
    fn unrelated_names_earn_nothing() {
        assert_eq!(name_affinity("total", "customer"), 0);
        assert_eq!(name_affinity("", "customer"), 0);
        assert_eq!(name_affinity("customer", ""), 0);
    }

    /// `get` on its own is a name, not a verb with nothing after it.
    #[test]
    fn a_bare_verb_is_kept_as_a_name() {
        assert_eq!(name_affinity("get", "get"), EXACT);
        assert_eq!(name_affinity("get", "customer"), 0);
    }

    /// Filler words connect rather than mean: `findByCustomerId` and `customerId` agree.
    #[test]
    fn filler_words_are_ignored() {
        assert_eq!(name_affinity("findByCustomerId", "customerId"), EXACT);
        assert_eq!(name_affinity("listOfOrders", "orders"), TAIL);
    }
}
