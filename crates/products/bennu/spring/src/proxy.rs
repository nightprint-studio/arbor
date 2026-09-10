//! When a Spring annotation on a method does **nothing**.
//!
//! ## The defect
//!
//! `@Transactional`, `@Async`, `@Cacheable` and their relatives are not implemented by the method
//! they are written on. They are implemented by a **proxy** that Spring puts in front of the bean,
//! and everything that reaches the bean without going through that proxy gets the bare method.
//!
//! ```java
//! public void importaTutto() {          // no transaction here
//!     for (Riga r : righe) salva(r);    // ← the proxy is not in this path
//! }
//!
//! @Transactional
//! public void salva(Riga r) { … }       // runs with no transaction at all
//! ```
//!
//! The annotation is there. The bean is a bean. The configuration is right. And the rollback does
//! not happen. What you are told is *"a metà import restano dati sporchi"*, which names a symptom
//! several layers away from a call that looks completely ordinary — and the line that is wrong
//! (`salva(r)`) carries no annotation at all, so nothing draws the eye to it.
//!
//! Three ways to miss the proxy, all silent:
//!
//! 1. **Self-invocation** — the call above. The reference is `this`, not the proxy.
//! 2. **A non-public method.** Spring's proxy AOP ignores the annotation outright.
//! 3. **A `final` method or class.** CGLIB subclasses the bean to proxy it, and cannot override
//!    what is final.
//!
//! ## Why it stays quiet on an AspectJ project
//!
//! With `mode = AdviceMode.ASPECTJ` (or `<tx:annotation-driven mode="aspectj"/>`) the advice is
//! woven into the bytecode rather than wrapped around the bean, and **all three** of the above
//! work correctly. That is rare, but reporting on such a project would be confidently wrong about
//! every annotated method in it — so [`ProxyMode`] is read from the project once, and an AspectJ
//! project silences the whole check (docs §7).

use bennu_java::prelude::{
    annotations_of, modifier_words, node_text as text, parse_java, simple_name,
    type_declarations as all_types,
};
use tree_sitter::Node;

/// How the project weaves its advice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProxyMode {
    /// The default: a proxy around the bean, with everything that follows from it.
    #[default]
    Proxy,
    /// Woven into the bytecode. Self-invocation, non-public and final all work — nothing to report.
    AspectJ,
}

/// The annotations that are implemented by the proxy rather than by the method.
///
/// Simple names, because that is how they are written; the qualified form is vanishingly rare and
/// the last segment is the same either way.
const PROXIED: &[(&str, &str)] = &[
    ("Transactional", "the transaction"),
    ("Async", "the asynchronous dispatch"),
    ("Cacheable", "the cache lookup"),
    ("CachePut", "the cache write"),
    ("CacheEvict", "the cache eviction"),
    ("PreAuthorize", "the authorization check"),
    ("PostAuthorize", "the authorization check"),
    ("Secured", "the authorization check"),
    ("RolesAllowed", "the authorization check"),
    ("Retryable", "the retry"),
    ("Validated", "the argument validation"),
];

fn proxied(name: &str) -> Option<&'static str> {
    let simple = simple_name(name);
    PROXIED.iter().find(|(n, _)| *n == simple).map(|(_, what)| *what)
}

/// What goes wrong, in the shape a diagnostic needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyIssue {
    /// Namespaced diagnostic code.
    pub code: &'static str,
    pub message: String,
    /// `"warning"` throughout: the code compiles and runs, it just does not do what it says.
    pub severity: &'static str,
    pub start: usize,
    pub end: usize,
}

pub const CODE_SELF_INVOCATION: &str = "spring.proxy.self-invocation";
pub const CODE_NOT_PUBLIC: &str = "spring.proxy.not-public";
pub const CODE_FINAL: &str = "spring.proxy.final";

/// Read a source for the three ways a proxied annotation can be inert.
///
/// `mode` is the project's, not the file's — an AspectJ project reports nothing.
pub fn issues_in(source: &str, mode: ProxyMode) -> Vec<ProxyIssue> {
    if mode == ProxyMode::AspectJ {
        return Vec::new();
    }
    // The cheap reject before the parse: most of a legacy tree has none of these words in it.
    if !PROXIED.iter().any(|(name, _)| source.contains(name)) {
        return Vec::new();
    }
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    // Interfaces and enums too: a `default` method on an interface can carry `@Transactional`,
    // and the shared walk is the one place that decides what a type is.
    for type_decl in all_types(tree.root_node()) {
        check_type(type_decl, source, &mut out);
    }
    out.sort_by_key(|i| i.start);
    out
}

/// One annotated method of the class being checked.
struct Advised<'t> {
    name: String,
    /// What the annotation provides, for the message (`"the transaction"`).
    what: &'static str,
    annotation: String,
    node: Node<'t>,
    method: Node<'t>,
}

fn check_type(type_decl: Node<'_>, source: &str, out: &mut Vec<ProxyIssue>) {
    let Some(body) = type_decl.child_by_field_name("body") else { return };
    let class_is_final = modifier_words(type_decl, source).contains(&"final".to_string());

    let mut advised: Vec<Advised<'_>> = Vec::new();
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if member.kind() != "method_declaration" {
            continue;
        }
        let Some(name_node) = member.child_by_field_name("name") else { continue };
        let name = text(&name_node, source).to_string();
        for (annotation, node) in annotations_of(member, source) {
            let Some(what) = proxied(&annotation) else { continue };

            let words = modifier_words(member, source);
            if !words.iter().any(|w| w == "public") {
                out.push(ProxyIssue {
                    code: CODE_NOT_PUBLIC,
                    message: format!(
                        "@{annotation} is ignored on a method that is not public — Spring's \
                         proxy-based AOP only advises public methods, so {what} never happens"
                    ),
                    severity: "warning",
                    start: node.start_byte(),
                    end: node.end_byte(),
                });
            } else if words.iter().any(|w| w == "final") || class_is_final {
                let subject = if class_is_final { "class" } else { "method" };
                out.push(ProxyIssue {
                    code: CODE_FINAL,
                    message: format!(
                        "@{annotation} cannot be applied to a method of a final {subject} — the \
                         proxy subclasses the bean to advise it, and cannot override what is \
                         final, so {what} never happens"
                    ),
                    severity: "warning",
                    start: node.start_byte(),
                    end: node.end_byte(),
                });
            }

            advised.push(Advised {
                name: name.clone(),
                what,
                annotation: annotation.clone(),
                node,
                method: member,
            });
        }
    }
    if advised.is_empty() {
        return;
    }
    for used in self_invocations(body, source, &advised) {
        out.push(used);
    }
    let _ = advised.iter().map(|a| a.node);
}

/// The calls inside this class that reach an advised method without going through the proxy.
fn self_invocations(
    body: Node<'_>,
    source: &str,
    advised: &[Advised<'_>],
) -> Vec<ProxyIssue> {
    let mut out = Vec::new();
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            // A nested type has its own members; a bare call inside it resolves there first, and
            // reading it as the outer class's would be a diagnostic on a method that is not
            // being called at all.
            if matches!(child.kind(), "class_declaration" | "record_declaration") {
                continue;
            }
            if child.kind() == "method_invocation" {
                if let Some(issue) = self_invocation(child, source, advised) {
                    out.push(issue);
                }
            }
            stack.push(child);
        }
    }
    out
}

fn self_invocation(
    call: Node<'_>,
    source: &str,
    advised: &[Advised<'_>],
) -> Option<ProxyIssue> {
    // Only a call with no receiver, or one on `this` — anything else goes through a reference that
    // may well BE the proxy, which is the case that works.
    match call.child_by_field_name("object") {
        None => {}
        Some(object) if text(&object, source) == "this" => {}
        Some(_) => return None,
    }
    let name_node = call.child_by_field_name("name")?;
    let called = text(&name_node, source);
    let target = advised.iter().find(|a| a.name == called)?;

    // Recursion bypasses the proxy too, but reporting it is noise: the first call already went
    // through the proxy, and what a recursive step would want is nearly always what it gets.
    let enclosing = enclosing_method(call)?;
    if enclosing.id() == target.method.id() {
        return None;
    }

    Some(ProxyIssue {
        code: CODE_SELF_INVOCATION,
        message: format!(
            "this call does not go through the proxy, so @{} on `{}` has no effect here — {} is \
             skipped. Move the call behind an injected reference, or move `{}` to another bean",
            target.annotation, target.name, target.what, target.name
        ),
        severity: "warning",
        start: call.start_byte(),
        end: name_node.end_byte(),
    })
}

fn enclosing_method(node: Node<'_>) -> Option<Node<'_>> {
    let mut at = node.parent();
    while let Some(current) = at {
        if current.kind() == "method_declaration" {
            return Some(current);
        }
        at = current.parent();
    }
    None
}

/// Whether a project source or configuration switches Spring to AspectJ weaving.
///
/// Read across the project rather than per file: the mode is set once, in a `@Configuration` or in
/// the XML, and it changes what is true about every other file.
pub fn declares_aspectj(text: &str) -> bool {
    text.contains("AdviceMode.ASPECTJ")
        || text.contains("mode=\"aspectj\"")
        || text.contains("mode='aspectj'")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"package com.acme;

import org.springframework.transaction.annotation.Transactional;

@Service
public class ImportService {

    private final RigaRepository righe;

    public void importaTutto() {
        for (Riga r : righe.tutte()) {
            salva(r);
        }
    }

    @Transactional
    public void salva(Riga r) {
        righe.save(r);
    }

    @Transactional
    void pacchetto(Riga r) { }

    @Async
    public final void nonSovrascrivibile() { }

    public void viaProxy(ImportService altro) {
        altro.salva(null);
    }
}
"#;

    fn issues() -> Vec<ProxyIssue> {
        issues_in(SRC, ProxyMode::Proxy)
    }

    /// The defect this module exists for: the annotation is right, the bean is right, and the
    /// transaction does not happen — reported on the CALL, which is the line that carries no
    /// annotation and therefore draws no eye.
    #[test]
    fn a_self_invocation_is_reported_on_the_call() {
        let found = issues();
        let call = found.iter().find(|i| i.code == CODE_SELF_INVOCATION).expect("the call");
        assert_eq!(&SRC[call.start..call.end], "salva");
        assert!(call.message.contains("@Transactional"), "{}", call.message);
        assert!(call.message.contains("the transaction is skipped"), "{}", call.message);
    }

    /// A call through a reference may well BE the proxy, and that is the case that works.
    #[test]
    fn a_call_through_a_reference_is_not_reported() {
        let found = issues();
        assert_eq!(
            found.iter().filter(|i| i.code == CODE_SELF_INVOCATION).count(),
            1,
            "`altro.salva(...)` is not a self-invocation"
        );
    }

    #[test]
    fn a_non_public_advised_method_is_reported_on_the_annotation() {
        let found = issues();
        let issue = found.iter().find(|i| i.code == CODE_NOT_PUBLIC).expect("the package-private");
        assert!(SRC[issue.start..issue.end].starts_with("@Transactional"));
        assert!(issue.message.contains("only advises public methods"));
    }

    #[test]
    fn a_final_method_cannot_be_proxied() {
        let found = issues();
        let issue = found.iter().find(|i| i.code == CODE_FINAL).expect("the final one");
        assert!(SRC[issue.start..issue.end].starts_with("@Async"));
    }

    /// The whole check goes quiet on a project that weaves rather than proxies — where all three
    /// of these work correctly, and every report would be wrong.
    #[test]
    fn an_aspectj_project_is_told_nothing() {
        assert!(issues_in(SRC, ProxyMode::AspectJ).is_empty());
    }

    #[test]
    fn aspectj_is_recognised_in_both_places_it_is_written() {
        assert!(declares_aspectj("@EnableTransactionManagement(mode = AdviceMode.ASPECTJ)"));
        assert!(declares_aspectj("<tx:annotation-driven mode=\"aspectj\"/>"));
        assert!(!declares_aspectj("@EnableTransactionManagement"));
    }

    /// Recursion misses the proxy as well, but the first call already went through it and the
    /// report would be noise on every recursive walker in the project.
    #[test]
    fn recursion_is_not_reported() {
        let src = "class A { @Transactional public void go(int n) { if (n > 0) go(n - 1); } }";
        let found = issues_in(src, ProxyMode::Proxy);
        assert!(found.iter().all(|i| i.code != CODE_SELF_INVOCATION), "{found:?}");
    }

    /// A bare call inside a nested class resolves to the nested class's own member, and reading it
    /// as the outer's would report a method nobody called.
    #[test]
    fn a_nested_classes_call_is_not_the_outers() {
        let src = r#"class Outer {
            @Transactional public void salva() { }
            class Inner { void salva() { } void go() { salva(); } }
        }"#;
        let found = issues_in(src, ProxyMode::Proxy);
        assert!(found.iter().all(|i| i.code != CODE_SELF_INVOCATION), "{found:?}");
    }

    /// The word `final` inside a string is not a modifier — the reason the modifiers are read as
    /// tokens rather than as text.
    #[test]
    fn a_modifier_word_in_a_string_is_not_a_modifier() {
        let src = r#"class A { @Cacheable(value = "final") public void go() { } }"#;
        assert!(issues_in(src, ProxyMode::Proxy).is_empty());
    }

    #[test]
    fn a_file_with_none_of_these_annotations_is_not_even_parsed() {
        assert!(issues_in("class A { void go() { helper(); } void helper() { } }", ProxyMode::Proxy)
            .is_empty());
    }
}
