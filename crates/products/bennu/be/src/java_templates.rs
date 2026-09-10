//! Java's abbreviations, on the wire — `psf`, `sout`, `psvm`.
//!
//! The vocabulary is [`bennu_java::templates`]; what is here is the shaping: which prefix the
//! caret is on, whether this is a place an abbreviation means anything, and the snippet body
//! turned into text plus tab stops.
//!
//! ## Why the parse happens here
//!
//! The bodies are written in LSP snippet syntax, and this workspace already has one tested parser
//! for it (`bennu-lsp`'s `snippet`), used on the way out of every language server. Reusing it is
//! the difference between one grammar with tests and two that drift — and the wire shape is
//! identical either way, so the frontend cannot tell a template from rust-analyzer's own snippet,
//! which is exactly right.
//!
//! ## Where they are offered
//!
//! Only where a **bare word** is being typed. After a `.` the popup is answering "what members
//! does this have", and an abbreviation there is a wrong answer to a precise question; after an
//! `@` it is answering "which annotation", same thing. Everything else is fair game — a class
//! body, a method body, a field's type position — because the abbreviation only appears once its
//! own prefix has been typed, and `psf` is not a word anyone types by accident.

use bennu_java::prelude::matching_templates;
use bennu_lsp::prelude::parse_snippet;
use bennu_proto::prelude::{CompletionItem, SnippetStop};

/// The abbreviation completions for a caret in a Java buffer, in the order they should be read.
///
/// Empty for every other language, for a caret that is not on a word, and for a word that begins
/// no abbreviation — which is nearly every keystroke, and costs a table scan of thirteen rows.
pub(crate) fn completions(file: &str, source: &str, offset: usize) -> Vec<CompletionItem> {
    if !is_java(file) {
        return Vec::new();
    }
    let Some((start, prefix)) = word_before(source, offset) else { return Vec::new() };
    matching_templates(prefix)
        .into_iter()
        .enumerate()
        .map(|(i, template)| {
            let parsed = parse_snippet(template.body);
            CompletionItem {
                label: template.abbrev.to_string(),
                kind: "snippet".to_string(),
                detail: Some(template.detail.to_string()),
                insert_text: Some(parsed.text),
                snippet: true,
                snippet_stops: parsed
                    .stops
                    .into_iter()
                    .map(|s| SnippetStop { start: s.start, end: s.end })
                    .collect(),
                // The abbreviation itself is what gets replaced, not whatever the frontend would
                // have guessed the word under the caret was: `psf` is three characters the user
                // typed on purpose and all three go.
                replace_start: Some(start),
                replace_end: Some(offset),
                // Above the members: somebody who has typed `psf` has typed an abbreviation, and
                // a field called `psfCount` matching the same three letters is not what they meant.
                sort_text: Some(format!("0{i:02}")),
                // Exactly what was typed — Enter should take it without an arrow key first.
                preselect: template.abbrev == prefix,
                ..Default::default()
            }
        })
        .collect()
}

fn is_java(file: &str) -> bool {
    file.rsplit('.').next().is_some_and(|e| e.eq_ignore_ascii_case("java"))
}

/// The identifier being typed that ends at `offset`, and where it starts.
///
/// `None` when the caret is not on a word, and — the part that matters — when the word is a
/// **member or annotation name**: the character before it decides, because `list.psf` and `@psf`
/// are questions with real answers that an abbreviation would push out of the way.
fn word_before(source: &str, offset: usize) -> Option<(usize, &str)> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    if start == offset {
        return None;
    }
    if start > 0 && matches!(bytes[start - 1], b'.' | b'@') {
        return None;
    }
    Some((start, &source[start..offset]))
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_in_a_java_file_offers_its_family() {
        let src = "class A { psf";
        let items = completions("/p/A.java", src, src.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["psf", "psfi", "psfs"]);
        // The whole abbreviation is what accepting replaces.
        assert_eq!(items[0].replace_start, Some(src.len() - 3));
        assert_eq!(items[0].replace_end, Some(src.len()));
        // And the one typed exactly is the one Enter takes.
        assert!(items[0].preselect);
        assert!(!items[1].preselect);
    }

    #[test]
    fn the_body_crosses_the_wire_as_text_plus_stops() {
        let src = "class A { void m() { sout";
        let items = completions("/p/A.java", src, src.len());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].insert_text.as_deref(), Some("System.out.println();"));
        assert!(items[0].snippet);
        // The caret lands between the parentheses, which is the whole point of the abbreviation.
        assert_eq!(items[0].snippet_stops.len(), 1);
        assert_eq!(items[0].snippet_stops[0].start, "System.out.println(".len());
    }

    /// After a dot the popup is answering "what members does this have", and that question has a
    /// right answer an abbreviation would push out of the way.
    #[test]
    fn a_member_position_is_left_to_the_members() {
        let src = "class A { void m() { list.psf";
        assert!(completions("/p/A.java", src, src.len()).is_empty());
    }

    #[test]
    fn an_annotation_position_is_left_to_the_annotations() {
        let src = "class A { @psf";
        assert!(completions("/p/A.java", src, src.len()).is_empty());
    }

    #[test]
    fn nothing_is_offered_in_another_language() {
        let src = "fn main() { psf";
        assert!(completions("/p/main.rs", src, src.len()).is_empty());
        assert!(completions("/p/page.jsp", src, src.len()).is_empty());
    }

    #[test]
    fn a_caret_that_is_not_on_a_word_offers_nothing() {
        let src = "class A { ";
        assert!(completions("/p/A.java", src, src.len()).is_empty());
    }

    /// A caret in the middle of a multi-byte character must not panic — the offset arrives from a
    /// buffer, and a buffer holds whatever the file holds.
    #[test]
    fn an_offset_inside_a_character_is_refused_rather_than_split() {
        let src = "class A { è";
        assert!(completions("/p/A.java", src, src.len() - 1).is_empty());
    }
}
