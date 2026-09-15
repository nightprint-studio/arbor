//! Bean declarations Spring refuses at startup, or silently ignores.
//!
//! ## The defect
//!
//! Most of what goes wrong in a Spring context goes wrong at **startup**, a build plus a deploy plus
//! a wait away from the line that caused it — or, worse, does not go wrong at all:
//!
//! ```java
//! @Configuration
//! public class DataConfig {
//!     @Bean
//!     private DataSource dataSource() { … }    // "@Bean method 'dataSource' must not be private or final"
//!
//!     @Autowired
//!     private static Clock clock;               // never injected: stays null, Spring only logs at INFO
//! }
//! ```
//!
//! Every check here is decidable from one file: the modifiers of a declaration, the kind of type it
//! sits in, the type it returns. None needs the bean model, so they answer before an index exists.
//!
//! ## Identified by origin
//!
//! `@Bean` is not a reserved word, and neither is `@Async` — every annotation is resolved through the
//! file's imports ([`crate::known`]), so a project's own annotation of the same name is never judged
//! by Spring's rules. The one meta-annotation followed is the fixed set that *is* `@Configuration`
//! by definition (`@SpringBootApplication`, `@SpringBootConfiguration`, `@TestConfiguration`);
//! a project's own meta-annotated one is missed, which loses a report rather than inventing one.
//!
//! ## What is left alone
//!
//! Under-report rather than risk a false positive (docs §7): a `proxyBeanMethods` that is not a
//! literal, a `@Repository` interface (a Spring Data repository is a bean, just not a scanned one),
//! an interface carrying any other annotation (a Feign client, a MyBatis mapper), an abstract class
//! with a `@Lookup` method (which Spring does instantiate).

mod async_result;
mod configuration;
mod injection;
mod stereotype;

use bennu_facts::prelude::{mentions_any, scan_java, AnnFacts, JavaFacts};
use bennu_java::prelude::{collect_annotations, node_text, parse_java, type_declarations};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// `@Bean` method that is `private` or `final` in a proxied `@Configuration`.
pub const CODE_BEAN_NOT_OVERRIDABLE: &str = "spring.bean.not-overridable";
/// `@Bean` method declared `void`.
pub const CODE_BEAN_VOID: &str = "spring.bean.void";
/// Non-static `@Bean` method returning a `BeanFactoryPostProcessor`.
pub const CODE_POST_PROCESSOR_NOT_STATIC: &str = "spring.bean.post-processor-not-static";
/// Stereotype on an interface or abstract class — component scanning registers nothing.
pub const CODE_NOT_INSTANTIABLE: &str = "spring.bean.not-instantiable";
/// Stereotype on a non-static inner class — component scanning registers nothing.
pub const CODE_INNER_CLASS: &str = "spring.bean.inner-class";
/// A proxied `@Configuration` class declared `final`.
pub const CODE_CONFIGURATION_FINAL: &str = "spring.configuration.final";
/// A proxied `@Configuration` class whose constructors are all private.
pub const CODE_CONFIGURATION_NO_VISIBLE_CONSTRUCTOR: &str =
    "spring.configuration.no-visible-constructor";
/// `@Autowired` / `@Inject` / `@Value` / `@Resource` on a static member.
pub const CODE_STATIC_INJECTION: &str = "spring.injection.static";
/// `@Async` method whose return type cannot carry a result back.
pub const CODE_ASYNC_DISCARDED_RESULT: &str = "spring.async.discarded-result";

/// What goes wrong, in the shape a diagnostic needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeanIssue {
    /// Namespaced diagnostic code.
    pub code: &'static str,
    pub message: String,
    /// `error` for what stops the context starting, `warning` for what Spring silently ignores.
    pub severity: &'static str,
    pub start: usize,
    pub end: usize,
}

impl From<BeanIssue> for Diagnostic {
    fn from(issue: BeanIssue) -> Self {
        Diagnostic {
            message: issue.message,
            severity: issue.severity.to_string(),
            code: issue.code.to_string(),
            start: issue.start,
            end: issue.end,
        }
    }
}

/// The cheap reject before any parse. Every annotation judged here resolves through an import or a
/// qualified name, and each of those spells one of these.
const MARKERS: &[&str] = &["springframework", "inject", "annotation"];

/// Read a source for the bean declarations Spring refuses or ignores.
pub fn issues_in(path: &str, source: &str) -> Vec<BeanIssue> {
    if !mentions_any(source, MARKERS) {
        return Vec::new();
    }
    let Some(facts) = scan_java(path, source) else { return Vec::new() };
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let origins = Origins { facts: &facts, source };
    let mut out = Vec::new();
    for type_decl in type_declarations(tree.root_node()) {
        configuration::check(type_decl, &origins, &mut out);
        injection::check(type_decl, &origins, &mut out);
        stereotype::check(type_decl, &origins, &mut out);
        async_result::check(type_decl, &origins, &mut out);
    }
    out.sort_by_key(|i| i.start);
    out
}

/// Which annotation a node really is — the file's imports, read once, behind the crate's table.
pub(crate) struct Origins<'f> {
    facts: &'f JavaFacts,
    source: &'f str,
}

impl<'f> Origins<'f> {
    /// The first annotation on `decl` that resolves to one of `names`, with the name it matched.
    fn first_of<'n>(&self, decl: Node<'_>, names: &[&'n str]) -> Option<(&'n str, AnnFacts)> {
        collect_annotations(&decl, self.source.as_bytes()).into_iter().find_map(|ann| {
            let name = crate::known::is_any(&ann, self.facts, names)?;
            Some((name, ann))
        })
    }

    fn text(&self, node: Node<'_>) -> &'f str {
        node_text(&node, self.source)
    }
}

/// The member declarations of a type's body.
fn members(type_decl: Node<'_>) -> Vec<Node<'_>> {
    let Some(body) = type_decl.child_by_field_name("body") else { return Vec::new() };
    let mut cursor = body.walk();
    let found = body.named_children(&mut cursor).collect();
    found
}

fn methods(type_decl: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    members(type_decl).into_iter().filter(|m| m.kind() == "method_declaration")
}

/// A method's name as written.
fn method_name<'f>(method: Node<'_>, origins: &Origins<'f>) -> &'f str {
    method.child_by_field_name("name").map(|n| origins.text(n)).unwrap_or_default()
}

/// The written return type without its type arguments, and its last segment
/// (`java.util.List<Order>` → `List`). An array keeps its brackets, so it never reads as its element.
fn simple_type(written: &str) -> &str {
    let raw = written.split_once('<').map(|(head, _)| head).unwrap_or(written).trim();
    raw.rsplit('.').next().unwrap_or(raw)
}

fn issue(
    code: &'static str,
    severity: &'static str,
    message: String,
    (start, end): (usize, usize),
) -> BeanIssue {
    BeanIssue { code, message, severity, start, end }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::{issues_in, BeanIssue};

    /// Every Spring annotation package the fixtures use, on one line — an annotation with no import
    /// is the project's own, and the checks rightly say nothing about it.
    pub const IMPORTS: &str = "import org.springframework.context.annotation.*; \
         import org.springframework.stereotype.*; \
         import org.springframework.beans.factory.annotation.*; \
         import org.springframework.scheduling.annotation.*; \
         import org.springframework.boot.autoconfigure.SpringBootApplication; \
         import org.springframework.web.bind.annotation.*; \
         import javax.inject.Inject; import javax.annotation.Resource;\n";

    /// The issues in `body`, with the imports prepended.
    pub fn issues(body: &str) -> (String, Vec<BeanIssue>) {
        let source = format!("package com.acme;\n{IMPORTS}{body}");
        let found = issues_in("/p/src/main/java/com/acme/T.java", &source);
        (source, found)
    }

    /// The issues of one code.
    pub fn with_code(body: &str, code: &str) -> (String, Vec<BeanIssue>) {
        let (source, found) = issues(body);
        (source, found.into_iter().filter(|i| i.code == code).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::IMPORTS;
    use super::*;

    /// A project's own `@Bean` is not Spring's — the reason every check resolves through the imports.
    #[test]
    fn an_annotation_without_a_spring_import_is_not_judged() {
        let src = "package com.acme;\nimport com.acme.annotations.Bean;\n\
                   @org.springframework.context.annotation.Configuration\n\
                   class C { @Bean private Object x() { return null; } }";
        assert!(issues_in("/p/C.java", src).is_empty(), "{:?}", issues_in("/p/C.java", src));
    }

    #[test]
    fn a_qualified_annotation_is_resolved_without_an_import() {
        let src = "package com.acme;\nclass C {\n\
                   @org.springframework.beans.factory.annotation.Autowired static Object x;\n}";
        let found = issues_in("/p/C.java", src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, CODE_STATIC_INJECTION);
    }

    #[test]
    fn a_file_naming_nothing_spring_is_not_even_parsed() {
        assert!(issues_in("/p/C.java", "class C { static Object x; }").is_empty());
    }

    #[test]
    fn issues_become_diagnostics_unchanged() {
        let src = format!("package com.acme;\n{IMPORTS}class C {{ @Bean void x() {{ }} }}");
        let found = issues_in("/p/C.java", &src);
        let d = Diagnostic::from(found[0].clone());
        assert_eq!(d.code, CODE_BEAN_VOID);
        assert_eq!(d.severity, "error");
        assert_eq!((d.start, d.end), (found[0].start, found[0].end));
    }

    #[test]
    fn a_simple_type_drops_generics_and_package_but_keeps_array_brackets() {
        assert_eq!(simple_type("java.util.List<Order>"), "List");
        assert_eq!(simple_type("String[]"), "String[]");
        assert_eq!(simple_type("void"), "void");
    }
}
