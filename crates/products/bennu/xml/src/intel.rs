//! The editor's answers, from the grammar and the caret.
//!
//! ## One gate above everything
//!
//! **No grammar, no answer.** Every function here starts from a resolved [`Grammar`], and a
//! document whose schema nobody could find gets nothing at all — not completion from the tags
//! already in the file, which would confidently propose whatever typo is already there.
//!
//! ## And a second gate on the checks
//!
//! Completion being incomplete costs a keystroke; a false diagnostic costs trust, and one is
//! enough for a user to turn the whole feature off. So the checks below only speak where the
//! grammar genuinely knows the answer, and three rules keep them there:
//!
//! - an element the schema declares **open** (`ANY`, `xs:any`, `mixed`) silences everything
//!   inside it;
//! - a **prefixed** name is never reported. A prefix means a namespace, and a document mixing
//!   four namespaces — the normal case in Spring XML — has at most one of them resolved. The
//!   unresolved ones must be invisible, not wrong;
//! - an element whose **parent** the grammar does not know is not judged either: the position is
//!   unknown, so nothing about it can be.
//!
//! Nothing here checks text content or cardinality. The grammar records both, and a curated or
//! flattened one is exactly the wrong place to be confident about either.

use bennu_complete::prelude::{ghost, token_after, within, Proposal, Proposals};
use bennu_ext::prelude::{ExtHover, ExtTarget};
use bennu_proto::prelude::{CompletionItem, Diagnostic};

use crate::caret::{classify, is_name_char, Caret};
use crate::grammar::{Attribute, Element, Grammar, GrammarKind};
use crate::scan::{local_name, Scan, Tag, TagKind};

/// Attribute prefixes that belong to XML itself rather than to the grammar. Never completed,
/// never reported — `xmlns:foo` and `xsi:schemaLocation` are how a document says which grammar
/// it wants, and a schema has no reason to declare them.
const RESERVED: &[&str] = &["xmlns", "xsi", "xml"];

fn reserved(name: &str) -> bool {
    RESERVED.contains(&name) || name.split_once(':').is_some_and(|(p, _)| RESERVED.contains(&p))
}

// ── Completion ───────────────────────────────────────────────────────────────

/// Candidates at the caret.
pub fn completions(grammar: &Grammar, scan: &Scan, source: &str, offset: usize) -> Vec<CompletionItem> {
    let mut out = Proposals::default();
    for p in candidates(grammar, scan, source, offset) {
        if !out.offer(p) && out.is_full() {
            break;
        }
    }
    out.into_items()
}

/// The candidates, before they become wire data — shared with [`inline_hint`] so the popup and
/// the ghost text can never disagree about what is possible.
fn candidates(grammar: &Grammar, scan: &Scan, source: &str, offset: usize) -> Vec<Proposal> {
    if grammar.is_empty() {
        return Vec::new();
    }
    let Some(caret) = classify(scan, source, offset) else { return Vec::new() };
    match &caret {
        // Closing a tag: there is exactly one right answer, and it is not a list.
        Caret::ElementName { parent, prefix, closing: true, .. } => {
            starts_with(parent, prefix)
                .then(|| vec![Proposal::new(parent.clone(), "element").detail("closes this element")])
                .unwrap_or_default()
        }
        Caret::ElementName { parent, prefix, .. } => {
            let legal: Vec<&Element> = if parent.is_empty() {
                // Before the root. The schema's declared roots, or — for a DTD, which names none
                // — every element it declares.
                match grammar.roots.is_empty() {
                    true => grammar.elements.iter().collect(),
                    false => grammar.roots.iter().filter_map(|r| grammar.element(r)).collect(),
                }
            } else {
                grammar.children_of(parent)
            };
            legal
                .into_iter()
                .filter(|e| starts_with(&e.name, prefix))
                .map(|e| Proposal::new(e.name.clone(), "element").detail(element_detail(e)))
                .collect()
        }
        Caret::AttrName { element, prefix, .. } => {
            let Some(e) = grammar.element(element) else { return Vec::new() };
            // Already written ones are gone: an attribute may appear once, and offering it twice
            // is offering something that cannot be accepted.
            let written = scan.tag_at(offset).map(attribute_names).unwrap_or_default();
            e.attributes
                .iter()
                .filter(|a| starts_with(&a.name, prefix) && !written.contains(&a.name))
                .map(|a| Proposal::new(a.name.clone(), "attribute").detail(attribute_detail(a)))
                .collect()
        }
        Caret::AttrValue { element, attr, prefix, .. } => {
            let Some(a) = grammar.element(element).and_then(|e| e.attribute(attr)) else {
                return Vec::new();
            };
            a.values
                .iter()
                .filter(|v| starts_with(v, prefix))
                .map(|v| Proposal::new(v.clone(), "value"))
                .collect()
        }
        // Text. The grammar says what may be written here only in the sense of "some string",
        // and offering the element names again would be offering them in the one place a `<` has
        // not been typed.
        Caret::Content { .. } => Vec::new(),
    }
}

/// Case-sensitive, because XML names are. `Order` is not `order`.
fn starts_with(candidate: &str, prefix: &str) -> bool {
    local_name(candidate).starts_with(local_name(prefix))
}

fn attribute_names(tag: &Tag) -> Vec<String> {
    tag.attrs.iter().map(|a| a.local().to_string()).collect()
}

fn element_detail(e: &Element) -> String {
    let mut parts = Vec::new();
    let required: Vec<&str> =
        e.attributes.iter().filter(|a| a.required).map(|a| a.name.as_str()).collect();
    if !required.is_empty() {
        parts.push(format!("needs {}", required.join(", ")));
    }
    if !e.children.is_empty() {
        parts.push(format!("{} children", e.children.len()));
    } else if e.text {
        parts.push("text".to_string());
    }
    parts.join("  ·  ")
}

fn attribute_detail(a: &Attribute) -> String {
    let mut parts = Vec::new();
    if a.required {
        parts.push("required".to_string());
    }
    if !a.values.is_empty() {
        parts.push(a.values.join(" | "));
    } else if !a.default.is_empty() {
        parts.push(format!("= {}", a.default));
    }
    parts.join("  ·  ")
}

// ── Ghost text ───────────────────────────────────────────────────────────────

/// The continuation that certainly follows the caret, or `None`.
///
/// Built from the same candidate list the popup shows, through the same rule ([`ghost`]) — so the
/// two can never disagree, and "there is exactly one thing this could be" means the same here as it
/// does in a property file.
///
/// One addition the general rule cannot express: an attribute the schema **fixes** to a single
/// value is certain even with nothing typed, because there is no other legal thing to write.
///
/// And one subtraction: nothing at all is offered while the buffer already holds the rest of the
/// name ahead of the caret. `</jav|a.version>` is where XML meets this — closing tags are written
/// by the editor, so the caret is *inside* a finished name far more often here than in a property
/// file, and every one of those carets has a certain continuation that is already on screen.
pub fn inline_hint(grammar: &Grammar, scan: &Scan, source: &str, offset: usize) -> Option<String> {
    // The same predicate the caret classifier tokenises names with, so `>`, `=` and quotes — which
    // no name can absorb — correctly do not count as "already written".
    let (_, ahead) = token_after(source, offset, is_name_char);
    if let Some(Caret::AttrValue { element, attr, prefix, .. }) = classify(scan, source, offset) {
        // `ahead` applies to the fixed value too: `encoding="|UTF-8"` is a caret with nothing
        // typed and nothing missing.
        if prefix.is_empty() && ahead.is_empty() {
            let fixed = grammar.element(&element)?.attribute(&attr)?.fixed.clone();
            if !fixed.is_empty() {
                return Some(fixed);
            }
        }
    }
    let prefix = match classify(scan, source, offset)? {
        Caret::ElementName { prefix, .. }
        | Caret::AttrName { prefix, .. }
        | Caret::AttrValue { prefix, .. } => prefix,
        Caret::Content { .. } => return None,
    };
    let labels: Vec<String> =
        candidates(grammar, scan, source, offset).into_iter().map(|p| p.label).collect();
    ghost(&prefix, ahead, labels)
}

// ── Hover ────────────────────────────────────────────────────────────────────

/// What the schema says about the thing under the caret.
pub fn hover(grammar: &Grammar, scan: &Scan, source: &str, offset: usize) -> Option<ExtHover> {
    let tag = scan.tag_at(offset)?;
    let element = grammar.element(&tag.name)?;

    // On an attribute: the attribute is the subject, not the element it happens to be on.
    if let Some(attr) = tag.attr_name_at(offset).or_else(|| tag.attr_value_at(offset)) {
        let declared = element.attribute(&attr.name)?;
        let mut lines = Vec::new();
        if declared.required {
            lines.push("Required.".to_string());
        }
        if !declared.values.is_empty() {
            lines.push(format!("One of: {}", declared.values.join(", ")));
        }
        if !declared.fixed.is_empty() {
            lines.push(format!("Fixed to `{}`.", declared.fixed));
        } else if !declared.default.is_empty() {
            lines.push(format!("Defaults to `{}`.", declared.default));
        }
        if !declared.doc.is_empty() {
            lines.push(declared.doc.clone());
        }
        lines.push(from_line(grammar));
        return Some(ExtHover {
            title: attr.name.clone(),
            signature: format!("attribute of <{}>", tag.name),
            doc: lines.join("\n"),
        });
    }

    let mut lines = Vec::new();
    if !element.doc.is_empty() {
        lines.push(element.doc.clone());
    }
    if element.open {
        // Worth saying out loud: it explains why nothing inside is ever flagged.
        lines.push("Holds any content, so nothing inside it is checked.".to_string());
    } else if !element.children.is_empty() {
        lines.push(format!("Contains: {}", element.children.join(", ")));
    } else if element.text {
        lines.push("Holds text.".to_string());
    }
    let required: Vec<&str> =
        element.attributes.iter().filter(|a| a.required).map(|a| a.name.as_str()).collect();
    if !required.is_empty() {
        lines.push(format!("Required attributes: {}", required.join(", ")));
    }
    lines.push(from_line(grammar));
    Some(ExtHover {
        title: tag.name.clone(),
        signature: format!("<{}>", tag.name),
        doc: lines.join("\n"),
    })
}

/// Which schema this came from. On every card, because "what is this file even checked against"
/// is the question a user has before any of the others.
fn from_line(grammar: &Grammar) -> String {
    match grammar.kind {
        Some(kind) => format!("From {} ({})", short_source(&grammar.source), kind.label()),
        None => String::new(),
    }
}

fn short_source(source: &str) -> &str {
    source.rsplit(['/', '\\']).next().unwrap_or(source)
}

// ── Go to declaration ────────────────────────────────────────────────────────

/// Jump from a tag (or an attribute) to where the schema declares it.
///
/// The feature that makes a schema-aware editor feel like one: `<result type="…">` is a word
/// until you can press a key and land on the `<!ATTLIST result …>` that defines it.
///
/// Empty when the declaration has no location — the built-in grammar declares things in a Rust
/// table, and pretending otherwise would open the wrong file.
pub fn navigate(grammar: &Grammar, scan: &Scan, _source: &str, offset: usize) -> Vec<ExtTarget> {
    // The URL the document names its schema by. Following it is the thing anyone tries first —
    // it *looks* like a link — and until now it did nothing, which reads as broken rather than
    // as unsupported. It lands on the file the URL actually resolved to, which is usually the
    // copy inside a jar rather than anything at that address.
    if let Some(dt) = &scan.doctype {
        if within(offset, dt.offset, dt.end) {
            return schema_target(grammar, &dt.system_id);
        }
    }
    let Some(tag) = scan.tag_at(offset) else { return Vec::new() };
    if let Some(a) = tag.attr_value_at(offset) {
        if a.local().ends_with("schemaLocation") {
            return schema_target(grammar, &location_at(a, offset));
        }
    }
    let Some(element) = grammar.element(&tag.name) else { return Vec::new() };

    let (decl, label, detail) = match tag.attr_name_at(offset) {
        Some(attr) => match element.attribute(&attr.name) {
            Some(declared) => (
                &declared.decl,
                attr.name.clone(),
                format!("attribute of <{}>", tag.local()),
            ),
            None => return Vec::new(),
        },
        None => (&element.decl, tag.name.clone(), format!("declared in {}", short_source(&grammar.source))),
    };
    if decl.file.is_empty() {
        return Vec::new();
    }
    vec![ExtTarget { file: decl.file.clone(), offset: decl.offset, label, detail }]
}

// ── Checks ───────────────────────────────────────────────────────────────────

/// What the grammar says is wrong, held to the standard in the module docs.
pub fn diagnostics(grammar: &Grammar, scan: &Scan) -> Vec<Diagnostic> {
    if grammar.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for tag in &scan.tags {
        if tag.kind == TagKind::Close || !tag.closed || tag.name.contains(':') {
            continue;
        }
        let parent = scan.path_at(tag.start).last().copied().unwrap_or_default();
        // Under an open element nothing is checked, and under an element the grammar has never
        // heard of nothing *can* be: the position itself is unknown.
        if !parent.is_empty() {
            match grammar.element(parent) {
                Some(p) if p.open => continue,
                Some(_) => {}
                None => continue,
            }
        }
        let Some(element) = grammar.element(&tag.name) else {
            out.push(diagnostic(
                format!("`{}` is not declared in {}", tag.name, short_source(&grammar.source)),
                "xml.unknown-element",
                tag.name_start,
                tag.name_end,
            ));
            continue;
        };
        // A real element in the wrong place. Only when the parent declares a child list at all —
        // an element the schema says contains nothing may still be one this grammar flattened
        // (see `bennu-xsd` on particles), and guessing there would report correct files as wrong.
        if let Some(p) = grammar.element(parent) {
            if !p.children.is_empty() && !p.children.iter().any(|c| c == &tag.name) {
                out.push(diagnostic(
                    format!("`{}` is not allowed inside `{parent}`", tag.name),
                    "xml.misplaced-element",
                    tag.name_start,
                    tag.name_end,
                ));
            }
        }

        for attr in &tag.attrs {
            if reserved(&attr.name) || attr.name.contains(':') {
                continue;
            }
            if element.attribute(&attr.name).is_none() && !element.open {
                out.push(diagnostic(
                    format!("`{}` has no attribute `{}`", tag.name, attr.name),
                    "xml.unknown-attribute",
                    attr.name_start,
                    attr.name_end,
                ));
            }
        }
        for declared in element.attributes.iter().filter(|a| a.required) {
            if !tag.attrs.iter().any(|a| a.local() == declared.name) {
                out.push(diagnostic(
                    format!("`{}` requires the attribute `{}`", tag.name, declared.name),
                    "xml.missing-attribute",
                    tag.name_start,
                    tag.name_end,
                ));
            }
        }
    }
    out
}

/// Where "go to the schema" lands.
///
/// Two answers, in this order, because the link means two different things depending on whether
/// the file it names is on the machine:
///
/// 1. **the local copy that actually answered.** Usually the one inside a dependency jar, which
///    is the file this document is really checked against — not the address it is written as;
/// 2. **the address itself**, when nothing local resolved. A built-in grammar has no file, and a
///    schema nobody ships has no copy — but the URL is still a URL, and opening it is what
///    anyone clicking a link expects. The host recognises an `http(s)` target and hands it to the
///    browser rather than to the editor.
///
/// Empty when there is neither, which is the only honest answer left.
fn schema_target(grammar: &Grammar, location: &str) -> Vec<ExtTarget> {
    let local = matches!(grammar.kind, Some(k) if k != GrammarKind::Builtin)
        && !grammar.source.is_empty();
    if local {
        return vec![ExtTarget {
            file: grammar.source.clone(),
            offset: 0,
            label: short_source(&grammar.source).to_string(),
            detail: format!(
                "the {} this file is checked against",
                grammar.kind.map(|k| k.label()).unwrap_or("schema"),
            ),
        }];
    }
    let url = location.trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        return vec![ExtTarget {
            file: url.to_string(),
            offset: 0,
            label: short_source(url).to_string(),
            detail: "not on this machine — opens in the browser".to_string(),
        }];
    }
    Vec::new()
}

/// The one location under the caret in an `xsi:schemaLocation`.
///
/// The attribute is a whitespace-separated list of *pairs* — namespace, then location — so the
/// token the pointer is on is as likely to be a namespace as a URL. Landing on a namespace means
/// the location is the token after it, which is what a reader means by "this one".
fn location_at(attr: &crate::scan::Attr, offset: usize) -> String {
    let mut tokens: Vec<(usize, &str)> = Vec::new();
    let mut at = attr.value_start;
    for part in attr.value.split_inclusive(char::is_whitespace) {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            tokens.push((at, trimmed));
        }
        at += part.len();
    }
    let hit = tokens.iter().position(|(start, t)| offset >= *start && offset <= start + t.len());
    match hit {
        // A namespace is followed by its location; a location is itself.
        Some(i) if i % 2 == 0 && tokens.len() > i + 1 => tokens[i + 1].1.to_string(),
        Some(i) => tokens[i].1.to_string(),
        // Off any token (the space between two): the last location is the best guess, and it is
        // the only one on a single-pair attribute, which is nearly all of them.
        None => tokens.last().map(|(_, t)| t.to_string()).unwrap_or_default(),
    }
}

fn diagnostic(message: String, code: &str, start: usize, end: usize) -> Diagnostic {
    Diagnostic { message, severity: "warning".to_string(), code: code.to_string(), start, end }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::from_dtd;
    use crate::scan::scan;

    const DTD: &str = "<!-- A Struts configuration. -->\n\
        <!ELEMENT struts (package*, constant*)>\n\
        <!ELEMENT package (action*)>\n\
        <!ATTLIST package name CDATA #REQUIRED namespace CDATA #IMPLIED>\n\
        <!ELEMENT action (result*)>\n\
        <!ATTLIST action name CDATA #REQUIRED class CDATA #IMPLIED>\n\
        <!ELEMENT result (#PCDATA)>\n\
        <!ATTLIST result name CDATA \"success\" type (dispatcher|redirect|redirectAction) #IMPLIED>\n\
        <!ELEMENT constant ANY>";

    fn grammar() -> Grammar {
        from_dtd(&bennu_dtd::prelude::parse(DTD), "/p/struts-2.5.dtd")
    }

    /// Complete at the `|`, which is stripped first.
    fn complete(text: &str) -> Vec<String> {
        let offset = text.find('|').expect("mark the caret");
        let src = text.replace('|', "");
        completions(&grammar(), &scan(&src), &src, offset).into_iter().map(|c| c.label).collect()
    }

    fn ghost(text: &str) -> Option<String> {
        let offset = text.find('|').expect("mark the caret");
        let src = text.replace('|', "");
        inline_hint(&grammar(), &scan(&src), &src, offset)
    }

    #[test]
    fn element_names_come_from_what_the_parent_may_contain() {
        assert_eq!(complete("<struts><|"), ["package", "constant"]);
        assert_eq!(complete("<struts><package><|"), ["action"]);
        // Filtered by what has been typed — and only among what the parent may contain, so a
        // real element in the wrong place is not offered either.
        assert_eq!(complete("<struts><pa|"), ["package"]);
        assert!(complete("<struts><package><pa|").is_empty(), "`package` cannot go here");
    }

    /// A DTD names no root, so before the root every declared element is a candidate — which is
    /// the honest answer rather than a silent empty list.
    #[test]
    fn before_the_root_the_declared_elements_are_offered() {
        assert!(complete("<|").contains(&"struts".to_string()));
    }

    #[test]
    fn a_closing_tag_has_exactly_one_right_answer() {
        assert_eq!(complete("<struts><package></|"), ["package"]);
        assert_eq!(complete("<struts><package></zz|"), Vec::<String>::new());
    }

    #[test]
    fn attributes_are_offered_once_and_carry_what_the_schema_says() {
        let items = {
            let text = "<struts><package |>";
            let offset = text.find('|').unwrap();
            let src = text.replace('|', "");
            completions(&grammar(), &scan(&src), &src, offset)
        };
        assert_eq!(items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["name", "namespace"]);
        assert_eq!(items[0].detail.as_deref(), Some("required"));
        // Already written, so no longer offered.
        assert_eq!(complete("<struts><package name=\"x\" |>"), ["namespace"]);
    }

    #[test]
    fn values_are_offered_only_where_the_schema_closes_the_set() {
        assert_eq!(
            complete("<struts><package><action><result type=\"|\">"),
            ["dispatcher", "redirect", "redirectAction"],
        );
        assert_eq!(complete("<struts><package><action><result type=\"redirect|\">"), ["redirect", "redirectAction"]);
        // `name` is CDATA — an open set, and offering its default as the only entry would dress
        // a guess up as a choice.
        assert!(complete("<struts><package><action><result name=\"|\">").is_empty());
    }

    #[test]
    fn ghost_text_appears_only_where_exactly_one_thing_can_follow() {
        assert_eq!(ghost("<struts><pa|").as_deref(), Some("ckage"));
        // Two candidates share the prefix, so nothing is drawn.
        assert_eq!(ghost("<struts><action><result type=\"redirect|\""), None);
        assert_eq!(ghost("<struts><|"), None, "nothing typed, nothing certain");
        assert_eq!(ghost("<struts><package><action><result type=\"redirectA|\"").as_deref(), Some("ction"));
    }

    /// The case that reaches XML far more often than a property file, because the editor writes
    /// closing tags for you: the caret lands *inside* a finished name, the continuation is
    /// certain, and it is already on screen one character to the right.
    #[test]
    fn nothing_is_ghosted_into_a_name_the_buffer_already_finishes() {
        assert_eq!(ghost("<struts><pack|age>"), None);
        assert_eq!(ghost("<struts><package></pack|age>"), None);
        // Still answered where the rest of the name is genuinely missing — a `>` is not part of
        // any name, so it does not count as already written.
        assert_eq!(ghost("<struts><pack|>").as_deref(), Some("age"));
    }

    #[test]
    fn hover_says_what_the_element_is_and_where_that_came_from() {
        let src = "<struts><package name=\"x\"/></struts>";
        let g = grammar();
        let s = scan(src);
        let h = hover(&g, &s, src, src.find("struts").unwrap() + 1).unwrap();
        assert_eq!(h.title, "struts");
        assert!(h.doc.contains("A Struts configuration."));
        assert!(h.doc.contains("Contains: package, constant"));
        assert!(h.doc.contains("struts-2.5.dtd (DTD)"));

        let h = hover(&g, &s, src, src.find("name=").unwrap() + 1).unwrap();
        assert_eq!(h.title, "name", "the attribute is the subject, not the element");
        assert!(h.doc.contains("Required."));
    }

    #[test]
    fn go_to_lands_on_the_declaration_in_the_schema() {
        let src = "<struts><package name=\"x\"/></struts>";
        let g = grammar();
        let s = scan(src);
        let t = navigate(&g, &s, src, src.find("package").unwrap() + 1);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file, "/p/struts-2.5.dtd");
        assert!(DTD[t[0].offset..].starts_with("<!ELEMENT package"));

        let t = navigate(&g, &s, src, src.find("name=").unwrap() + 1);
        assert!(DTD[t[0].offset..].starts_with("name CDATA"), "the attribute's own declaration");
    }

    #[test]
    fn a_builtin_grammar_offers_no_go_to_rather_than_the_wrong_file() {
        let g = crate::builtin::pom();
        let src = "<project><modelVersion>4.0.0</modelVersion></project>";
        assert!(navigate(&g, &scan(src), src, 2).is_empty());
    }

    /// A schema nobody ships locally still has an address, and following a link is what anyone
    /// clicking one expects. The host sends an `http` target to the browser.
    #[test]
    fn a_schema_with_no_local_copy_falls_back_to_the_address_it_names() {
        let g = crate::builtin::pom();
        let src = r#"<project xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd"/>"#;
        // On the namespace half of the pair — the location is the token after it.
        let t = navigate(&g, &scan(src), src, src.find("http").unwrap() + 4);
        assert_eq!(t[0].file, "http://maven.apache.org/xsd/maven-4.0.0.xsd");
        // And on the location itself.
        let t = navigate(&g, &scan(src), src, src.find("/xsd/").unwrap());
        assert_eq!(t[0].file, "http://maven.apache.org/xsd/maven-4.0.0.xsd");
        assert!(t[0].detail.contains("browser"));

        // A relative location that resolved to nothing is not a link, so nothing is offered.
        let src = r#"<a xsi:noNamespaceSchemaLocation="missing.xsd"/>"#;
        let empty = Grammar { kind: Some(GrammarKind::Xsd), ..Grammar::default() };
        assert!(navigate(&empty, &scan(src), src, src.find("missing").unwrap()).is_empty());
    }

    /// The link everyone tries first. It resolves to the copy that actually answered — usually
    /// one inside a jar, never the address it is written as.
    #[test]
    fn the_schema_a_document_names_can_be_followed() {
        let g = grammar();
        let src = "<!DOCTYPE struts SYSTEM \"http://struts.apache.org/dtds/struts-2.5.dtd\">\n<struts/>";
        let t = navigate(&g, &scan(src), src, src.find("http").unwrap() + 4);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file, "/p/struts-2.5.dtd");
        assert!(t[0].detail.contains("DTD"));
        // Anywhere on the declaration, not only on the URL — a ctrl+click lands where the
        // pointer was.
        assert_eq!(navigate(&g, &scan(src), src, 4).len(), 1);

        let src = r#"<struts xsi:schemaLocation="urn:x http://example.com/struts.xsd"><package/></struts>"#;
        assert_eq!(navigate(&g, &scan(src), src, src.find("urn:x").unwrap()).len(), 1);
    }

    // ── Checks ───────────────────────────────────────────────────────────────

    fn check(src: &str) -> Vec<String> {
        diagnostics(&grammar(), &scan(src)).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn an_undeclared_element_and_a_misplaced_one_are_told_apart() {
        assert_eq!(
            check("<struts><packge/></struts>"),
            ["`packge` is not declared in struts-2.5.dtd"],
        );
        assert_eq!(
            check("<struts><action/></struts>"),
            [
                "`action` is not allowed inside `struts`".to_string(),
                "`action` requires the attribute `name`".to_string(),
            ],
        );
    }

    #[test]
    fn attributes_are_checked_both_ways() {
        assert_eq!(
            check("<struts><package name=\"x\" nmspace=\"y\"/></struts>"),
            ["`package` has no attribute `nmspace`"],
        );
        assert_eq!(
            check("<struts><package namespace=\"y\"/></struts>"),
            ["`package` requires the attribute `name`"],
        );
    }

    /// The three gates, each of which exists because breaking it produced a false report.
    #[test]
    fn nothing_is_reported_where_the_grammar_does_not_actually_know() {
        // Under an ANY element.
        assert!(check("<struts><constant><anything foo=\"1\"/></constant></struts>").is_empty());
        // A prefixed name — its namespace may have no schema here at all.
        assert!(check("<struts><s:custom s:attr=\"1\"/></struts>").is_empty());
        // Under a parent the grammar never declared.
        assert!(check("<struts><packge><action/></packge></struts>").len() == 1);
        // With no grammar at all.
        assert!(diagnostics(&Grammar::default(), &scan("<anything><at all=\"1\"/></anything>")).is_empty());
        // Reserved attributes are XML's, not the schema's.
        assert!(check("<struts xmlns:s=\"urn:x\" xsi:schemaLocation=\"a b\"/>").is_empty());
    }

    /// Mid-edit, before the tag is finished, there is nothing to be wrong about yet.
    #[test]
    fn an_unterminated_tag_is_not_reported() {
        assert!(check("<struts><packg").is_empty());
    }
}
