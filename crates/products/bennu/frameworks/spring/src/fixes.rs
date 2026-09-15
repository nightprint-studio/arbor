//! Alt+Enter on the declaration checks — each repair is a modifier added, removed or swapped, or an
//! inert annotation deleted.
//!
//! Every diagnostic these answer is anchored on an **annotation**, so the fix finds that annotation
//! at exactly the problem's span and edits the declaration it sits on. A span that no longer holds
//! an annotation — the buffer moved on a keystroke after the squiggle was drawn — offers nothing,
//! and so does a declaration that no longer carries the modifier a fix would remove.
//!
//! What is deliberately not offered: removing `static` from an injected member (its static callers
//! stop compiling), and anything about an `@Async` return type or a private constructor, where the
//! right change is a decision rather than an edit.

use bennu_ext::prelude::{ExtEdit, ExtIntention, ExtProblem};
use bennu_java::prelude::{named_child_of, node_text, parse_java};
use tree_sitter::Node;

use crate::bean_check::{
    CODE_BEAN_NOT_OVERRIDABLE, CODE_CONFIGURATION_FINAL, CODE_INNER_CLASS, CODE_NOT_INSTANTIABLE,
    CODE_POST_PROCESSOR_NOT_STATIC,
};
use crate::proxy::{CODE_FINAL, CODE_NOT_PUBLIC};

pub const INTENTION_REMOVE_MODIFIERS: &str = "spring.bean.remove-modifiers";
pub const INTENTION_MAKE_METHOD_STATIC: &str = "spring.bean.make-static";
pub const INTENTION_MAKE_CLASS_STATIC: &str = "spring.bean.make-class-static";
pub const INTENTION_REMOVE_STEREOTYPE: &str = "spring.bean.remove-stereotype";
pub const INTENTION_CONFIGURATION_REMOVE_FINAL: &str = "spring.configuration.remove-final";
pub const INTENTION_PROXY_REMOVE_FINAL: &str = "spring.proxy.remove-final";
pub const INTENTION_PROXY_MAKE_PUBLIC: &str = "spring.proxy.make-public";

const FIXABLE: &[&str] = &[
    CODE_BEAN_NOT_OVERRIDABLE,
    CODE_POST_PROCESSOR_NOT_STATIC,
    CODE_INNER_CLASS,
    CODE_NOT_INSTANTIABLE,
    CODE_CONFIGURATION_FINAL,
    CODE_FINAL,
    CODE_NOT_PUBLIC,
];

/// The repairs for the problems under the caret that this crate knows how to fix.
pub fn intentions(source: &str, problems: &[ExtProblem]) -> Vec<ExtIntention> {
    let ours: Vec<&ExtProblem> =
        problems.iter().filter(|p| FIXABLE.contains(&p.code.as_str())).collect();
    if ours.is_empty() {
        return Vec::new();
    }
    let Some(tree) = parse_java(source) else { return Vec::new() };
    ours.into_iter()
        .filter_map(|p| {
            let annotation = annotation_at(tree.root_node(), p.start, p.end)?;
            let declaration = annotation.parent().filter(|m| m.kind() == "modifiers")?.parent()?;
            fix(&p.code, annotation, declaration, source)
        })
        .collect()
}

fn fix(code: &str, annotation: Node<'_>, decl: Node<'_>, source: &str) -> Option<ExtIntention> {
    let name = decl.child_by_field_name("name").map(|n| node_text(&n, source)).unwrap_or_default();
    match code {
        CODE_BEAN_NOT_OVERRIDABLE => {
            let present: Vec<&str> = ["private", "final"]
                .into_iter()
                .filter(|w| modifier_token(decl, source, w).is_some())
                .collect();
            let edits: Vec<ExtEdit> =
                present.iter().filter_map(|w| remove_modifier(decl, source, w)).collect();
            intention(INTENTION_REMOVE_MODIFIERS, format!("Remove `{}`", present.join(" ")), edits)
        }
        CODE_CONFIGURATION_FINAL => intention(
            INTENTION_CONFIGURATION_REMOVE_FINAL,
            format!("Remove `final` from `{name}`"),
            remove_modifier(decl, source, "final").into_iter().collect(),
        ),
        CODE_FINAL => {
            // The method, or — when the method itself is not final — the class that makes it so.
            let owner = if modifier_token(decl, source, "final").is_some() {
                decl
            } else {
                decl.parent()?.parent()?
            };
            let owner_name =
                owner.child_by_field_name("name").map(|n| node_text(&n, source)).unwrap_or_default();
            intention(
                INTENTION_PROXY_REMOVE_FINAL,
                format!("Remove `final` from `{owner_name}`"),
                remove_modifier(owner, source, "final").into_iter().collect(),
            )
        }
        CODE_NOT_PUBLIC => intention(
            INTENTION_PROXY_MAKE_PUBLIC,
            format!("Make `{name}` public"),
            make_public(decl, source).into_iter().collect(),
        ),
        CODE_POST_PROCESSOR_NOT_STATIC => intention(
            INTENTION_MAKE_METHOD_STATIC,
            format!("Make `{name}` static"),
            after_modifiers(decl).map(|at| ExtEdit::insert(at, "static ")).into_iter().collect(),
        ),
        CODE_INNER_CLASS => intention(
            INTENTION_MAKE_CLASS_STATIC,
            format!("Make `{name}` static"),
            child_of_kind(decl, "class").map(|kw| ExtEdit::insert(kw.start_byte(), "static ")).into_iter().collect(),
        ),
        CODE_NOT_INSTANTIABLE => {
            let written = annotation
                .child_by_field_name("name")
                .map(|n| node_text(&n, source))
                .unwrap_or_default();
            intention(
                INTENTION_REMOVE_STEREOTYPE,
                format!("Remove @{written}"),
                vec![delete_annotation(annotation, source)],
            )
        }
        _ => None,
    }
}

/// An offer, or none when there is nothing to write.
fn intention(id: &str, label: String, edits: Vec<ExtEdit>) -> Option<ExtIntention> {
    (!edits.is_empty()).then(|| ExtIntention { id: id.to_string(), label, edits })
}

/// The annotation covering exactly `[start, end)`.
fn annotation_at(root: Node<'_>, start: usize, end: usize) -> Option<Node<'_>> {
    let mut node = root.descendant_for_byte_range(start, end)?;
    loop {
        if node.start_byte() != start || node.end_byte() != end {
            return None;
        }
        if matches!(node.kind(), "annotation" | "marker_annotation") {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// The keyword token `word` among a declaration's modifiers — a token, never text, so `"final"` in
/// an annotation argument is not it.
fn modifier_token<'t>(decl: Node<'t>, source: &str, word: &str) -> Option<Node<'t>> {
    let modifiers = named_child_of(decl, "modifiers")?;
    let mut cursor = modifiers.walk();
    let found = modifiers.children(&mut cursor).find(|c| {
        !matches!(c.kind(), "annotation" | "marker_annotation") && node_text(c, source) == word
    });
    found
}

/// Delete a modifier and the space after it (or, at the end of a line, the space before it).
fn remove_modifier(decl: Node<'_>, source: &str, word: &str) -> Option<ExtEdit> {
    let token = modifier_token(decl, source, word)?;
    let (mut start, mut end) = (token.start_byte(), token.end_byte());
    let trailing = source[end..].bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
    if trailing > 0 {
        end += trailing;
    } else {
        start -= source[..start].bytes().rev().take_while(|b| matches!(b, b' ' | b'\t')).count();
    }
    Some(ExtEdit::replace(start, end, ""))
}

/// `private` / `protected` becomes `public`; a package-private method gains it in front of its
/// first keyword modifier, or of its type.
fn make_public(method: Node<'_>, source: &str) -> Option<ExtEdit> {
    if let Some(token) = ["private", "protected"].iter().find_map(|w| modifier_token(method, source, w)) {
        return Some(ExtEdit::replace(token.start_byte(), token.end_byte(), "public"));
    }
    let first_keyword = named_child_of(method, "modifiers").and_then(|modifiers| {
        let mut cursor = modifiers.walk();
        let found = modifiers
            .children(&mut cursor)
            .find(|c| !matches!(c.kind(), "annotation" | "marker_annotation"))
            .map(|c| c.start_byte());
        found
    });
    let at = first_keyword.or_else(|| after_modifiers(method))?;
    Some(ExtEdit::insert(at, "public "))
}

/// Where a method's modifiers end and its signature begins: its type parameters, else its type.
fn after_modifiers(method: Node<'_>) -> Option<usize> {
    named_child_of(method, "type_parameters")
        .or_else(|| method.child_by_field_name("type"))
        .map(|n| n.start_byte())
}

fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| c.kind() == kind);
    found
}

/// The annotation's whole line when it stands alone there, otherwise just it and its trailing space.
fn delete_annotation(annotation: Node<'_>, source: &str) -> ExtEdit {
    let (start, end) = (annotation.start_byte(), annotation.end_byte());
    let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = source[end..].find('\n').map(|i| end + i + 1).unwrap_or(source.len());
    if source[line_start..start].trim().is_empty() && source[end..line_end].trim().is_empty() {
        return ExtEdit::replace(line_start, line_end, "");
    }
    let trailing = source[end..].bytes().take_while(|b| matches!(b, b' ' | b'\t')).count();
    ExtEdit::replace(start, end + trailing, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bean_check::test_support::issues;
    use crate::proxy::{issues_in as proxy_issues, ProxyMode};

    /// The source after the one offer for the first problem of `code`.
    fn fixed(source: &str, problems: Vec<ExtProblem>, code: &str) -> (String, String) {
        let problems: Vec<ExtProblem> = problems.into_iter().filter(|p| p.code == code).collect();
        assert!(!problems.is_empty(), "no `{code}` problem to fix");
        let offers = intentions(source, &problems[..1]);
        assert_eq!(offers.len(), 1, "{offers:?}");
        let mut edits = offers[0].edits.clone();
        edits.sort_by_key(|e| std::cmp::Reverse(e.start));
        let mut out = source.to_string();
        for e in edits {
            out.replace_range(e.start..e.end, &e.text);
        }
        (out, offers[0].label.clone())
    }

    fn bean_problems(body: &str) -> (String, Vec<ExtProblem>) {
        let (source, found) = issues(body);
        let problems = found
            .into_iter()
            .map(|i| ExtProblem { code: i.code.to_string(), start: i.start, end: i.end })
            .collect();
        (source, problems)
    }

    #[test]
    fn a_private_final_bean_method_loses_both_modifiers() {
        let (src, problems) =
            bean_problems("@Configuration class C { @Bean private final Object d() { return null; } }");
        let (out, label) = fixed(&src, problems, CODE_BEAN_NOT_OVERRIDABLE);
        assert!(out.contains("@Bean Object d()"), "{out}");
        assert_eq!(label, "Remove `private final`");
    }

    #[test]
    fn a_final_configuration_loses_final() {
        let (src, problems) =
            bean_problems("@Configuration public final class C { @Bean Object d() { return null; } }");
        let (out, _) = fixed(&src, problems, CODE_CONFIGURATION_FINAL);
        assert!(out.contains("@Configuration public class C"), "{out}");
    }

    #[test]
    fn a_post_processor_bean_becomes_static_after_its_other_modifiers() {
        let (src, problems) = bean_problems(
            "@Configuration class C { @Bean public PropertySourcesPlaceholderConfigurer p() { return null; } }",
        );
        let (out, label) = fixed(&src, problems, CODE_POST_PROCESSOR_NOT_STATIC);
        assert!(out.contains("@Bean public static PropertySourcesPlaceholderConfigurer p()"), "{out}");
        assert_eq!(label, "Make `p` static");
    }

    #[test]
    fn an_inner_component_becomes_static() {
        let (src, problems) = bean_problems("class Outer { @Component public class Inner { } }");
        let (out, _) = fixed(&src, problems, CODE_INNER_CLASS);
        assert!(out.contains("@Component public static class Inner"), "{out}");
    }

    #[test]
    fn a_stereotype_alone_on_its_line_takes_the_line() {
        let (src, problems) = bean_problems("@Service\npublic interface Orders { }\n");
        let (out, label) = fixed(&src, problems, CODE_NOT_INSTANTIABLE);
        assert!(out.ends_with("\npublic interface Orders { }\n"), "{out}");
        assert!(!out.contains("@Service"));
        assert_eq!(label, "Remove @Service");
    }

    #[test]
    fn a_non_public_advised_method_becomes_public_from_private_or_package() {
        let src = "class A {\n@Transactional private void a() { }\n@Transactional static void b() { }\n}";
        let problems: Vec<ExtProblem> = proxy_issues(src, ProxyMode::Proxy)
            .into_iter()
            .map(|i| ExtProblem { code: i.code.to_string(), start: i.start, end: i.end })
            .collect();
        let offers = intentions(src, &problems);
        assert_eq!(offers.len(), 2, "{offers:?}");
        let (out, _) = fixed(src, problems.clone(), CODE_NOT_PUBLIC);
        assert!(out.contains("@Transactional public void a()"), "{out}");
        let (out, _) = fixed(src, problems[1..].to_vec(), CODE_NOT_PUBLIC);
        assert!(out.contains("@Transactional public static void b()"), "{out}");
    }

    #[test]
    fn a_final_class_makes_the_proxy_fix_target_the_class() {
        let src = "final class A { @Async public void go() { } }";
        let problems: Vec<ExtProblem> = proxy_issues(src, ProxyMode::Proxy)
            .into_iter()
            .map(|i| ExtProblem { code: i.code.to_string(), start: i.start, end: i.end })
            .collect();
        let (out, label) = fixed(src, problems, CODE_FINAL);
        assert_eq!(out, "class A { @Async public void go() { } }");
        assert_eq!(label, "Remove `final` from `A`");
    }

    /// A squiggle that has outlived its text repairs nothing.
    #[test]
    fn a_span_that_no_longer_holds_the_annotation_offers_nothing() {
        let src = "@Configuration class C { @Bean private Object d() { return null; } }";
        let stale = ExtProblem { code: CODE_BEAN_NOT_OVERRIDABLE.to_string(), start: 0, end: 3 };
        assert!(intentions(src, &[stale]).is_empty());
    }

    #[test]
    fn a_problem_this_crate_does_not_fix_is_not_even_parsed_for() {
        let other = ExtProblem { code: "spring.async.discarded-result".to_string(), start: 0, end: 1 };
        assert!(intentions("class A { }", &[other]).is_empty());
    }
}
