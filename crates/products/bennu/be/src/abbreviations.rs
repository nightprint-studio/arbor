//! Abbreviations, on the wire — `psf` and `sout` in a Java file, and whatever you have written for
//! the rest.
//!
//! Java's built-in vocabulary is [`bennu_java::templates`]; yours is [`crate::templates_live`], and
//! what is here is the shaping: which prefix the caret is on, whether this is a place an
//! abbreviation means anything, and the snippet body turned into text plus tab stops.
//!
//! ## Which abbreviation belongs to which file
//!
//! A template's own name says it: `dbg.rs.jinja` is for Rust, `logd.java.jinja` for Java, and one
//! written `notes.jinja` — no language in the name — is offered everywhere, which is what a template
//! that writes a comment or a licence header wants. The built-in ones stay Java's: `psvm` in a
//! `.rs` file would be a joke at the reader's expense.
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
use bennu_templates::prelude::{render_code, unmet, LiveContext, TemplateFacts};
use bennu_lsp::prelude::parse_snippet;
use bennu_proto::prelude::{CompletionItem, SnippetStop};
use bennu_proto::prelude::SourceEdit;

use crate::templates_live::LiveTemplate;

/// The abbreviation completions for a caret in a Java buffer, in the order they should be read: the
/// user's own abbreviations first, then the built-in ones — minus any the user redefined.
///
/// Empty for every other language, for a caret that is not on a word, and for a word that begins
/// no abbreviation — which is nearly every keystroke, and costs a table scan of thirteen rows.
pub(crate) fn completions(file: &str, source: &str, offset: usize) -> Vec<CompletionItem> {
    completions_with(&crate::templates_live::live_templates(), &|| facts_of(file), file, source, offset)
}

/// `project` and `style` for the project `file` belongs to — the defaults outside any project.
fn facts_of(file: &str) -> TemplateFacts {
    crate::index_service::IndexService::global()
        .root_for_file(file)
        .map(|root| crate::templates_facts::facts_at(&root, file))
        .unwrap_or_default()
}

/// `facts` is asked only when one of the user's abbreviations starts with the word — which is rarely.
fn completions_with(
    mine: &[LiveTemplate],
    facts: &dyn Fn() -> TemplateFacts,
    file: &str,
    source: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    let Some((start, prefix)) = word_before(source, offset) else { return Vec::new() };
    let mut items: Vec<CompletionItem> = Vec::new();
    let mut matching: Vec<&LiveTemplate> = mine
        .iter()
        .filter(|t| writes_for(&t.extension, file) && starts_with_ignoring_case(&t.name, prefix))
        .collect();
    if !matching.is_empty() {
        let facts = facts();
        // Offered only where the project meets what it requires.
        matching.retain(|t| unmet(&t.requires, &facts.project).is_none());
        // Rendered now, with the file it is typed in — `class_name` is only known here.
        let context = LiveContext::new(file, source);
        for template in matching {
            match render_code(&template.text, &context, &facts, None) {
                Ok(output) => items.push(item(
                    &template.name,
                    template.description.as_deref().unwrap_or("Your abbreviation"),
                    output.text.trim_end_matches(['\n', '\r']),
                    prefix,
                    (start, offset),
                    items.len(),
                    import_edits_in(file, source, &output.imports),
                )),
                // A broken template is left out of the popup rather than breaking it; what is wrong
                // with it is in the log, and its preview says so where it is being edited.
                Err(error) => eprintln!("bennu-be: abbreviation `{}` does not render: {error}", template.name),
            }
        }
    }
    // `psf` and its family are Java's own — offered in a Java file and nowhere else.
    for template in matching_templates(prefix).into_iter().filter(|_| is_java(file)) {
        if mine.iter().any(|t| t.name.eq_ignore_ascii_case(template.abbrev)) {
            continue;
        }
        items.push(item(template.abbrev, template.detail, template.body, prefix, (start, offset), items.len(), Vec::new()));
    }
    items
}

/// The imports an abbreviation's expansion asked for, as edits to the buffer it is typed in — applied after it.
fn import_edits_in(file: &str, source: &str, imports: &[String]) -> Vec<SourceEdit> {
    crate::templates_imports::import_edits(source, imports)
        .into_iter()
        .map(|edit| SourceEdit { file: file.to_string(), start: edit.start, end: edit.end, new_text: edit.replacement })
        .collect()
}

fn item(
    abbrev: &str,
    detail: &str,
    body: &str,
    prefix: &str,
    (start, offset): (usize, usize),
    index: usize,
    edits: Vec<SourceEdit>,
) -> CompletionItem {
    let parsed = parse_snippet(body);
    CompletionItem {
        label: abbrev.to_string(),
        kind: "snippet".to_string(),
        detail: Some(detail.to_string()),
        insert_text: Some(parsed.text),
        snippet: true,
        // The placeholder number rides along: two stops written `${1:name}` are one thing typed in
        // two places, and only the parser knows they were the same number.
        snippet_stops: parsed
            .stops
            .into_iter()
            .map(|s| SnippetStop { start: s.start, end: s.end, group: s.index })
            .collect(),
        // The abbreviation itself is what gets replaced, not whatever the frontend would have guessed
        // the word under the caret was: `psf` is three characters the user typed on purpose and all
        // three go.
        replace_start: Some(start),
        replace_end: Some(offset),
        // Above the members: somebody who has typed `psf` has typed an abbreviation, and a field
        // called `psfCount` matching the same three letters is not what they meant.
        sort_text: Some(format!("0{index:02}")),
        // Exactly what was typed — Enter should take it without an arrow key first.
        preselect: abbrev == prefix,
        edits,
        ..Default::default()
    }
}

/// Prefix match the way the built-ins do it: what is being typed decides which abbreviation is meant,
/// and nobody typing `Log` means a different template from the one they named `logd`.
fn starts_with_ignoring_case(name: &str, prefix: &str) -> bool {
    name.len() >= prefix.len() && name.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn is_java(file: &str) -> bool {
    extension_of(file).eq_ignore_ascii_case("java")
}

/// Whether a template that writes `extension` belongs in `file`.
///
/// A template with no language in its name — `notes.jinja` — is for every file: what it writes is
/// text, and a comment banner or a licence header has no language of its own. Anything else has to
/// match, because an abbreviation that expands Java into a shader is worse than one that is missing.
fn writes_for(extension: &str, file: &str) -> bool {
    extension.is_empty() || extension.eq_ignore_ascii_case(extension_of(file))
}

fn extension_of(file: &str) -> &str {
    match file.rsplit_once('.') {
        Some((_, extension)) if !extension.contains(['/', '\\']) => extension,
        _ => "",
    }
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

    fn mine(name: &str, text: &str) -> LiveTemplate {
        for_language(name, "java", text)
    }

    fn for_language(name: &str, extension: &str, text: &str) -> LiveTemplate {
        LiveTemplate {
            name: name.to_string(),
            description: None,
            requires: Vec::new(),
            extension: extension.to_string(),
            text: text.to_string(),
        }
    }

    /// The built-ins lower the prefix before matching, so an abbreviation of the user's that did not
    /// was the same word behaving differently depending on who wrote it.
    #[test]
    fn an_abbreviation_of_yours_is_matched_whatever_case_it_is_typed_in() {
        let templates = [mine("Hget", "get()")];
        let src = "class A { hget";
        let items = completions_with(&templates, &TemplateFacts::default, "/p/A.java", src, src.len());
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Hget"]);
    }

    /// One of yours named like a built-in replaces it — including when only the case differs, or the
    /// popup would offer the same word twice.
    #[test]
    fn yours_replaces_the_built_in_it_is_named_after() {
        let templates = [mine("PSVM", "void main()")];
        let src = "class A { psvm";
        let items = completions_with(&templates, &TemplateFacts::default, "/p/A.java", src, src.len());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "PSVM");
    }

    #[test]
    fn a_prefix_in_a_java_file_offers_its_family() {
        let src = "class A { psf";
        let items = completions_with(&[], &TemplateFacts::default, "/p/A.java", src, src.len());
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
        let items = completions_with(&[], &TemplateFacts::default, "/p/A.java", src, src.len());
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
        assert!(completions_with(&[], &TemplateFacts::default, "/p/A.java", src, src.len()).is_empty());
    }

    #[test]
    fn an_annotation_position_is_left_to_the_annotations() {
        let src = "class A { @psf";
        assert!(completions_with(&[], &TemplateFacts::default, "/p/A.java", src, src.len()).is_empty());
    }

    /// The whole point of the extension in a template's name: `dbg.rs.jinja` is Rust's.
    #[test]
    fn an_abbreviation_is_offered_in_the_language_it_writes() {
        let templates = [for_language("dbg", "rs", "dbg!($0);")];
        let src = "fn main() { dbg";
        let rust = completions_with(&templates, &TemplateFacts::default, "/p/main.rs", src, src.len());
        assert_eq!(rust.len(), 1, "offered in the language it writes");
        let java = completions_with(&templates, &TemplateFacts::default, "/p/A.java", src, src.len());
        assert!(java.is_empty(), "and in no other");
    }

    /// A template whose name says no language writes text, and text has no language.
    #[test]
    fn an_abbreviation_with_no_language_is_offered_everywhere() {
        let templates = [for_language("lic", "", "// (c) $0")];
        let src = "fn main() { lic";
        for file in ["/p/main.rs", "/p/A.java", "/p/notes.md"] {
            assert_eq!(
                completions_with(&templates, &TemplateFacts::default, file, src, src.len()).len(),
                1,
                "expected it in {file}",
            );
        }
    }

    /// The built-in family is Java's: `psvm` in a shader is a joke at the reader's expense.
    #[test]
    fn the_built_in_abbreviations_stay_in_java() {
        let src = "fn main() { psf";
        assert!(completions_with(&[], &TemplateFacts::default, "/p/main.rs", src, src.len()).is_empty());
        assert!(completions_with(&[], &TemplateFacts::default, "/p/page.jsp", src, src.len()).is_empty());
    }

    #[test]
    fn a_caret_that_is_not_on_a_word_offers_nothing() {
        let src = "class A { ";
        assert!(completions_with(&[], &TemplateFacts::default, "/p/A.java", src, src.len()).is_empty());
    }

    /// A caret in the middle of a multi-byte character must not panic — the offset arrives from a
    /// buffer, and a buffer holds whatever the file holds.
    #[test]
    fn an_offset_inside_a_character_is_refused_rather_than_split() {
        let src = "class A { è";
        assert!(completions_with(&[], &TemplateFacts::default, "/p/A.java", src, src.len() - 1).is_empty());
    }

    #[test]
    fn a_users_abbreviation_comes_first_renders_with_the_file_and_replaces_a_built_in_of_its_name() {
        let mine = vec![
            LiveTemplate { requires: Vec::new(), name: "psf".into(), description: Some("Mine".into()), text: "public static final ${1:String} ${2:NAME} = ${3:null};$0".into() },
            LiveTemplate { requires: Vec::new(), name: "logx".into(), description: None, text: "LOG.info(\"{{ class_name }}: $1\");$0\n".into() },
        ];
        let src = "class OrderService { psf";
        let items = completions_with(&mine, &TemplateFacts::default, "/p/OrderService.java", src, src.len());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["psf", "psfi", "psfs"], "the user's psf replaces the built-in one");
        assert_eq!(items[0].detail.as_deref(), Some("Mine"));

        let src = "class OrderService { logx";
        let items = completions_with(&mine, &TemplateFacts::default, "/p/OrderService.java", src, src.len());
        assert_eq!(items[0].insert_text.as_deref(), Some("LOG.info(\"OrderService: \");"));
    }
}
