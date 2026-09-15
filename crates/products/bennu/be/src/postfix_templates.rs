//! The user's postfix templates, on the wire — offered beside `for` and `nn` after a value's dot.
//!
//! The built-in catalogue is `bennu-java`'s and reaches the list through the engine; yours is
//! [`crate::templates_live`]. Both go through `bennu-intel`'s one postfix site ([`postfix_site`]) and one item
//! shape ([`postfix_item`]), so a template of yours replaces the same range, carries its stops the same way
//! and adds its imports the same way as `for` does. What is here is the half only the backend can do: the
//! template store, `bennu.applies`, rendering, and merging the result into the engine's list.
//!
//! ## One of yours named like a built-in
//!
//! It replaces the built-in — for the values it applies to. A `for` of yours that applies to `iterable`
//! leaves the built-in `for` alone on an `int`, where yours is not offered: a name you took over for one
//! kind of value is not a name you meant to remove from the others.
//!
//! [`postfix_site`]: bennu_intel::prelude::postfix_site

use std::cell::OnceCell;

use bennu_complete::prelude::{match_tier, MatchCase};
use bennu_intel::prelude::{postfix_item, PostfixItem, PostfixSite};
use bennu_java::prelude::PostfixSubject;
use bennu_lsp::prelude::parse_snippet;
use bennu_proto::prelude::{CompletionItem, SnippetStop};
use bennu_templates::prelude::{
    render_code, unmet, Output, PostfixApplicability, PostfixTemplateContext, TemplateFacts, TemplateKind,
};

use crate::index_service::IndexService;
use crate::templates_live::UserTemplate;

/// The kind tag every postfix item carries, built-in or yours.
const POSTFIX: &str = "postfix";

/// Your postfix templates for a caret after `expression.name` in `file`, or nothing.
///
/// Nothing, cheaply, for nearly every keystroke: a user with no postfix templates costs a directory
/// listing, and a caret that is not after a dot costs a byte test — the expression is typed only when both
/// say there may be something to offer.
pub(crate) fn completions(file: &str, source: &str, offset: usize, case: MatchCase) -> Vec<CompletionItem> {
    let mine = crate::templates_live::user_templates(TemplateKind::Postfix);
    if mine.is_empty() {
        return Vec::new();
    }
    let Some(at) = IndexService::global().postfix_site(file, source, offset) else { return Vec::new() };
    // The hierarchy is walked once, and only when a template names a class.
    let supertypes: OnceCell<Vec<String>> = OnceCell::new();
    let is_a = |name: &str| supertypes.get_or_init(|| at.site.supertypes(&*at.resolver)).iter().any(|s| s == name);
    let typing = Typing { file, source, site: &at.site, level: at.level, case };
    completions_at(&mine, &|| crate::templates_facts::facts_of_file(file), &typing, &is_a)
}

/// `engine`'s list with `mine` in it: where the built-in postfix items are — below the members — and with
/// every built-in dropped that one of `mine` is named after. The same comparison abbreviations make, case
/// ignored, so the popup never offers one word twice.
pub(crate) fn merge(mut engine: Vec<CompletionItem>, mine: Vec<CompletionItem>) -> Vec<CompletionItem> {
    if mine.is_empty() {
        return engine;
    }
    engine.retain(|item| item.kind != POSTFIX || !mine.iter().any(|m| m.label.eq_ignore_ascii_case(&item.label)));
    let at = engine.iter().position(|item| item.kind == POSTFIX).unwrap_or(engine.len());
    engine.splice(at..at, mine);
    engine
}

/// Where the templates are being typed.
struct Typing<'a> {
    file: &'a str,
    source: &'a str,
    site: &'a PostfixSite,
    /// The Java level of the file's module.
    level: u32,
    case: MatchCase,
}

/// `facts` is asked only when one of `mine` matches what was typed; `is_a` only by a template naming a class.
fn completions_at(
    mine: &[UserTemplate],
    facts: &dyn Fn() -> TemplateFacts,
    at: &Typing<'_>,
    is_a: &dyn Fn(&str) -> bool,
) -> Vec<CompletionItem> {
    let PostfixSubject::Value { text, shape } = &at.site.subject else { return Vec::new() };
    let named: Vec<&UserTemplate> = mine.iter().filter(|t| match_tier(&t.name, &at.site.typed, at.case).is_some()).collect();
    if named.is_empty() {
        return Vec::new();
    }
    let facts = facts();
    let context = PostfixTemplateContext::new(text, shape, at.level, at.file, at.source);
    let mut items = Vec::new();
    for template in named {
        if unmet(&template.requires, &facts.project).is_some() {
            continue;
        }
        // A template that cannot say what it applies to is left out rather than offered everywhere; like one
        // that does not render, what is wrong with it is in the log, and its preview says so.
        let applies = match PostfixApplicability::read(&template.text) {
            Ok(applies) => applies,
            Err(error) => {
                eprintln!("bennu-be: postfix template `{}` does not say what it applies to: {error}", template.name);
                continue;
            }
        };
        if !applies.accepts(&at.site.subject, at.level, is_a) {
            continue;
        }
        match render_code(&template.text, &context, &facts, None) {
            Ok(output) => items.push(item(at, template, &context, output)),
            Err(error) => eprintln!("bennu-be: postfix template `{}` does not render: {error}", template.name),
        }
    }
    items
}

fn item(at: &Typing<'_>, template: &UserTemplate, context: &PostfixTemplateContext, output: Output) -> CompletionItem {
    let parsed = parse_snippet(output.text.trim_end_matches(['\n', '\r']));
    // What the template asked for with `imported`, and the classes of `type` and `element_type` it wrote.
    let mut imports = output.imports;
    imports.extend(context.implied_imports(&parsed.text));
    imports.sort();
    imports.dedup();
    let stops = parsed.stops.into_iter().map(|s| SnippetStop { start: s.start, end: s.end, group: s.index }).collect();
    let detail = template.description.clone().unwrap_or_else(|| "Your postfix template".to_string());
    postfix_item(
        at.file,
        at.source,
        at.site,
        PostfixItem { label: &template.name, detail, text: parsed.text, stops, imports: &imports },
    )
}

#[cfg(test)]
mod tests {
    use bennu_java::prelude::{PostfixElement, PostfixShape, PostfixWritten};

    use super::*;

    const SOURCE: &str = "package com.acme;\n\nclass A { void m() { orders.l";

    fn mine(name: &str, text: &str) -> UserTemplate {
        UserTemplate {
            name: name.to_string(),
            description: None,
            requires: Vec::new(),
            extension: "java".to_string(),
            text: text.to_string(),
        }
    }

    fn written(text: &str, imports: &[&str]) -> PostfixWritten {
        PostfixWritten { text: text.to_string(), imports: imports.iter().map(|i| i.to_string()).collect() }
    }

    /// `orders`, a `List<Order>` of a class from another package, with `l` typed after its dot.
    fn site() -> PostfixSite {
        let order = PostfixElement { ty: written("Order", &["com.shop.Order"]), name: "order".to_string(), primitive: None };
        let shape = PostfixShape {
            ty: written("List<Order>", &["com.shop.Order", "java.util.List"]),
            name: "orders".to_string(),
            iterable_element: Some(order),
            collection: true,
            ..PostfixShape::default()
        };
        let start = SOURCE.find("orders").unwrap();
        PostfixSite::for_subject(start, SOURCE.len(), "l", PostfixSubject::Value { text: "orders".to_string(), shape })
    }

    fn offered(templates: &[UserTemplate], is_a: &dyn Fn(&str) -> bool, level: u32) -> Vec<CompletionItem> {
        let site = site();
        let typing = Typing { file: "/p/A.java", source: SOURCE, site: &site, level, case: MatchCase::from_flag(false) };
        completions_at(templates, &TemplateFacts::default, &typing, is_a)
    }

    fn built_in(label: &str) -> CompletionItem {
        CompletionItem { label: label.to_string(), kind: POSTFIX.to_string(), ..Default::default() }
    }

    fn member(label: &str) -> CompletionItem {
        CompletionItem { label: label.to_string(), kind: "method".to_string(), ..Default::default() }
    }

    #[test]
    fn a_template_is_offered_on_what_it_applies_to_and_nothing_broken_is() {
        let templates = [
            mine("logv", "log.debug(\"{{ expr }} = {}\", {{ expr }});$0\n"),
            mine("loop", "{# bennu.applies: iterable #}\nfor ({{ element_type }} ${1:{{ element_name }}} : {{ expr }}) {\n$0\n}\n"),
            mine("lopt", "{# bennu.applies: optional #}\n{{ expr }}.ifPresent(x -> {});"),
            mine("lcoll", "{# bennu.applies: java.util.Collection #}\n{{ expr }}.clear();"),
            mine("lnew", "{# bennu.applies: iterable #}\n{# bennu.level: 21 #}\n{{ expr }}.reversed();"),
            mine("lbroken", "{{ expr | nosuchfilter }}"),
            mine("lbad", "{# bennu.applies: iterabel #}\n{{ expr }}"),
            mine("other", "{{ expr }}"),
        ];
        let is_a_collection = |name: &str| name == "java.util.Collection";
        let items = offered(&templates, &is_a_collection, 17);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["logv", "loop", "lcoll"]);
    }

    #[test]
    fn an_item_replaces_the_expression_with_the_rendered_snippet_its_stops_and_its_imports() {
        let templates = [
            mine("logv", "log.debug(\"{{ expr }} = {}\", {{ expr }});$0\n"),
            mine("loop", "{# bennu.applies: iterable #}\nfor ({{ element_type }} ${1:{{ element_name }}} : {{ expr }}) {$0}"),
        ];
        let items = offered(&templates, &|_| false, 17);
        let start = SOURCE.find("orders").unwrap();

        let log = &items[0];
        assert_eq!(log.kind, POSTFIX);
        assert_eq!((log.replace_start, log.replace_end), (Some(start), Some(SOURCE.len())));
        assert_eq!(log.insert_text.as_deref(), Some("log.debug(\"orders = {}\", orders);"));
        assert_eq!(log.detail.as_deref(), Some("Your postfix template"));
        assert!(log.edits.is_empty(), "it writes no type: {:?}", log.edits);

        let each = &items[1];
        let text = each.insert_text.as_deref().unwrap();
        assert_eq!(text, "for (Order order : orders) {}");
        let name = &each.snippet_stops[0];
        assert_eq!(&text[name.start..name.end], "order");
        assert_eq!(each.snippet_stops.last().map(|s| s.start), Some(text.len() - 1), "$0 inside the braces");
        assert_eq!(each.edits.len(), 1, "{:?}", each.edits);
        assert!(each.edits[0].new_text.contains("import com.shop.Order;"));
    }

    /// Yours sits where the built-ins are, below the members, and takes the place of the one it is named after.
    #[test]
    fn yours_replaces_the_built_in_of_its_name_and_joins_the_built_ins() {
        let engine = vec![member("size"), built_in("for"), built_in("nn")];
        let merged = merge(engine, vec![built_in("FOR"), built_in("logv")]);
        let labels: Vec<&str> = merged.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["size", "FOR", "logv", "nn"]);
    }

    #[test]
    fn a_member_is_never_replaced_and_nothing_of_yours_changes_nothing() {
        let merged = merge(vec![member("stream")], vec![built_in("stream")]);
        let labels: Vec<(&str, &str)> = merged.iter().map(|i| (i.label.as_str(), i.kind.as_str())).collect();
        assert_eq!(labels, [("stream", "method"), ("stream", POSTFIX)]);
        assert_eq!(merge(vec![built_in("for")], Vec::new()).len(), 1);
    }
}
