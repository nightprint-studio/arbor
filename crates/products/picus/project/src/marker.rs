//! [`MarkerTemplate`] — the comment Picus writes above a block it generated, and
//! the means of recognising it again.
//!
//! Why a marker exists at all: it is what makes an apply **idempotent**. Without
//! it, regenerating a change appends a second copy of the block and the user has
//! to find and delete the first one by hand. With it, Picus can locate what it
//! wrote and replace it in place.
//!
//! Why it is a *template*: projects disagree about what belongs on that line.
//! Several want the version transition spelled out on every block. So the shape is
//! configuration, in the same `-- picus:` namespace the suppression comments use,
//! and a project that wants its files free of tool markers empties the template —
//! losing exactly the idempotence described above, which the settings UI says out
//! loud.
//!
//! ## Template syntax
//!
//! * `{name}` — a placeholder. Known names: `table`, `operation`, `from_version`,
//!   `to_version`, `hash`.
//! * `[ … ]` — an optional segment, kept only when **every** placeholder inside it
//!   has a value. This is what lets one template serve both an update script and
//!   an initialisation script: `[({from_version} -> {to_version})]` simply
//!   disappears where there is no version guard, instead of leaving `( -> )`
//!   behind. Brackets do not nest and cannot be escaped.
//!
//! ## What is deliberately absent: a timestamp
//!
//! There is no `{date}` placeholder and there will not be one. Generation in Picus
//! is deterministic — same input, byte-identical output — because that is what
//! makes a generated block reviewable in a diff. A timestamp would make every
//! regeneration a change, and the marker would stop being evidence of anything.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// The default: enough to find the block again, quiet enough to live in someone
/// else's repository. ASCII only — these files are frequently windows-1252, and a
/// marker is a poor place to discover an unrepresentable character.
pub const DEFAULT_MARKER: &str = "-- picus: generated {table}[ ({from_version} -> {to_version})]";

/// Every placeholder the renderer knows.
pub const KNOWN_PLACEHOLDERS: [&str; 5] =
    ["table", "operation", "from_version", "to_version", "hash"];

/// The comment written above a generated block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarkerTemplate(pub String);

impl Default for MarkerTemplate {
    fn default() -> Self {
        MarkerTemplate(DEFAULT_MARKER.to_string())
    }
}

/// What a marker can say about the block beneath it.
#[derive(Debug, Clone, Default)]
pub struct MarkerFields<'a> {
    pub table: Option<&'a str>,
    pub operation: Option<&'a str>,
    pub from_version: Option<&'a str>,
    pub to_version: Option<&'a str>,
    /// A digest of the generated statements, for a project that wants to detect a
    /// hand-edited block. Opt-in: absent from the default template.
    pub hash: Option<&'a str>,
}

impl MarkerFields<'_> {
    fn get(&self, name: &str) -> Option<&str> {
        match name {
            "table" => self.table,
            "operation" => self.operation,
            "from_version" => self.from_version,
            "to_version" => self.to_version,
            "hash" => self.hash,
            _ => None,
        }
        .filter(|v| !v.is_empty())
    }
}

/// One piece of a parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Literal(String),
    Placeholder(String),
    /// Kept only when every placeholder inside resolves.
    Optional(Vec<Piece>),
}

impl MarkerTemplate {
    /// An empty (or whitespace-only) template means "do not mark blocks".
    pub fn is_disabled(&self) -> bool {
        self.0.trim().is_empty()
    }

    /// Render the marker line, or `None` when marking is switched off.
    pub fn render(&self, fields: &MarkerFields<'_>) -> Option<String> {
        if self.is_disabled() {
            return None;
        }
        let mut out = String::new();
        render_pieces(&parse(&self.0), fields, &mut out);
        let trimmed = out.trim_end().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    /// Does this line look like a marker produced by this template?
    ///
    /// Used to find a previously generated block. It matches the *shape*, not the
    /// values, so a block generated before the table was renamed is still found.
    pub fn recognises(&self, line: &str) -> bool {
        match self.matcher() {
            Some(re) => re.is_match(line.trim()),
            None => false,
        }
    }

    /// Placeholder names in the template that the renderer does not know — so the
    /// settings UI can say "`{autore}` will always be empty" instead of letting the
    /// user discover it in a committed file.
    pub fn unknown_placeholders(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect_unknown(&parse(&self.0), &mut out);
        out
    }

    /// The regex that recognises this template's output.
    fn matcher(&self) -> Option<Regex> {
        if self.is_disabled() {
            return None;
        }
        let mut pattern = String::from(r"^\s*");
        build_matcher(&parse(&self.0), &mut pattern);
        pattern.push_str(r"\s*$");
        Regex::new(&pattern).ok()
    }
}

/// Split a template into literals, placeholders and optional segments.
///
/// A hand-written scan rather than a regex: the grammar is three tokens and the
/// unterminated cases (`[` never closed, `{` never closed) have to degrade to
/// literal text rather than fail, because this string comes from a user's config
/// file and a typo in it must not take the project down with it.
fn parse(template: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut literal = String::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let mut name = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    name.push(c);
                }
                if closed {
                    flush(&mut literal, &mut pieces);
                    pieces.push(Piece::Placeholder(name));
                } else {
                    literal.push('{');
                    literal.push_str(&name);
                }
            }
            '[' => {
                let mut inner = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == ']' {
                        closed = true;
                        break;
                    }
                    inner.push(c);
                }
                if closed {
                    flush(&mut literal, &mut pieces);
                    pieces.push(Piece::Optional(parse(&inner)));
                } else {
                    // An unterminated `[` is a typo, not an instruction to stop
                    // reading: the bracket becomes literal text and everything
                    // after it is parsed as usual, so the placeholders the user
                    // did write still work.
                    flush(&mut literal, &mut pieces);
                    pieces.push(Piece::Literal("[".to_string()));
                    pieces.extend(parse(&inner));
                }
            }
            _ => literal.push(c),
        }
    }
    flush(&mut literal, &mut pieces);
    pieces
}

fn flush(literal: &mut String, pieces: &mut Vec<Piece>) {
    if !literal.is_empty() {
        pieces.push(Piece::Literal(std::mem::take(literal)));
    }
}

fn render_pieces(pieces: &[Piece], fields: &MarkerFields<'_>, out: &mut String) {
    for piece in pieces {
        match piece {
            Piece::Literal(text) => out.push_str(text),
            Piece::Placeholder(name) => out.push_str(fields.get(name).unwrap_or("")),
            Piece::Optional(inner) => {
                if all_resolved(inner, fields) {
                    render_pieces(inner, fields, out);
                }
            }
        }
    }
}

/// An optional segment survives only if every placeholder in it has a value —
/// including none at all, which makes `[literal text]` a way to write a segment
/// that is always kept, and therefore harmless.
fn all_resolved(pieces: &[Piece], fields: &MarkerFields<'_>) -> bool {
    pieces.iter().all(|piece| match piece {
        Piece::Placeholder(name) => fields.get(name).is_some(),
        Piece::Optional(inner) => all_resolved(inner, fields),
        Piece::Literal(_) => true,
    })
}

fn build_matcher(pieces: &[Piece], out: &mut String) {
    for piece in pieces {
        match piece {
            Piece::Literal(text) => out.push_str(&regex::escape(text)),
            // Non-greedy, and never empty: an empty capture would let the marker
            // match a bare `-- picus:` line that is something else entirely.
            Piece::Placeholder(_) => out.push_str(r".+?"),
            Piece::Optional(inner) => {
                out.push_str("(?:");
                build_matcher(inner, out);
                out.push_str(")?");
            }
        }
    }
}

fn collect_unknown(pieces: &[Piece], out: &mut Vec<String>) {
    for piece in pieces {
        match piece {
            Piece::Placeholder(name) if !KNOWN_PLACEHOLDERS.contains(&name.as_str()) => {
                out.push(name.clone())
            }
            Piece::Optional(inner) => collect_unknown(inner, out),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update_fields() -> MarkerFields<'static> {
        MarkerFields {
            table: Some("PARAMETRI"),
            operation: Some("upsert"),
            from_version: Some("4.12"),
            to_version: Some("4.13"),
            hash: None,
        }
    }

    fn init_fields() -> MarkerFields<'static> {
        MarkerFields { table: Some("PARAMETRI"), operation: Some("insert"), ..Default::default() }
    }

    #[test]
    fn the_default_template_serves_both_kinds_of_script() {
        let t = MarkerTemplate::default();
        assert_eq!(
            t.render(&update_fields()).unwrap(),
            "-- picus: generated PARAMETRI (4.12 -> 4.13)"
        );
        // The whole version clause disappears rather than leaving "( -> )".
        assert_eq!(t.render(&init_fields()).unwrap(), "-- picus: generated PARAMETRI");
    }

    #[test]
    fn an_empty_template_switches_marking_off() {
        let t = MarkerTemplate(String::new());
        assert!(t.is_disabled());
        assert_eq!(t.render(&update_fields()), None);
        assert!(!t.recognises("-- picus: generated PARAMETRI"));

        assert!(MarkerTemplate("   ".to_string()).is_disabled());
    }

    #[test]
    fn a_marker_recognises_its_own_output_in_both_shapes() {
        let t = MarkerTemplate::default();
        assert!(t.recognises(&t.render(&update_fields()).unwrap()));
        assert!(t.recognises(&t.render(&init_fields()).unwrap()));
        // Leading whitespace: a marker indented inside a block is still a marker.
        assert!(t.recognises("    -- picus: generated PARAMETRI (4.12 -> 4.13)"));
    }

    #[test]
    fn a_marker_does_not_recognise_a_neighbouring_picus_comment() {
        // The suppression comments share the namespace and must not be mistaken
        // for a generated-block marker.
        let t = MarkerTemplate::default();
        assert!(!t.recognises("-- picus: ignore DML001 — full reload on install"));
        assert!(!t.recognises("-- picus: generated"));
        assert!(!t.recognises("INSERT INTO PARAMETRI (COD) VALUES ('X');"));
    }

    #[test]
    fn the_shape_matches_even_when_the_values_changed() {
        // A block generated before the table was renamed must still be findable,
        // or a regeneration would append instead of replacing.
        let t = MarkerTemplate::default();
        assert!(t.recognises("-- picus: generated LISTINI (1.0 -> 1.1)"));
    }

    #[test]
    fn a_custom_template_works_and_unknown_placeholders_are_reported() {
        let t = MarkerTemplate("-- {autore} touched {table}".to_string());
        assert_eq!(t.unknown_placeholders(), vec!["autore".to_string()]);
        // It still renders — the unknown placeholder is simply empty, and the
        // settings UI is what tells the user before they commit the file.
        assert_eq!(t.render(&update_fields()).unwrap(), "--  touched PARAMETRI");
        assert!(MarkerTemplate::default().unknown_placeholders().is_empty());
    }

    #[test]
    fn a_malformed_template_degrades_to_literal_text() {
        // A typo in a config file must not take the project down.
        let t = MarkerTemplate("-- picus: {table".to_string());
        assert_eq!(t.render(&update_fields()).unwrap(), "-- picus: {table");
        let t = MarkerTemplate("-- picus: [{table}".to_string());
        assert_eq!(t.render(&update_fields()).unwrap(), "-- picus: [PARAMETRI");
    }

    #[test]
    fn an_optional_segment_of_pure_text_is_always_kept() {
        let t = MarkerTemplate("-- gen[erated] {table}".to_string());
        assert_eq!(t.render(&init_fields()).unwrap(), "-- generated PARAMETRI");
    }
}
