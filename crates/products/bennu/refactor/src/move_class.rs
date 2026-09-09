//! **Move class** — a type that is written inside another one gets its own file.
//!
//! ```java
//! class Order {                       class Order {
//!     static class Line {        →    }
//!         final int qty;
//!     }                               // Line.java
//! }                                   class Line {
//!                                         final int qty;
//!                                     }
//! ```
//!
//! ## The one move that needs no target
//!
//! Every other move asks *where to*. This one does not: a type lifted out of its outer class lands
//! **in the same package**, and a package is a directory, so "its own file" and "beside this one"
//! are the same instruction. That removes the whole question of source roots — the part that goes
//! wrong on a multi-module build and puts the file where nothing compiles it. It is the same rule
//! [`crate::create`] follows for a type that does not exist yet, and for the same reason.
//!
//! Because the package does not change, every unqualified `Line` in the package still resolves
//! after the move. What does not is `Order.Line` — a qualified mention from anywhere — and seeing
//! those needs the reference index, which the caller has and this crate does not.
//!
//! ## What it refuses
//!
//! - **An inner class**, meaning a nested `class` without `static`. It holds a reference to an
//!   instance of the class around it (JLS §8.1.3), and a top-level type has nowhere to keep one:
//!   `new Line()` written inside `Order` becomes a constructor call that cannot be made.
//! - **A type that reads the outer class's members**, for the same reason seen from the other side.
//! - **The only top-level type in the file**, which is already in its own file.
//!
//! ## What it writes
//!
//! The file's `package` line, the imports the moved type actually mentions, and the type itself
//! dedented to column zero with the modifiers a top-level type may carry — `static` goes, because
//! nothing encloses it any more, and so do `private` and `protected`, which are not modifiers a
//! top-level type has. `public` stays, and the file is named after the type, which is what Java
//! requires of it.

use tree_sitter::Node;

use crate::body::{
    imports_of, line_after, line_start, members_of, package_of, reindent, type_named, types_in,
};
use crate::plan::{NewSource, Outcome, Plan, RefactorEdit, Refusal};
use crate::selection::{descendants, enclosing, enclosing_type, indent_at, newline, text, TYPE_DECLS};

const ID: (&str, &str) = ("move-class", "Move class to its own file");

/// Plan a *move class*: the type the caret is on leaves this file for one of its own.
pub fn move_class(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = ID;
    let type_decl = type_at(root, start, end)?;
    let name = type_decl.child_by_field_name("name").map(|n| text(&n, source))?;

    // The file's only type is already in its own file.
    let outer = enclosing_type_of(&type_decl);
    if outer.is_none() && types_in(root).iter().filter(|t| t.parent().map(|p| p.kind()) == Some("program")).count() <= 1 {
        return None;
    }
    if let Some(reason) = unfit(&type_decl, outer.as_ref(), source, name) {
        return Some(Err(Refusal::new(id, label, reason)));
    }
    // A **private member** of a nested type is visible to the class around it (JLS §6.6.1: the
    // whole top-level body is one access unit) and to nothing else. Lifted out, every one of those
    // reads stops compiling — `AVAILABLE_LOCALE_ULIST has private access in SyncAvoid`, and the
    // private constructor cases, which is what a `new Moved(…)` outside becomes.
    if let Some(taken) = private_member_used_outside(root, source, &type_decl, name) {
        return Some(Err(Refusal::new(
            id,
            label,
            format!(
                "`{taken}` is private to `{name}` and read from outside it — which only works while \
                 `{name}` is nested"
            ),
        )));
    }
    if let Some(outer_name) = outer.as_ref().and_then(|o| o.child_by_field_name("name")) {
        let outer_name = text(&outer_name, source);
        if qualified_mention(root, source, outer_name, name) {
            return Some(Err(Refusal::new(
                id,
                label,
                format!(
                    "`{outer_name}.{name}` is written somewhere in this file, and once `{name}` is \
                     top-level that name means nothing"
                ),
            )));
        }
    }

    let (from, to) = (line_start(source, type_decl.start_byte()), line_after(source, type_decl.end_byte()));
    let own_indent = indent_at(source, type_decl.start_byte());
    let moved = source.get(from..to)?.trim_end_matches(['\n', '\r']);
    let body = strip_nesting_modifiers(&reindent(moved, &own_indent, ""));

    let nl = newline(source);
    let mut file = String::new();
    if let Some(package) = package_of(root, source) {
        file.push_str(&format!("package {package};{nl}{nl}"));
    }
    let imports = imports_the_type_reads(root, &type_decl, source);
    if !imports.is_empty() {
        file.push_str(&imports.join(nl));
        file.push_str(nl);
        file.push_str(nl);
    }
    file.push_str(&body);
    file.push_str(nl);

    let plan = Plan::new(id, label, vec![RefactorEdit::new(from, to, String::new(), "removal")])
        .named(name)
        .creating(NewSource {
            name: name.to_string(),
            text: file,
            name_at: type_decl
                .child_by_field_name("name")
                .map(|n| n.start_byte())
                .unwrap_or(type_decl.start_byte()),
            was_nested: outer.is_some(),
        });
    Some(Ok(plan))
}

/// Why this type cannot be lifted out — `None` when it can.
fn unfit(
    type_decl: &Node<'_>,
    outer: Option<&Node<'_>>,
    source: &str,
    name: &str,
) -> Option<String> {
    let Some(outer) = outer else { return None };
    // Only a `class` can be an inner one: a nested interface, enum, record or annotation type is
    // implicitly static (JLS §8.1.3, §8.9, §9.5) and has no enclosing instance to lose.
    if type_decl.kind() == "class_declaration"
        && !crate::body::has_modifier(type_decl, source, "static")
    {
        return Some(format!(
            "`{name}` is an inner class — it holds a reference to an instance of `{}`, and a \
             top-level type has nowhere to keep one",
            outer.child_by_field_name("name").map(|n| text(&n, source)).unwrap_or("its outer class")
        ));
    }
    // What it reads from around it goes the same way: outside the outer class, those names are not
    // in scope at all.
    if let Some(taken) = reads_from_outer(type_decl, outer, source) {
        return Some(format!("`{name}` reads `{taken}` from the class around it"));
    }
    None
}

/// A private member of the moved type that something outside it reads, if any.
///
/// The type's own name counts as one when it has a **private constructor**: `new Moved(…)` written
/// anywhere else in the file is a use of that constructor, and it stops compiling the moment the
/// two are not in the same top-level body.
fn private_member_used_outside(
    root: Node<'_>,
    source: &str,
    type_decl: &Node<'_>,
    name: &str,
) -> Option<String> {
    let body = crate::body::body_of(type_decl)?;
    let mut private_names: Vec<String> = Vec::new();
    for member in members_of(&body) {
        if !crate::body::has_modifier(&member, source, "private") {
            continue;
        }
        match member.kind() {
            "field_declaration" => private_names.extend(
                descendants(member, "variable_declarator")
                    .iter()
                    .filter_map(|d| d.child_by_field_name("name"))
                    .map(|n| text(&n, source).to_string()),
            ),
            // A private constructor is used by writing the TYPE's name.
            "constructor_declaration" => private_names.push(name.to_string()),
            _ => private_names.extend(crate::body::member_name(&member, source).map(str::to_string)),
        }
    }
    if private_names.is_empty() {
        return None;
    }
    crate::selection::descendants_any(root, &["identifier", "type_identifier"])
        .into_iter()
        .filter(|n| {
            n.start_byte() < type_decl.start_byte() || n.end_byte() > type_decl.end_byte()
        })
        .map(|n| text(&n, source).to_string())
        .find(|word| private_names.contains(word))
}

/// Whether anything in this file spells the nested type as `Outer.Inner`.
///
/// Once it is top-level, that name means nothing — `Outer` has no member `Inner` any more. The
/// unqualified `Inner` is fine and is what nearly every mention is, which is why this looks only for
/// the qualified form.
///
/// **In this file only.** A qualified mention from ANOTHER file breaks in exactly the same way and
/// is invisible from here; seeing those is the reference index's job, and the caller has one.
fn qualified_mention(root: Node<'_>, source: &str, outer: &str, name: &str) -> bool {
    let qualified = format!("{outer}.{name}");
    crate::selection::descendants_any(root, &["scoped_type_identifier", "field_access", "scoped_identifier"])
        .iter()
        .any(|n| text(n, source) == qualified)
}

/// A member of the outer class the nested type mentions, if any.
fn reads_from_outer(type_decl: &Node<'_>, outer: &Node<'_>, source: &str) -> Option<String> {
    let body = crate::body::body_of(outer)?;
    let siblings: Vec<String> = members_of(&body)
        .iter()
        .filter(|m| m.id() != type_decl.id())
        .flat_map(|m| match m.kind() {
            "field_declaration" => descendants(*m, "variable_declarator")
                .iter()
                .filter_map(|d| d.child_by_field_name("name").map(|n| text(&n, source).to_string()))
                .collect::<Vec<_>>(),
            _ => crate::body::member_name(m, source).map(str::to_string).into_iter().collect(),
        })
        .collect();
    // Names AND written types. A sibling **nested type** is the case that matters and the one a
    // scan of identifiers alone misses: `Line implements Order.Part` written as bare `Part` still
    // resolves while `Line` is inside `Order`, and resolves to nothing the moment it is not — the
    // sibling is still nested, and a top-level class in the package cannot see it unqualified.
    crate::selection::descendants_any(*type_decl, &["identifier", "type_identifier"])
        .into_iter()
        .map(|n| text(&n, source).to_string())
        .find(|word| siblings.contains(word))
}

/// Drop the modifiers a top-level type may not carry: `static`, because nothing encloses it now,
/// and `private` / `protected`, which are not top-level modifiers at all.
///
/// Read off the **parse of the text being written**, not searched for in it. Searching for the
/// modifiers' source text is what the first version did, and it silently did nothing whenever the
/// text had changed shape on the way here — an annotation on its own line above the declaration was
/// enough — leaving `private class Line` in a file of its own: `modifier private not allowed here`,
/// five times over.
fn strip_nesting_modifiers(body: &str) -> String {
    const FORBIDDEN: &[&str] = &["static", "private", "protected"];
    let Some(tree) = bennu_java::prelude::parse_java(body) else { return body.to_string() };
    let Some(type_decl) = types_in(tree.root_node()).into_iter().next() else {
        return body.to_string();
    };
    let mut cursor = type_decl.walk();
    let Some(modifiers) = type_decl.children(&mut cursor).find(|c| c.kind() == "modifiers") else {
        return body.to_string();
    };
    let mut inner = modifiers.walk();
    let mut cuts: Vec<(usize, usize)> = modifiers
        .children(&mut inner)
        .filter(|c| FORBIDDEN.contains(&c.kind()))
        .map(|c| (c.start_byte(), c.end_byte()))
        .collect();
    cuts.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = body.to_string();
    for (start, end) in cuts {
        let end = if out[end..].starts_with(' ') { end + 1 } else { end };
        out.replace_range(start..end, "");
    }
    out.trim_start_matches(' ').to_string()
}

/// The `import` lines whose type the moved type actually mentions — the same rule
/// [`crate::move_member`] uses, and the same reason for erring towards keeping one.
fn imports_the_type_reads(root: Node<'_>, type_decl: &Node<'_>, source: &str) -> Vec<String> {
    let words: Vec<&str> =
        crate::selection::identifiers(*type_decl).iter().map(|n| text(n, source)).collect();
    let types: Vec<&str> = crate::selection::descendants_any(*type_decl, &["type_identifier"])
        .iter()
        .map(|n| text(n, source))
        .collect();
    imports_of(root, source)
        .into_iter()
        .filter(|(what, _)| {
            let simple = what.rsplit('.').next().unwrap_or(what);
            simple == "*" || words.contains(&simple) || types.contains(&simple)
        })
        .map(|(_, line)| line)
        .collect()
}

/// The type declaration around this one, when there is one.
fn enclosing_type_of<'t>(type_decl: &Node<'t>) -> Option<Node<'t>> {
    enclosing_type(type_decl.parent()?)
}

/// The type declaration the caret is on — through its own header, never from inside its body.
fn type_at<'t>(root: Node<'t>, start: usize, end: usize) -> Option<Node<'t>> {
    let at = crate::selection::node_covering(root, start, end)?;
    let decl = enclosing(at, TYPE_DECLS)?;
    let head_end = decl.child_by_field_name("body").map(|b| b.start_byte()).unwrap_or(decl.end_byte());
    (start >= decl.start_byte() && start <= head_end).then_some(decl)
}

/// Whether this file declares a type of that name — the check a caller repeats against the
/// filesystem, and the one this crate can make on its own.
pub fn declares_type(root: Node<'_>, source: &str, name: &str) -> bool {
    type_named(root, source, name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn outcome(source: &str, needle: &str) -> Outcome {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap();
        move_class(tree.root_node(), source, at, at)
    }

    fn refusal(source: &str, needle: &str) -> String {
        match outcome(source, needle) {
            Some(Err(r)) => r.reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// The plan, with **both** halves insisted on as Java: the file that is written and the file
    /// that is left. A move class is the one refactoring here that produces two files, and a
    /// removal that took a brace with it would leave the second one broken in silence.
    fn both_parse(source: &str, needle: &str) -> (String, String) {
        let Some(Ok(plan)) = outcome(source, needle) else { panic!("expected a plan") };
        let created = plan.new_source.clone().expect("a file").text;
        let left = plan.apply(source);
        for (what, text) in [("the new file", &created), ("what is left", &left)] {
            assert!(
                parse_java(text).is_some_and(|t| !t.root_node().has_error()),
                "{what} does not parse:\n{text}"
            );
        }
        (created, left)
    }

    /// Only the imports the type actually reads travel with it — an unused one in a new file is
    /// noise, and a missing one is a file that does not compile.
    #[test]
    fn only_the_imports_the_type_reads_travel() {
        let src = "package p;\n\nimport java.util.List;\nimport java.util.Map;\n\nclass Order {\n    Map<String, String> lookup;\n\n    static class Line {\n        List<String> parts;\n    }\n}\n";
        let (created, _) = both_parse(src, "static class Line");
        assert!(created.contains("import java.util.List;"), "{created}");
        assert!(!created.contains("import java.util.Map;"), "{created}");
    }

    /// A file with no package declaration produces one with none either.
    #[test]
    fn a_file_without_a_package_makes_one_without_a_package() {
        let src = "class Order {\n    static class Line {\n        int qty;\n    }\n}\n";
        let (created, _) = both_parse(src, "static class Line");
        assert!(!created.contains("package"), "{created}");
        assert!(created.starts_with("class Line"), "{created}");
    }

    /// `final` is a modifier a top-level type may carry, so it stays. Only the three that cannot
    /// go.
    #[test]
    fn a_modifier_a_top_level_type_may_keep_is_kept() {
        let src = "class Order {\n    private static final class Line {\n        int qty;\n    }\n}\n";
        let (created, _) = both_parse(src, "private static final class Line");
        assert!(created.starts_with("final class Line"), "{created}");
    }

    /// A `public` nested type stays public — and Java then requires the file to be named after it,
    /// which is exactly what the caller writes.
    #[test]
    fn a_public_nested_type_stays_public() {
        let src = "package p;\n\nclass Order {\n    public static class Line {\n        int qty;\n    }\n}\n";
        let (created, _) = both_parse(src, "public static class Line");
        assert!(created.contains("public class Line"), "{created}");
    }

    /// A record nests implicitly static and travels with its header intact.
    #[test]
    fn a_nested_record_moves_whole() {
        let src = "class Order {\n    record Line(int qty, String sku) {\n    }\n}\n";
        let (created, left) = both_parse(src, "record Line");
        assert!(created.contains("record Line(int qty, String sku)"), "{created}");
        assert!(!left.contains("record Line"), "{left}");
    }

    /// The javadoc above a nested type is about the type, and the type is leaving.
    #[test]
    fn what_is_left_behind_is_still_a_class() {
        let src = "package p;\n\nclass Order {\n    int total;\n\n    static class Line {\n        int qty;\n    }\n\n    int total() {\n        return total;\n    }\n}\n";
        let (_, left) = both_parse(src, "static class Line");
        assert!(left.contains("int total()"), "{left}");
        assert!(!left.contains("class Line"), "{left}");
    }

    /// A nested type inside a nested type is still a move — its own file, same package.
    #[test]
    fn a_doubly_nested_type_moves_to_the_package() {
        let src = "package p;\n\nclass Order {\n    static class Line {\n        static class Part {\n            int qty;\n        }\n    }\n}\n";
        let (created, _) = both_parse(src, "static class Part");
        assert!(created.contains("class Part {"), "{created}");
        assert!(created.starts_with("package p;"), "{created}");
    }

    #[test]
    fn a_static_nested_class_gets_its_own_file() {
        let src = "package p;\n\nimport java.util.List;\n\nclass Order {\n    static class Line {\n        List<String> parts;\n    }\n}\n";
        let Some(Ok(plan)) = outcome(src, "static class Line") else { panic!("expected a plan") };
        let created = plan.new_source.as_ref().expect("a file");
        assert_eq!(created.name, "Line");
        assert!(created.text.starts_with("package p;\n"), "{}", created.text);
        assert!(created.text.contains("import java.util.List;"), "{}", created.text);
        assert!(created.text.contains("class Line {\n    List<String> parts;\n}"), "{}", created.text);
        // `static` is gone: nothing encloses it any more.
        assert!(!created.text.contains("static class"), "{}", created.text);
        // …and it is out of the file it came from.
        assert_eq!(plan.apply(src).matches("class Line").count(), 0);
    }

    #[test]
    fn an_inner_class_keeps_its_enclosing_instance_and_stays() {
        let src = "class Order {\n    class Line {\n    }\n}\n";
        assert!(refusal(src, "class Line").contains("inner class"));
    }

    #[test]
    fn a_nested_type_that_reads_the_outer_class_stays() {
        let src = "class Order {\n    static int total;\n    static class Line {\n        int f() {\n            return total;\n        }\n    }\n}\n";
        assert!(refusal(src, "static class Line").contains("reads `total`"));
    }

    /// A nested interface or enum is implicitly static and has nothing to lose.
    #[test]
    fn a_nested_enum_moves_without_a_static_keyword() {
        let src = "class Order {\n    enum Kind {\n        BIG, SMALL\n    }\n}\n";
        let Some(Ok(plan)) = outcome(src, "enum Kind") else { panic!("expected a plan") };
        assert!(plan.new_source.unwrap().text.contains("enum Kind {"));
    }

    /// A second top-level type in a file is a move too — and the only one there is not.
    #[test]
    fn a_second_top_level_type_moves_and_a_lone_one_does_not() {
        let src = "class A {\n}\n\nclass B {\n}\n";
        let Some(Ok(plan)) = outcome(src, "class B") else { panic!("expected a plan") };
        assert_eq!(plan.new_source.unwrap().name, "B");
        assert!(outcome("class A {\n}\n", "class A").is_none());
    }

    #[test]
    fn a_private_nested_class_loses_a_modifier_it_may_not_keep() {
        let src = "class Order {\n    private static class Line {\n    }\n}\n";
        let Some(Ok(plan)) = outcome(src, "private static class Line") else {
            panic!("expected a plan")
        };
        let text = plan.new_source.unwrap().text;
        assert!(text.starts_with("class Line"), "{text}");
    }

    /// A private member of a nested type is visible to the class around it, and to nothing else.
    #[test]
    fn a_private_member_read_from_outside_keeps_the_type_nested() {
        let src = "class Order {\n    static class Line {\n        private static int qty;\n    }\n\n    int total() {\n        return Line.qty;\n    }\n}\n";
        assert!(refusal(src, "static class Line").contains("private to `Line`"));
    }

    /// An annotation above the declaration used to make the modifier stripping miss entirely.
    #[test]
    fn an_annotated_nested_type_still_loses_its_modifiers() {
        let src = "class Order {\n    @Deprecated\n    private static class Line {\n    }\n}\n";
        let Some(Ok(plan)) = outcome(src, "@Deprecated") else { panic!("expected a plan") };
        let text = plan.new_source.unwrap().text;
        assert!(text.contains("@Deprecated\nclass Line"), "{text}");
        assert!(!text.contains("private"), "{text}");
    }

    /// A sibling nested type stays nested, so the bare name it was written with resolves to
    /// nothing once the class that used it is top-level.
    #[test]
    fn a_type_that_names_a_sibling_nested_type_stays() {
        let src = "class Order {\n    interface Part {\n    }\n\n    static class Line implements Part {\n    }\n}\n";
        assert!(refusal(src, "static class Line").contains("reads `Part`"));
    }

    /// `Outer.Inner` stops meaning anything once `Inner` is top-level.
    #[test]
    fn a_qualified_mention_of_the_nested_type_is_refused() {
        let src = "class Order {\n    static class Line {\n    }\n\n    Order.Line make() {\n        return null;\n    }\n}\n";
        assert!(refusal(src, "static class Line").contains("means nothing"));
    }

    /// Standing in the body is not standing on the declaration.
    #[test]
    fn a_caret_inside_the_body_offers_nothing() {
        let src = "class Order {\n    static class Line {\n        int qty;\n    }\n}\n";
        let tree = parse_java(src).unwrap();
        let at = src.find("int qty").unwrap();
        assert!(move_class(tree.root_node(), src, at, at).is_none());
    }
}
