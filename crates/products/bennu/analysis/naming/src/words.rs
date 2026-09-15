//! Splitting an identifier into the words it is made of — the operation every convention is
//! defined in terms of.
//!
//! `getUserName`, `get_user_name` and `GET_USER_NAME` are the same three words under three
//! spellings, so a convention is not a predicate over characters but a *rendering* of a word
//! list. Everything in [`crate::convention`] is built on this, which is why it lives at the
//! bottom of the crate and depends on nothing.
//!
//! It moved here from `bennu_intel::spell`, which needed exactly the same split to spell-check
//! declaration names one word at a time. Two copies of this would have drifted on the first
//! acronym anybody disagreed about; `spell` now re-exports these.

/// A sub-word of an identifier: its text plus its byte span *relative to the identifier start*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubWord {
    /// The sub-word text.
    pub text: String,
    /// Start byte offset within the identifier.
    pub start: usize,
    /// End byte offset (exclusive) within the identifier.
    pub end: usize,
}

/// Split an identifier into sub-words on camelCase boundaries, `_`, `-`, and digit runs.
///
/// Rules:
/// - Separators (`_`, `-`, and any non-alphanumeric) split and are dropped.
/// - A lower→Upper transition starts a new word (`camelCase` → `camel`, `Case`).
/// - An acronym run keeps the trailing capital that starts the next word
///   (`HTTPServer` → `HTTP`, `Server`; `parseXMLFile` → `parse`, `XML`, `File`).
/// - Digit runs are their own token.
///
/// Offsets are byte offsets into `ident` (ASCII-only identifiers keep them equal to char
/// offsets; non-ASCII letters are handled by iterating `char_indices`).
pub fn tokenize_identifier(ident: &str) -> Vec<SubWord> {
    let mut out = Vec::new();
    let mut word_start: Option<usize> = None;
    let mut prev: Option<char> = None;
    let chars: Vec<(usize, char)> = ident.char_indices().collect();

    for (i, &(off, c)) in chars.iter().enumerate() {
        let class = CharClass::of(c);
        if class == CharClass::Sep {
            if let Some(ws) = word_start.take() {
                out.push(sub_word(ident, ws, off));
            }
            prev = None;
            continue;
        }
        // Decide whether a boundary starts here (a new word begins at `off`).
        let next_lower =
            matches!(chars.get(i + 1), Some(&(_, n)) if CharClass::of(n) == CharClass::Lower);
        let boundary = match (prev.map(CharClass::of), class) {
            // Start of a word.
            (None, _) => true,
            // Upper → lower is a *continuation* (`Ha` in `Hash`), never a boundary.
            (Some(CharClass::Upper), CharClass::Lower) => false,
            // Any other class change starts a new word (camelCase `xU`, digit runs, etc.).
            (Some(prev_class), _) if prev_class != class => true,
            // Upper → Upper: a boundary only at the LAST capital of an acronym run whose
            // next char is lowercase (the `S` in `HTTPServer` → `HTTP`, `Server`).
            (Some(CharClass::Upper), CharClass::Upper) => next_lower,
            _ => false,
        };
        if boundary {
            if let Some(ws) = word_start.take() {
                out.push(sub_word(ident, ws, off));
            }
            word_start = Some(off);
        }
        prev = Some(c);
    }
    if let Some(ws) = word_start.take() {
        out.push(sub_word(ident, ws, ident.len()));
    }
    out
}

fn sub_word(ident: &str, start: usize, end: usize) -> SubWord {
    SubWord { text: ident[start..end].to_string(), start, end }
}

/// Character class for the tokenizer's boundary logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Upper,
    Lower,
    Digit,
    Sep,
}

impl CharClass {
    fn of(c: char) -> CharClass {
        if c.is_uppercase() {
            CharClass::Upper
        } else if c.is_lowercase() {
            CharClass::Lower
        } else if c.is_numeric() {
            CharClass::Digit
        } else {
            CharClass::Sep
        }
    }
}

/// Whether a word is an acronym worth preserving as it stands (`XML`, `UID`, `HTTP`).
///
/// A convention renders `Xml` from `XML` only if it is willing to rewrite `serialVersionUID`
/// into `serialVersionUid`, which no Java project wants — so an all-caps run of two or more
/// letters is kept verbatim wherever the convention capitalises.
pub(crate) fn is_acronym(word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    letters >= 2 && word.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(ident: &str) -> Vec<String> {
        tokenize_identifier(ident).into_iter().map(|w| w.text).collect()
    }

    #[test]
    fn splits_camel_snake_and_acronyms() {
        assert_eq!(words("getUserName"), ["get", "User", "Name"]);
        assert_eq!(words("get_user_name"), ["get", "user", "name"]);
        assert_eq!(words("GET_USER_NAME"), ["GET", "USER", "NAME"]);
        assert_eq!(words("parseXMLFile"), ["parse", "XML", "File"]);
        assert_eq!(words("HTTPServer"), ["HTTP", "Server"]);
    }

    #[test]
    fn digit_runs_are_their_own_word() {
        assert_eq!(words("utf8Decode"), ["utf", "8", "Decode"]);
    }

    #[test]
    fn offsets_index_back_into_the_identifier() {
        let ident = "get_user";
        let split = tokenize_identifier(ident);
        assert_eq!(&ident[split[1].start..split[1].end], "user");
    }

    #[test]
    fn acronyms_are_recognised_only_when_multi_letter() {
        assert!(is_acronym("XML"));
        assert!(is_acronym("UID"));
        // A single capital is a Title-cased word, not an acronym — otherwise `getA` would be
        // untouchable by every convention.
        assert!(!is_acronym("A"));
        assert!(!is_acronym("Xml"));
    }
}
