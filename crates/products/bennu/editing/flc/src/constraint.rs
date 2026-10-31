//! Which bytes may come next. The model writes tokens; the host knows names. A token is allowed when every byte of
//! it continues a name the host admits, so the check is on bytes and the model never has to know what a name is.

/// For every byte value, whether it may come next.
pub type ByteMask = [bool; 256];

/// Restricts the next byte given what has been generated so far; `None` = no opinion (the constrained stretch is
/// over, or there is nothing to say).
pub type Constraint<'a> = &'a dyn Fn(&[u8]) -> Option<ByteMask>;

fn ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// The bytes that may follow `partial` among `names`: the next byte of every name that starts with it, and — when a
/// name is complete — anything that cannot be part of an identifier. `None` when no name starts with `partial`:
/// the user is typing something the index does not know, and the constraint stays out of it.
pub fn allowed_after<S: AsRef<str>>(names: &[S], partial: &str) -> Option<ByteMask> {
    let mut mask = [false; 256];
    let mut any = false;
    for n in names {
        if let Some(rest) = n.as_ref().strip_prefix(partial) {
            any = true;
            match rest.bytes().next() {
                Some(c) => mask[c as usize] = true,
                None => {
                    for (c, slot) in mask.iter_mut().enumerate() {
                        if !ident_byte(c as u8) {
                            *slot = true;
                        }
                    }
                }
            }
        }
    }
    any.then_some(mask)
}

/// The constraint for a caret that sits after `receiver.` with `typed` already written: what the model generates
/// continues one of `names`, and once it leaves the identifier (a `(`, a `.`, a space) the constraint ends.
pub fn names_constraint<'a, S: AsRef<str> + 'a>(names: &'a [S], typed: &'a str) -> impl Fn(&[u8]) -> Option<ByteMask> + 'a {
    move |gen: &[u8]| {
        if gen.iter().any(|&b| !ident_byte(b)) {
            return None;
        }
        let partial = format!("{typed}{}", String::from_utf8_lossy(gen));
        allowed_after(names, &partial)
    }
}

/// Token healing: the word being typed is taken out of the prompt (a half token is something the model never read
/// in training, and it answers it badly) and forced back as the first bytes of what it writes, after which `then`
/// — the constraint that would have applied anyway — takes over, seeing those bytes too.
pub fn forced_prefix<'a>(forced: &'a str, then: Option<Constraint<'a>>) -> impl Fn(&[u8]) -> Option<ByteMask> + 'a {
    move |gen: &[u8]| {
        if gen.len() < forced.len() {
            if !forced.as_bytes().starts_with(gen) {
                return None;
            }
            let mut mask = [false; 256];
            mask[forced.as_bytes()[gen.len()] as usize] = true;
            return Some(mask);
        }
        then.and_then(|c| c(gen))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_byte_of_every_name_that_continues_the_prefix() {
        let m = allowed_after(&["getName", "getAge", "size"], "ge").unwrap();
        assert!(m[b't' as usize] && !m[b's' as usize]);
        assert!(allowed_after(&["size"], "zz").is_none());
    }

    #[test]
    fn a_complete_name_lets_any_non_identifier_byte_follow() {
        let m = allowed_after(&["size"], "size").unwrap();
        assert!(m[b'(' as usize] && m[b' ' as usize] && !m[b'x' as usize]);
    }

    #[test]
    fn the_typed_word_is_forced_first_then_the_other_constraint_continues() {
        let names = ["getName", "getAge"];
        let inner = names_constraint(&names, "");
        let c = forced_prefix("ge", Some(&inner));
        assert!(c(b"").unwrap()[b'g' as usize] && !c(b"").unwrap()[b'x' as usize]);
        assert!(c(b"g").unwrap()[b'e' as usize]);
        assert!(c(b"ge").unwrap()[b't' as usize] && !c(b"ge").unwrap()[b'x' as usize]);
        assert!(c(b"zz").is_none());
        assert!(forced_prefix("", None)(b"").is_none());
    }

    #[test]
    fn the_constraint_ends_when_the_identifier_does() {
        let names = ["getName"];
        let c = names_constraint(&names, "get");
        assert!(c(b"N").is_some());
        assert!(c(b"Name(").is_none());
    }
}
