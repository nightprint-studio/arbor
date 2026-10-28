//! Whole-statement templates that **read the surrounding code** — `iter`.
//!
//! The abbreviations of `bennu-java` (`psvm`, `sout`, `fori`) are text: they expand the same
//! everywhere. IntelliJ's `iter` is not — it loops over *the collection in scope* — and that is why
//! it was left out of them: a version that guesses writes a name that is not there. With the type
//! of every local at hand the guess disappears, so `iter` is offered here instead, by the one rule
//! that keeps it honest:
//!
//! > it is offered only when a local or a parameter in scope **is** an array or an `Iterable`.
//!
//! The loop it writes is the postfix `.for` of that variable (the same catalogue, so the same
//! element type, the same name spelled the project's way, `var` from Java 10, and the same
//! imports) — typing `orders.for` and typing `iter` with `orders` in scope produce one loop. With
//! several candidates the nearest wins: the variable declared closest to the caret, then the
//! innermost scope. That is a choice, so the row says which variable it loops over; it is a popup
//! item, never ghost text.
//!
//! Offered only where a statement can start: right after a `;`, `{` or `}`, inside a body, with at
//! least two letters typed (`i` alone would put it on every word starting with `i`).

use bennu_complete::prelude::{match_tier, MatchCase};
use bennu_java::prelude::{
    declared_variable_names, extract_symbols, parse_java, postfix_expansions, postfix_indent_unit, postfix_shape,
    visible_bindings, PostfixContext, PostfixSubject, TypeResolver,
};
use bennu_naming::prelude::Convention;
use bennu_proto::prelude::{CompletionItem, SnippetStop};
use tree_sitter::Node;

use crate::postfix::{import_edits, package_of, POSTFIX_LEGACY_LEVEL};

/// The word that asks for the loop.
pub const ITER: &str = "iter";

/// Fewer letters than this and the template would follow every word that starts with `i`.
const MIN_TYPED: usize = 2;

/// [`iter_completion`] when the user has statement templates switched on (Settings → Completion).
pub(crate) fn completions(
    file: &str,
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
    level: Option<u32>,
    naming: Option<Convention>,
    case: MatchCase,
) -> Vec<CompletionItem> {
    if !bennu_query::prelude::completion_tuning().statements {
        return Vec::new();
    }
    iter_completion(file, source, offset, resolver, level, naming, case)
}

/// The `iter` row for a caret typing a statement's first word, or nothing — see the module docs for
/// every condition. `level` is the Java level of the file's module (`None` is read as Java 8, the
/// assumption that fails safely) and `naming` the convention the project declared for locals.
pub fn iter_completion(
    file: &str,
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
    level: Option<u32>,
    naming: Option<Convention>,
    case: MatchCase,
) -> Vec<CompletionItem> {
    iter_item(file, source, offset, resolver, level, naming, case).into_iter().collect()
}

fn iter_item(
    file: &str,
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
    level: Option<u32>,
    naming: Option<Convention>,
    case: MatchCase,
) -> Option<CompletionItem> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    let (start, typed) = statement_word(source, offset)?;
    if typed.chars().count() < MIN_TYPED || match_tier(ITER, &typed, case).is_none() {
        return None;
    }
    // The buffer with the word's continuation spliced in, as scope completion repairs it: a half
    // statement can flatten the enclosing method, and then there is no scope left to walk.
    let sited = format!("{}x{}", &source[..offset], &source[offset..]);
    let tree = parse_java(&sited)?;
    let root = tree.root_node();
    let node = root.named_descendant_for_byte_range(offset, (offset + 1).min(sited.len()))?;
    if !inside_a_body(node) {
        return None;
    }
    let symbols = extract_symbols(&sited);
    let mut bindings = visible_bindings(&root, &sited, &symbols, offset, resolver);
    // The innermost scope, and within it the declaration nearest the caret.
    bindings.sort_by(|a, b| a.depth.cmp(&b.depth).then(b.decl.cmp(&a.decl)));
    let (name, mut shape) = bindings.iter().find_map(|binding| {
        let shape = postfix_shape(binding.ty.as_ref()?, &binding.name, resolver);
        (shape.array_element.is_some() || shape.iterable_element.is_some()).then(|| (binding.name.clone(), shape))
    })?;
    if let Some(convention) = Convention::for_variables(naming, declared_variable_names(source)) {
        shape.respell(|n| convention.render(n).unwrap_or_else(|| n.to_string()));
    }

    let unit = postfix_indent_unit(source);
    let ctx = PostfixContext { level: level.unwrap_or(POSTFIX_LEGACY_LEVEL), unit: &unit };
    let subject = PostfixSubject::Value { text: name.clone(), shape };
    let expansion = postfix_expansions(&subject, &ctx, &|n: &str| n == "for").into_iter().next()?;

    let stops = expansion.stops.iter().map(|s| SnippetStop { start: s.start, end: s.end, group: s.group }).collect();
    let edits = import_edits(file, source, package_of(source).as_deref(), &expansion.imports);
    Some(CompletionItem {
        label: ITER.to_string(),
        kind: "snippet".to_string(),
        detail: Some(format!("for each element of {name}")),
        insert_text: Some(expansion.text),
        snippet: true,
        snippet_stops: stops,
        replace_start: Some(start),
        replace_end: Some(offset),
        edits,
        // Beside the abbreviations (`000`, `001`, …), above the members.
        sort_text: Some("0iter".to_string()),
        // Exactly what was typed — Enter should take it without an arrow key first.
        preselect: typed == ITER,
        ..Default::default()
    })
}

/// The word being typed and where it starts, when it is the first word of a statement: after a `;`,
/// `{` or `}`, and not after a `.` or an `@`.
fn statement_word(source: &str, offset: usize) -> Option<(usize, String)> {
    let (start, typed) = bennu_query::prelude::split_completion_prefix(source, offset);
    let before = source[..start].trim_end();
    before.ends_with([';', '{', '}']).then_some((start, typed))
}

/// Whether `node` is inside code that runs — a block, a switch body, an initializer — rather than
/// directly in a type's body, where a statement cannot start.
fn inside_a_body(node: Node<'_>) -> bool {
    let mut current = Some(node);
    while let Some(n) = current {
        match n.kind() {
            "block" | "switch_block" | "constructor_body" | "static_initializer" => return true,
            "class_body" | "interface_body" | "enum_body" | "enum_body_declarations" | "annotation_type_body"
            | "program" => return false,
            _ => {}
        }
        current = n.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use bennu_java::prelude::{ClassMembers, Import, TypeRef};

    use super::*;

    /// A tiny JDK: `Iterable`, `Collection`, `List`, plus `String` and two project classes.
    struct Jdk(HashMap<&'static str, ClassMembers>);

    fn class(params: &[&str], supers: Vec<TypeRef>) -> ClassMembers {
        ClassMembers {
            type_params: params.iter().map(|p| p.to_string()).collect(),
            superclass: None,
            interfaces: supers,
            methods: Vec::new(),
            fields: Vec::new(),
            flags: Default::default(),
        }
    }

    fn of(name: &str, argument: &str) -> TypeRef {
        TypeRef { type_args: vec![TypeRef::simple(argument)], ..TypeRef::simple(name) }
    }

    impl Jdk {
        fn new() -> Self {
            Jdk(HashMap::from([
                ("java/lang/Iterable", class(&["T"], vec![])),
                ("java/util/Collection", class(&["E"], vec![of("java/lang/Iterable", "E")])),
                ("java/util/List", class(&["E"], vec![of("java/util/Collection", "E")])),
                ("java/lang/String", class(&[], vec![])),
                ("shop/Order", class(&[], vec![])),
                ("shop/OrderLine", class(&[], vec![])),
            ]))
        }
    }

    impl TypeResolver for Jdk {
        fn members_of(&self, binary_name: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(binary_name).cloned().map(Arc::new)
        }

        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            Some(match name {
                "List" => "java/util/List",
                "String" => "java/lang/String",
                "Order" => "shop/Order",
                "OrderLine" => "shop/OrderLine",
                _ => return None,
            })
            .map(str::to_string)
        }
    }

    /// The rows offered at the `¶` marker in `marked`, which is removed from the source first.
    fn offered(marked: &str, level: Option<u32>, naming: Option<Convention>) -> Vec<CompletionItem> {
        let offset = marked.find('¶').expect("a caret marker");
        let source = marked.replacen('¶', "", 1);
        iter_completion("/p/A.java", &source, offset, &Jdk::new(), level, naming, MatchCase::default())
    }

    /// A method taking a list of orders, with `statements` as its body.
    fn body(statements: &str) -> String {
        format!("class A {{\n    void m(List<Order> orders) {{\n{statements}\n    }}\n}}\n")
    }

    #[test]
    fn iter_loops_over_the_iterable_in_scope() {
        let marked = body("        iter¶");
        let start = marked.find("iter").unwrap();
        let items = offered(&marked, None, None);
        assert_eq!(items.len(), 1, "{items:?}");
        let item = &items[0];
        assert_eq!((item.label.as_str(), item.kind.as_str()), ("iter", "snippet"));
        assert!(item.snippet && item.preselect);
        assert!(item.insert_text.as_deref().unwrap().starts_with("for (Order order : orders) {"), "{:?}", item.insert_text);
        assert_eq!((item.replace_start, item.replace_end), (Some(start), Some(start + 4)));
        assert!(!item.snippet_stops.is_empty(), "the caret lands in the body");
    }

    #[test]
    fn it_follows_the_java_level_like_the_postfix_loop_does() {
        let modern = offered(&body("        iter¶"), Some(17), None);
        assert!(modern[0].insert_text.as_deref().unwrap().starts_with("for (var order : orders)"));
    }

    #[test]
    fn a_partly_typed_word_still_offers_it_but_does_not_preselect() {
        let items = offered(&body("        it¶"), None, None);
        assert_eq!(items.len(), 1);
        assert!(!items[0].preselect);
    }

    #[test]
    fn the_nearest_variable_wins() {
        let marked = body("        List<Order> pending = null;\n        iter¶");
        let item = &offered(&marked, None, None)[0];
        assert!(item.insert_text.as_deref().unwrap().contains(": pending)"), "{:?}", item.insert_text);
        assert_eq!(item.detail.as_deref(), Some("for each element of pending"));
    }

    #[test]
    fn an_array_is_looped_over_too() {
        let marked = "class A {\n    void m(String[] names) {\n        iter¶\n    }\n}\n";
        let item = &offered(marked, None, None)[0];
        assert!(item.insert_text.as_deref().unwrap().starts_with("for (String name : names)"), "{:?}", item.insert_text);
    }

    /// A version that guesses writes a name that is not there.
    #[test]
    fn nothing_without_an_array_or_iterable_in_scope() {
        let marked = "class A {\n    void m(int n, String s) {\n        iter¶\n    }\n}\n";
        assert!(offered(marked, None, None).is_empty());
    }

    #[test]
    fn the_variable_is_named_the_projects_way() {
        let marked = "class A {\n    void m(List<OrderLine> orderLines) {\n        iter¶\n    }\n}\n";
        let item = &offered(marked, None, Some(Convention::LowerSnake))[0];
        assert!(item.insert_text.as_deref().unwrap().contains("order_line"), "{:?}", item.insert_text);
    }

    #[test]
    fn only_a_word_that_could_be_iter_is_answered() {
        assert!(offered(&body("        i¶"), None, None).is_empty(), "one letter");
        assert!(offered(&body("        iterate¶"), None, None).is_empty(), "a longer word");
        assert!(offered(&body("        for¶"), None, None).is_empty(), "another word");
    }

    #[test]
    fn only_where_a_statement_starts() {
        assert!(offered(&body("        foo(iter¶"), None, None).is_empty(), "inside a call");
        assert!(offered(&body("        orders.iter¶"), None, None).is_empty(), "after a dot");
        assert!(offered(&body("        int x = iter¶"), None, None).is_empty(), "in an expression");
        // After a statement and after a block both start one.
        assert_eq!(offered(&body("        run();\n        iter¶"), None, None).len(), 1);
        assert_eq!(offered(&body("        if (x) { }\n        iter¶"), None, None).len(), 1);
    }

    #[test]
    fn a_member_position_is_not_a_statement() {
        let inside = |source: &str, marker: &str| {
            let tree = parse_java(source).expect("parse");
            let at = source.find(marker).unwrap();
            let node = tree.root_node().named_descendant_for_byte_range(at, at + marker.len()).expect("a node");
            inside_a_body(node)
        };
        assert!(!inside("class A { int f; it x; }", "it x"), "a field's type");
        assert!(inside("class A { void m() { it x; } }", "it x"), "a local's type");
    }

    #[test]
    fn a_caret_inside_a_character_or_past_the_end_is_refused() {
        let source = body("        é");
        let ask = |offset: usize| {
            iter_completion("/p/A.java", &source, offset, &Jdk::new(), None, None, MatchCase::default())
        };
        let end_of_e = source.find('é').unwrap() + 'é'.len_utf8();
        assert!(ask(end_of_e - 1).is_empty());
        assert!(ask(source.len() + 5).is_empty());
    }
}
