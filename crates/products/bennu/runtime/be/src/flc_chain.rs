//! The members of a chain, checked while the model writes it.
//!
//! The first dot after the caret is constrained from the names the index offers there. The model goes on: `a.b().c` has
//! a second dot whose receiver is the *result* of `b()`, and nothing in the prompt says what that offers. At each later
//! dot the member name that follows is restricted to what the receiver typed so far really has — the same question the
//! editor answers when `.` is typed — so a name that does not exist is never written, instead of cut afterwards.

use std::cell::RefCell;
use std::collections::HashMap;

use bennu_flc::prelude::{allowed_after, ByteMask};

/// Dots that are asked about: each one is an index query, and the later ones are less and less the model's own idea.
pub(crate) const MAX_DOTS: usize = 4;

/// `Object`'s methods: a receiver that offers only these is one the index could not type.
const OBJECT: [&str; 9] = ["equals", "hashCode", "toString", "getClass", "notify", "notifyAll", "wait", "clone", "finalize"];

fn ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// A member access in progress at the end of `gen` (bytes the model wrote after the caret): the index of its dot, the
/// number of member dots before and including it, and the identifier typed after it so far.
pub(crate) fn open_member(gen: &[u8]) -> Option<(usize, usize, &[u8])> {
    let (mut in_str, mut quote, mut dots, mut last) = (false, 0u8, 0usize, None);
    let mut i = 0;
    while i < gen.len() {
        let b = gen[i];
        if in_str {
            if b == b'\\' {
                i += 1;
            } else if b == quote {
                in_str = false;
            }
        } else if b == b'"' || b == b'\'' {
            in_str = true;
            quote = b;
        } else if b == b'.' && is_member_dot(gen, i) {
            dots += 1;
            last = Some((i, dots));
        }
        i += 1;
    }
    let (dot, n) = last?;
    let rest = &gen[dot + 1..];
    // the access is still open only while what follows the dot is the identifier being typed
    (!in_str && rest.iter().all(|&b| ident(b))).then_some((dot, n, rest))
}

/// A dot between a receiver and a member: not a decimal point, not a spread (`...`).
fn is_member_dot(gen: &[u8], dot: usize) -> bool {
    let Some(&prev) = dot.checked_sub(1).and_then(|p| gen.get(p)) else { return false };
    if prev == b'.' || gen.get(dot + 1) == Some(&b'.') {
        return false;
    }
    if prev.is_ascii_digit() {
        // `1.5`: the run before the dot is all digits
        let run = gen[..dot].iter().rev().take_while(|&&b| ident(b)).all(|b| b.is_ascii_digit());
        return !run;
    }
    ident(prev) || matches!(prev, b')' | b']') || prev.is_ascii_whitespace()
}

/// Whether `names` say anything about the receiver: more than `Object`'s own methods.
pub(crate) fn informative(names: &[String]) -> bool {
    names.iter().any(|n| !OBJECT.contains(&n.as_str()))
}

/// The constraint over what the model writes after the caret. `base` is the buffer up to where the model's answer
/// starts, `names_at(virtual_source)` the member names the index offers at the end of that text, and `first` the
/// constraint of the first dot (the one at the caret), which this wraps.
pub(crate) fn chain_constraint<'a>(
    base: &'a str,
    names_at: impl Fn(&str) -> Vec<String> + 'a,
    first: Option<&'a dyn Fn(&[u8]) -> Option<ByteMask>>,
) -> impl Fn(&[u8]) -> Option<ByteMask> + 'a {
    let memo: RefCell<HashMap<usize, Vec<String>>> = RefCell::new(HashMap::new());
    move |gen: &[u8]| {
        let Some((dot, n, typed)) = open_member(gen) else { return first.and_then(|c| c(gen)) };
        if n > MAX_DOTS {
            return None;
        }
        let mut memo = memo.borrow_mut();
        let names = memo.entry(dot).or_insert_with(|| {
            let virt = format!("{base}{}", String::from_utf8_lossy(&gen[..=dot]));
            names_at(&virt)
        });
        if !informative(names) {
            return None;
        }
        allowed_after(names, &String::from_utf8_lossy(typed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn the_open_member_is_the_last_dot_with_only_a_name_after_it() {
        assert_eq!(open_member(b"stream().co"), Some((8, 1, &b"co"[..])));
        assert_eq!(open_member(b"a.b().c."), Some((7, 3, &b""[..])));
        assert_eq!(open_member(b"a.b(x)"), None, "the name is closed by a paren");
        assert_eq!(open_member(b"name"), None, "no dot, the first dot is the caller's");
    }

    #[test]
    fn dots_in_strings_numbers_and_spreads_are_not_member_dots() {
        assert_eq!(open_member(b"f(\"a.b\").g"), Some((8, 1, &b"g"[..])));
        assert_eq!(open_member(b"x = 1.5"), None);
        assert_eq!(open_member(b"f(String... a"), None);
        assert_eq!(open_member(b"get(\"a.").map(|_| ()), None, "inside a string literal");
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_later_dot_restricts_the_member_to_what_the_receiver_has() {
        let c = chain_constraint("items", |_| names(&["size", "stream"]), None);
        let m = c(b"stream().").expect("restricted");
        assert!(m[b's' as usize] && !m[b'x' as usize]);
        let m = c(b"stream().si").expect("restricted");
        assert!(m[b'z' as usize] && !m[b't' as usize]);
    }

    #[test]
    fn each_dot_costs_one_query() {
        let calls = Cell::new(0);
        let c = chain_constraint(
            "x",
            |virt| {
                calls.set(calls.get() + 1);
                assert!(virt.ends_with('.'), "{virt}");
                names(&["alpha", "beta"])
            },
            None,
        );
        for gen in [&b"a"[..], b"al", b"alp", b"alph"] {
            c(gen); // no dot written yet: the first dot was the caller's
        }
        c(b"alpha().");
        c(b"alpha().b");
        c(b"alpha().be");
        assert_eq!(calls.get(), 1, "the dot is asked about once however many letters follow");
        c(b"alpha().beta().");
        assert_eq!(calls.get(), 2, "and the next dot is a new question");
    }

    #[test]
    fn a_receiver_the_index_cannot_type_has_no_opinion() {
        let c = chain_constraint("x", |_| names(&["toString", "hashCode"]), None);
        assert!(c(b"foo().").is_none());
        let c = chain_constraint("x", |_| Vec::new(), None);
        assert!(c(b"foo().").is_none());
    }

    #[test]
    fn past_the_cap_there_is_no_opinion_and_no_query() {
        let calls = Cell::new(0);
        let c = chain_constraint("x", |_| { calls.set(calls.get() + 1); names(&["a"]) }, None);
        assert!(c(b"a().a().a().a().a().a").is_none());
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn without_a_later_dot_the_first_constraint_decides() {
        let first = |gen: &[u8]| -> Option<ByteMask> {
            let mut m = [false; 256];
            m[gen.len() as usize + b'a' as usize] = true;
            Some(m)
        };
        let c = chain_constraint("x", |_| Vec::new(), Some(&first));
        assert!(c(b"ab").unwrap()[b'c' as usize]);
    }
}
