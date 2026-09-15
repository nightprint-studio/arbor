//! Postfix templates in the completion list — `orders.for`, `found.ifpe`.
//!
//! The catalogue is `bennu-java`'s and is pure. What is here is the half that needs the project: the
//! type of the expression before the dot (the same inference member completion runs), the language
//! level of the module the file belongs to, the typed name matched the way every other candidate is,
//! and each expansion turned into a completion item that replaces the whole expression and adds the
//! imports its text names.
//!
//! ## One site, two catalogues
//!
//! The user's own postfix templates (`bennu-templates`' `postfix` kind) are rendered by the backend, which
//! has the template store. They are offered at the same place and replace the same range as the built-ins,
//! so the two halves are public here — [`postfix_site`] finds and types the expression, [`postfix_item`]
//! makes the item — and the built-ins go through both. A second copy of either would be two answers to
//! "is this a postfix site" that drift the first time one of them learns a new case.

use bennu_complete::prelude::{match_tier, MatchCase};
use bennu_intentions::prelude::insert_import_edit;
use bennu_java::prelude::{
    declared_variable_names, extract_symbols, infer_receiver_type, postfix_expansions, postfix_indent_unit,
    postfix_shape, postfix_subject_start, supertype_names, PostfixContext, PostfixExpansion, PostfixSubject,
    TypeRef, TypeResolver,
};
use bennu_naming::prelude::Convention;
use bennu_proto::prelude::{CompletionItem, SnippetStop, SourceEdit};

/// The level assumed when the module's is not known — the old language, which is the assumption
/// that fails safely: a template missing is a keystroke, a template that does not compile is a bug.
pub const POSTFIX_LEGACY_LEVEL: u32 = 8;

/// A caret after `expression.name`, with the expression found and typed.
#[derive(Debug, Clone)]
pub struct PostfixSite {
    /// Where the expression before the dot starts — the start of what accepting a template replaces.
    pub start: usize,
    /// The caret, where that range ends.
    pub caret: usize,
    /// What has been typed after the dot (`fo`) — never empty.
    pub typed: String,
    /// The expression, as a value with its shape or as a class name.
    pub subject: PostfixSubject,
    /// The value's inferred type, kept for [`PostfixSite::supertypes`]; `None` for a class name.
    receiver: Option<TypeRef>,
}

impl PostfixSite {
    /// A site whose subject was typed by other means. It has no inferred type to walk, so
    /// [`PostfixSite::supertypes`] answers nothing — what a caller that already knows the subject, or a test,
    /// wants.
    pub fn for_subject(start: usize, caret: usize, typed: &str, subject: PostfixSubject) -> Self {
        Self { start, caret, typed: typed.to_string(), subject, receiver: None }
    }

    /// The dotted names of the value's type and of every type above it, nearest first
    /// (`java.util.ArrayList`, …, `java.util.Collection`, `java.lang.Iterable`) — how "is it one of these"
    /// is answered for a class a template names. Nested types are written with dots (`java.util.Map.Entry`).
    ///
    /// Empty for an array, which is none of its element's supertypes, and for a class name. A walk of the
    /// hierarchy, so asked only by a template that names a class.
    pub fn supertypes(&self, resolver: &dyn TypeResolver) -> Vec<String> {
        let Some(ty) = self.receiver.as_ref().filter(|ty| ty.dims == 0) else { return Vec::new() };
        // The type itself first: a class the resolver cannot answer for is not walked, and is still itself.
        let mut out = vec![dotted(&ty.binary_name)];
        for name in supertype_names(resolver, &ty.binary_name) {
            let name = dotted(&name);
            if !out.contains(&name) {
                out.push(name);
            }
        }
        out
    }
}

/// The postfix site at `caret`, or `None`.
///
/// `None` when no name has been started after the dot: on a bare `order.` the list is the members, and
/// forty templates under them would be forty rows nobody asked for. `None`, too, on a `package` or
/// `import` line, and when the expression's type is not known — a template chosen by type is exactly the
/// one that must not guess it.
///
/// `naming` is the convention the project declared for local variables, if any. The names the templates
/// declare (`.var`, `.for`) are spelled in it, or else in the one the file's names already follow — see
/// [`Convention::for_variables`].
pub fn postfix_site(
    source: &str,
    caret: usize,
    resolver: &dyn TypeResolver,
    naming: Option<Convention>,
) -> Option<PostfixSite> {
    if caret > source.len() || !source.is_char_boundary(caret) {
        return None;
    }
    let (word_start, typed) = bennu_query::prelude::split_completion_prefix(source, caret);
    if typed.is_empty() || word_start == 0 || source.as_bytes()[word_start - 1] != b'.' {
        return None;
    }
    let dot = word_start - 1;
    if on_a_header_line(source, dot) {
        return None;
    }
    let start = postfix_subject_start(source, dot)?;
    let (mut subject, receiver) = subject_of(source, (start, dot), (word_start, caret), resolver)?;
    if let PostfixSubject::Value { shape, .. } = &mut subject {
        if let Some(convention) = Convention::for_variables(naming, declared_variable_names(source)) {
            shape.respell(|name| convention.render(name).unwrap_or_else(|| name.to_string()));
        }
    }
    Some(PostfixSite { start, caret, typed, subject, receiver })
}

/// What a postfix completion writes, from either catalogue.
#[derive(Debug, Clone)]
pub struct PostfixItem<'a> {
    /// What is typed after the dot.
    pub label: &'a str,
    /// The popup's right-hand column.
    pub detail: String,
    /// The replacement for the whole expression, at column 0.
    pub text: String,
    /// Tab stops, as byte ranges into `text`, in visiting order.
    pub stops: Vec<SnippetStop>,
    /// The fully-qualified classes `text` names; the ones the file has, or never needs, are skipped.
    pub imports: &'a [String],
}

/// The completion item for a postfix template at `site`: kind `postfix`, replacing the expression through
/// the caret, with the import edits `item.imports` needs in `file`.
pub fn postfix_item(file: &str, source: &str, site: &PostfixSite, item: PostfixItem<'_>) -> CompletionItem {
    item_in(file, source, package_of(source).as_deref(), site, item)
}

/// The built-in postfix templates for a caret after `expression.name`, or nothing — see [`postfix_site`].
pub(crate) fn postfix_completions(
    file: &str,
    source: &str,
    caret: usize,
    resolver: &dyn TypeResolver,
    level: Option<u32>,
    naming: Option<Convention>,
    case: MatchCase,
) -> Vec<CompletionItem> {
    let Some(site) = postfix_site(source, caret, resolver, naming) else { return Vec::new() };
    let unit = postfix_indent_unit(source);
    let ctx = PostfixContext { level: level.unwrap_or(POSTFIX_LEGACY_LEVEL), unit: &unit };
    let wanted = |name: &str| match_tier(name, &site.typed, case).is_some();
    let package = package_of(source);
    postfix_expansions(&site.subject, &ctx, &wanted)
        .into_iter()
        .map(|expansion| built_in_item(file, source, package.as_deref(), &site, expansion))
        .collect()
}

/// The expression at `start..dot`, typed — a value, or a class name for `.new` — and the value's type.
fn subject_of(
    source: &str,
    (start, dot): (usize, usize),
    (word_start, caret): (usize, usize),
    resolver: &dyn TypeResolver,
) -> Option<(PostfixSubject, Option<TypeRef>)> {
    let text = source.get(start..dot)?.to_string();
    // The typed name is cut out so the receiver ends cleanly at the dot — the repair member completion
    // makes, for the same reason: `orders.fo` does not parse as anything with a receiver in it.
    let repaired = format!("{}{}", source.get(..word_start)?, source.get(caret..)?);
    if let Some(ty) = infer_receiver_type(&repaired, word_start, resolver) {
        let shape = postfix_shape(&ty, &text, resolver);
        return Some((PostfixSubject::Value { text, shape }, Some(ty)));
    }
    class_subject(source, text, resolver).map(|subject| (subject, None))
}

/// A class name that `new` can instantiate: `Order.new`. Inference answers nothing for a type
/// receiver — a type is not an expression — so it is resolved as a name instead.
fn class_subject(source: &str, text: String, resolver: &dyn TypeResolver) -> Option<PostfixSubject> {
    let pascal = text.starts_with(|c: char| c.is_ascii_uppercase())
        && text.chars().any(|c| c.is_ascii_lowercase())
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if !pascal {
        return None;
    }
    let imports = extract_symbols(source).imports;
    let binary = resolver.resolve_simple_name(&text, &imports)?;
    let members = resolver.members_of(&binary)?;
    let flags = &members.flags;
    let instantiable = !(flags.is_interface || flags.is_abstract || flags.is_enum || flags.is_annotation);
    instantiable.then_some(PostfixSubject::Type { text })
}

fn built_in_item(
    file: &str,
    source: &str,
    package: Option<&str>,
    site: &PostfixSite,
    expansion: PostfixExpansion,
) -> CompletionItem {
    let stops = expansion
        .stops
        .iter()
        .map(|stop| SnippetStop { start: stop.start, end: stop.end, group: stop.group })
        .collect();
    let item = PostfixItem {
        label: expansion.name,
        detail: expansion.detail,
        text: expansion.text,
        stops,
        imports: &expansion.imports,
    };
    item_in(file, source, package, site, item)
}

fn item_in(file: &str, source: &str, package: Option<&str>, site: &PostfixSite, item: PostfixItem<'_>) -> CompletionItem {
    CompletionItem {
        label: item.label.to_string(),
        kind: "postfix".to_string(),
        detail: Some(item.detail),
        edits: import_edits(file, source, package, item.imports),
        snippet: true,
        snippet_stops: item.stops,
        insert_text: Some(item.text),
        // The whole expression goes, not just the name after the dot: `orders.for` becomes the loop.
        replace_start: Some(site.start),
        replace_end: Some(site.caret),
        ..Default::default()
    }
}

/// The import lines an expansion needs that the file does not have.
///
/// Two imports that belong at the same line are merged into one insertion: two edits at one offset
/// would be applied in an order nothing here controls.
fn import_edits(file: &str, source: &str, package: Option<&str>, imports: &[String]) -> Vec<SourceEdit> {
    let mut edits: Vec<SourceEdit> = Vec::new();
    for fqn in imports {
        let Some((owner, _)) = fqn.rsplit_once('.') else { continue };
        if owner == "java.lang" || Some(owner) == package {
            continue;
        }
        let Some(edit) = insert_import_edit(source, fqn) else { continue };
        match edits.iter_mut().find(|e| e.start == edit.start && e.end == edit.end) {
            Some(same) => same.new_text.push_str(&edit.replacement),
            None => edits.push(SourceEdit {
                file: file.to_string(),
                start: edit.start,
                end: edit.end,
                new_text: edit.replacement,
            }),
        }
    }
    edits
}

/// Whether the dot is on a `package` or `import` line, where a dot separates names, not a value from
/// what to do with it.
fn on_a_header_line(source: &str, dot: usize) -> bool {
    let line_start = source[..dot].rfind('\n').map_or(0, |i| i + 1);
    let line = source[line_start..dot].trim_start();
    line.starts_with("import ") || line.starts_with("package ")
}

fn package_of(source: &str) -> Option<String> {
    source
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("package "))
        .map(|rest| rest.trim_end_matches(';').trim().to_string())
}

/// `java/util/Map$Entry` → `java.util.Map.Entry`.
fn dotted(binary: &str) -> String {
    binary.replace(['/', '$'], ".")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bennu_java::prelude::{ClassMembers, Import};

    use super::*;

    const SOURCE: &str = "package com.acme;\n\nimport java.util.List;\n\nclass A {}\n";

    /// A resolver that knows nothing — enough for the guards that answer before any inference.
    struct Nothing;

    impl TypeResolver for Nothing {
        fn members_of(&self, _binary_name: &str) -> Option<Arc<ClassMembers>> {
            None
        }
        fn resolve_simple_name(&self, _name: &str, _imports: &[Import]) -> Option<String> {
            None
        }
    }

    #[test]
    fn imports_the_file_already_has_or_never_needs_are_not_added() {
        let imports = ["java.lang.String".to_string(), "java.util.List".to_string(), "com.acme.Order".to_string()];
        assert!(import_edits("/p/A.java", SOURCE, Some("com.acme"), &imports).is_empty());
    }

    #[test]
    fn imports_that_land_on_one_line_are_one_insertion() {
        let imports = ["java.util.Arrays".to_string(), "java.util.Objects".to_string()];
        let edits = import_edits("/p/A.java", SOURCE, Some("com.acme"), &imports);
        assert_eq!(edits.len(), 1, "{edits:?}");
        assert!(edits[0].new_text.contains("import java.util.Arrays;"));
        assert!(edits[0].new_text.contains("import java.util.Objects;"));
    }

    #[test]
    fn a_dot_in_an_import_or_package_line_is_not_a_postfix_site() {
        let src = "package com.acme;\nimport java.util.fo";
        assert!(on_a_header_line(src, src.rfind('.').unwrap()));
        assert!(postfix_site(src, src.len(), &Nothing, None).is_none());
        let body = "class A { void m() { orders.fo";
        assert!(!on_a_header_line(body, body.rfind('.').unwrap()));
    }

    /// On a bare `orders.` the popup is the members, and a caret mid-character is refused rather than split.
    #[test]
    fn no_site_before_a_name_is_started_or_inside_a_character() {
        let bare = "class A { void m() { orders.";
        assert!(postfix_site(bare, bare.len(), &Nothing, None).is_none());
        let wide = "class A { void m() { è";
        assert!(postfix_site(wide, wide.len() - 1, &Nothing, None).is_none());
    }

    #[test]
    fn the_package_is_read_off_its_declaration() {
        assert_eq!(package_of(SOURCE).as_deref(), Some("com.acme"));
        assert_eq!(package_of("class A {}"), None);
    }

    #[test]
    fn a_nested_binary_name_is_written_with_dots() {
        assert_eq!(dotted("java/util/Map$Entry"), "java.util.Map.Entry");
    }

    /// The shape a user's template shares with the built-ins: the whole expression replaced, the stops as
    /// they were given, and the imports as edits to the file.
    #[test]
    fn an_item_replaces_the_expression_through_the_caret_and_keeps_its_stops() {
        let source = "package com.acme;\n\nclass A { void m() { this.orders.logv";
        let start = source.find("this.orders").unwrap();
        let site = PostfixSite {
            start,
            caret: source.len(),
            typed: "logv".to_string(),
            subject: PostfixSubject::Type { text: "Order".to_string() },
            receiver: None,
        };
        let imports = ["java.util.Objects".to_string()];
        let item = postfix_item(
            "/p/A.java",
            source,
            &site,
            PostfixItem {
                label: "logv",
                detail: "Mine".to_string(),
                text: "log.debug(this.orders);".to_string(),
                stops: vec![SnippetStop { start: 23, end: 23, group: 0 }],
                imports: &imports,
            },
        );
        assert_eq!(item.kind, "postfix");
        assert_eq!((item.replace_start, item.replace_end), (Some(start), Some(source.len())));
        assert_eq!(item.snippet_stops.len(), 1);
        assert_eq!(item.snippet_stops[0].start, 23);
        assert!(item.snippet);
        assert_eq!(item.edits.len(), 1, "{:?}", item.edits);
        assert!(item.edits[0].new_text.contains("import java.util.Objects;"));
    }
}
