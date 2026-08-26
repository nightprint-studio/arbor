//! The conventions themselves — enumerated, never regexes.
//!
//! ## Why not a regex
//!
//! A regex can say that `get_user_name` is wrong. It cannot say what would be right. The whole
//! point of this pack is that a violation arrives with the fix already computed, and that is only
//! possible if a convention is a **function from words to a spelling** rather than a predicate
//! over characters. So [`Convention::render`] is the definition and [`Convention::accepts`] is
//! `render(name) == name` — one implementation, so the check and its quick-fix can never disagree
//! about what the rule is.
//!
//! It also makes the fix idempotent by construction: applying it twice cannot change the name a
//! second time, because a rendered name is a fixed point of `render`.
//!
//! ## Two things it deliberately preserves
//!
//! **Acronyms.** `serialVersionUID` already satisfies camelCase in every Java codebase that has
//! ever existed; rendering it to `serialVersionUid` would be a rule nobody asked for. An all-caps
//! run of two or more letters is kept verbatim (see [`crate::words::is_acronym`]).
//!
//! **Leading and trailing underscores.** `_unused` in Rust and `_internal` in Java are markers,
//! not spelling. They are stripped, the core is rendered, and they are put back.

use serde::{Deserialize, Serialize};

use crate::words::{is_acronym, tokenize_identifier};

/// How a name is spelled. `Any` is the off switch, and is the default for every target — a
/// project adopts a convention deliberately, it is never assumed to want one (see
/// [`crate::config`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Convention {
    /// No rule. Nothing is ever flagged.
    #[default]
    #[serde(rename = "any")]
    Any,
    /// `getUserName`
    #[serde(rename = "camelCase")]
    Camel,
    /// `GetUserName`
    #[serde(rename = "PascalCase")]
    Pascal,
    /// `GET_USER_NAME`
    #[serde(rename = "UPPER_SNAKE_CASE")]
    UpperSnake,
    /// `get_user_name`
    #[serde(rename = "snake_case")]
    LowerSnake,
    /// `getusername` — for package / module segments, where words do not separate.
    #[serde(rename = "lowercase")]
    Lower,
}

impl Convention {
    /// Every convention, in the order a settings UI should list them. `Any` first: it is the
    /// default, and the one a user picks to switch a target back off.
    pub const ALL: [Convention; 6] = [
        Convention::Any,
        Convention::Camel,
        Convention::Pascal,
        Convention::UpperSnake,
        Convention::LowerSnake,
        Convention::Lower,
    ];

    /// The wire / TOML spelling, which is also the example: the value `"camelCase"` *is* what
    /// camelCase looks like, so a config file explains itself and a dropdown needs no second
    /// column of examples.
    pub const fn as_str(self) -> &'static str {
        match self {
            Convention::Any => "any",
            Convention::Camel => "camelCase",
            Convention::Pascal => "PascalCase",
            Convention::UpperSnake => "UPPER_SNAKE_CASE",
            Convention::LowerSnake => "snake_case",
            Convention::Lower => "lowercase",
        }
    }

    /// Whether this convention checks anything at all.
    pub const fn is_off(self) -> bool {
        matches!(self, Convention::Any)
    }

    /// Whether `name` already satisfies this convention.
    ///
    /// Defined as "rendering it changes nothing", so there is exactly one description of the rule
    /// in the crate and a fix can never produce a name the check would flag again.
    pub fn accepts(self, name: &str) -> bool {
        match self.render(name) {
            Some(fixed) => fixed == name,
            None => true,
        }
    }

    /// The spelling of `name` under this convention, or `None` when the question does not apply:
    /// the convention is `Any`, the identifier carries a `$` (synthetic / inner-class machinery,
    /// never authored), or it has no words at all (`_`, `__`).
    pub fn render(self, name: &str) -> Option<String> {
        if self.is_off() || name.contains('$') {
            return None;
        }
        // Markers, not spelling: `_unused` keeps its underscore and `unused` is what gets rendered.
        let core = name.trim_matches('_');
        if core.is_empty() {
            return None;
        }
        let leading = &name[..name.len() - name.trim_start_matches('_').len()];
        let trailing = &name[name.trim_end_matches('_').len()..];

        let words: Vec<String> = tokenize_identifier(core).into_iter().map(|w| w.text).collect();
        if words.is_empty() {
            return None;
        }
        let body = match self {
            Convention::Any => return None,
            Convention::Camel => join_concat(&words, true),
            Convention::Pascal => join_concat(&words, false),
            Convention::UpperSnake => join_snake(&words, str::to_uppercase),
            Convention::LowerSnake => join_snake(&words, str::to_lowercase),
            Convention::Lower => words.concat().to_lowercase(),
        };
        Some(format!("{leading}{body}{trailing}"))
    }
}

impl std::fmt::Display for Convention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `camelCase` / `PascalCase`: words concatenated, each capitalised except (for camel) the first,
/// which is lowercased whole — `XMLParser` as a method is `xmlParser`, not `xMLParser`.
fn join_concat(words: &[String], lower_first: bool) -> String {
    let mut out = String::new();
    for (i, word) in words.iter().enumerate() {
        if i == 0 && lower_first {
            out.push_str(&word.to_lowercase());
        } else {
            out.push_str(&capitalize(word));
        }
    }
    out
}

/// `snake_case` / `UPPER_SNAKE_CASE`: words joined by `_`, cased by `f`.
///
/// A digit run is glued to the word before it instead of becoming a segment of its own —
/// `utf8Decode` is `utf8_decode`, never `utf_8_decode`, which is what splitting on digit
/// boundaries would otherwise produce.
fn join_snake(words: &[String], f: fn(&str) -> String) -> String {
    let mut out = String::new();
    for word in words {
        let cased = f(word);
        if out.is_empty() || word.chars().all(char::is_numeric) {
            out.push_str(&cased);
        } else {
            out.push('_');
            out.push_str(&cased);
        }
    }
    out
}

/// Title-case one word, leaving an acronym (`XML`) and a digit run (`8`) as they are.
fn capitalize(word: &str) -> String {
    if is_acronym(word) {
        return word.to_string();
    }
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::Convention::*;
    use super::*;

    #[test]
    fn renders_the_spellings_of_the_same_words() {
        assert_eq!(Camel.render("get_user_name").unwrap(), "getUserName");
        assert_eq!(Pascal.render("get_user_name").unwrap(), "GetUserName");
        assert_eq!(UpperSnake.render("getUserName").unwrap(), "GET_USER_NAME");
        assert_eq!(LowerSnake.render("getUserName").unwrap(), "get_user_name");
        assert_eq!(Lower.render("com_Acme").unwrap(), "comacme");
    }

    #[test]
    fn accepts_is_render_being_a_no_op() {
        assert!(Camel.accepts("getUserName"));
        assert!(!Camel.accepts("get_user_name"));
        assert!(!Camel.accepts("GetUserName"));
        assert!(Pascal.accepts("OrderService"));
        assert!(UpperSnake.accepts("MAX_VALUE"));
        assert!(LowerSnake.accepts("read_to_string"));
    }

    #[test]
    fn the_fix_is_a_fixed_point() {
        // The property the whole design rests on: applying a fix can never produce something the
        // check flags again, for any convention and any input.
        for convention in Convention::ALL {
            for name in
                ["get_user_name", "GetUserName", "GET_USER_NAME", "parseXMLFile", "_unused", "x"]
            {
                if let Some(fixed) = convention.render(name) {
                    assert!(
                        convention.accepts(&fixed),
                        "{convention} rendered {name} to {fixed}, which it then rejects"
                    );
                }
            }
        }
    }

    #[test]
    fn acronyms_survive() {
        assert!(Camel.accepts("serialVersionUID"));
        assert!(Camel.accepts("parseXMLFile"));
        assert_eq!(Camel.render("XMLParser").unwrap(), "xmlParser");
        assert_eq!(Pascal.render("xml_parser").unwrap(), "XmlParser");
    }

    #[test]
    fn underscore_markers_are_preserved() {
        assert_eq!(LowerSnake.render("_unusedThing").unwrap(), "_unused_thing");
        assert!(LowerSnake.accepts("_unused_thing"));
        // Nothing but underscores has no words to render, so nothing is ever flagged.
        assert!(Camel.render("__").is_none());
        assert!(Camel.accepts("__"));
    }

    #[test]
    fn digits_glue_to_the_word_before_them() {
        assert_eq!(LowerSnake.render("utf8Decode").unwrap(), "utf8_decode");
        assert_eq!(Camel.render("utf8_decode").unwrap(), "utf8Decode");
    }

    #[test]
    fn synthetic_names_are_never_touched() {
        assert!(Camel.render("Outer$Inner").is_none());
        assert!(Camel.accepts("Outer$Inner"));
    }

    #[test]
    fn any_never_flags_and_never_fixes() {
        assert!(Any.accepts("this_is_whatever_You_Want"));
        assert!(Any.render("this_is_whatever").is_none());
    }
}
