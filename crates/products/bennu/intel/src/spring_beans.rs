//! Spring stereotype-bean policy.
//!
//! Legacy Spring apps declare beans two ways: XML `<bean id class>` (parsed by `bennu-web`)
//! AND annotations — `@Component`/`@Service`/`@Repository`/`@Controller` (and the meta-stereotypes
//! `@RestController`/`@Configuration`, plus JSR-330 `@Named`/`@ManagedBean`) on a class. This
//! module reproduces the bean each such stereotype declares — its **name** (the bean id a Struts
//! `<action class=…>` or an `@Autowired` by-name resolves against) and the impl **FQCN** — so the
//! config resolver can consult annotation-declared beans as a fallback when the XML `<bean>`s don't
//! name the id (docs §10 C1).
//!
//! Mirrors [`crate::lombok`]: a bennu-intel policy over bennu-java's generic annotation model
//! (`TypeDecl.annotations`), framework meaning applied here rather than in the language layer.
//! The bean **name** follows Spring's default: the stereotype's explicit `value` if present, else
//! the simple class name run through `Introspector.decapitalize` ([`decapitalize`]).

use std::path::PathBuf;

use bennu_java::prelude::{extract_symbols, TypeDecl};

/// The bean-defining stereotype annotations (simple names). Includes the Spring meta-stereotypes
/// (`@RestController` is `@Controller`, `@Configuration` is `@Component`) and the JSR-330 markers,
/// since each declares a bean the same way.
const STEREOTYPES: &[&str] = &[
    "Component",
    "Service",
    "Repository",
    "Controller",
    "RestController",
    "Configuration",
    "Named",
    "ManagedBean",
];

/// An annotation-declared Spring bean: the bean id/name it registers under, the implementation
/// class FQCN, and the `.java` it was declared in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationBean {
    /// The bean id/name — the explicit stereotype `value`, else the decapitalized simple name.
    pub name: String,
    /// The fully-qualified name of the annotated class (`TypeDecl.fqn`).
    pub fqcn: String,
    /// The declaring source file (forward-slashed by the collector).
    pub source_file: String,
}

/// The [`AnnotationBean`] a type declares if it carries any bean-defining stereotype, else `None`.
/// The bean **name** = the stereotype's explicit string `value` (`@Service("foo")` → `foo`) if
/// present, else the class simple name decapitalized ([`decapitalize`]). `fqcn` = `type_decl.fqn`.
pub fn stereotype_bean(type_decl: &TypeDecl, source_file: &str) -> Option<AnnotationBean> {
    let stereotype = type_decl
        .annotations
        .iter()
        .find(|a| STEREOTYPES.contains(&a.name.as_str()))?;

    let name = stereotype
        .value
        .clone()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| decapitalize(&type_decl.name));

    Some(AnnotationBean { name, fqcn: type_decl.fqn.clone(), source_file: source_file.to_string() })
}

/// Scan `sources` for annotation-declared Spring beans — the project-wide collector the index
/// build calls. Each source is (path, text); every top-level and nested type carrying a stereotype
/// contributes one bean, with `source_file` the forward-slashed path.
pub fn collect_annotation_beans(sources: &[(PathBuf, String)]) -> Vec<AnnotationBean> {
    let mut beans = Vec::new();
    for (path, src) in sources {
        let file = path.to_string_lossy().replace('\\', "/");
        for td in extract_symbols(src).types {
            if let Some(bean) = stereotype_bean(&td, &file) {
                beans.push(bean);
            }
        }
    }
    beans
}

/// Spring's `Introspector.decapitalize`: lowercase the first character, UNLESS the first TWO
/// characters are BOTH upper-case (`URLResolver` stays `URLResolver`, `FooService` → `fooService`).
/// An empty string stays empty.
fn decapitalize(name: &str) -> String {
    let bytes = name.as_bytes();
    // Two leading upper-case ASCII letters → the JavaBeans rule leaves the name untouched.
    if bytes.len() >= 2 && bytes[0].is_ascii_uppercase() && bytes[1].is_ascii_uppercase() {
        return name.to_string();
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::Annotation;

    /// A type with the given annotations (name only, no value) and fqn.
    fn typed(name: &str, fqn: &str, annotations: Vec<Annotation>) -> TypeDecl {
        TypeDecl {
            span: None, // built by hand, not read from a file
            name: name.to_string(),
            fqn: fqn.to_string(),
            kind: bennu_java::prelude::TypeKind::Class,
            is_abstract: false,
            is_final: false,
            is_sealed: false,
            type_params: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            extends: None,
            implements: Vec::new(),
            annotations,
        }
    }

    fn marker(name: &str) -> Annotation {
        Annotation { name: name.to_string(), value: None, args: Vec::new(), positional: None }
    }

    #[test]
    fn bare_service_decapitalizes_simple_name() {
        let td = typed("FooService", "com.x.FooService", vec![marker("Service")]);
        let bean = stereotype_bean(&td, "com/x/FooService.java").unwrap();
        assert_eq!(bean.name, "fooService");
        assert_eq!(bean.fqcn, "com.x.FooService");
        assert_eq!(bean.source_file, "com/x/FooService.java");
    }

    #[test]
    fn explicit_value_wins() {
        let td = typed(
            "FooService",
            "com.x.FooService",
            vec![Annotation { name: "Service".into(), value: Some("custom".into()), args: Vec::new(), positional: None }],
        );
        assert_eq!(stereotype_bean(&td, "f.java").unwrap().name, "custom");
    }

    #[test]
    fn two_leading_caps_are_left_untouched() {
        let td = typed("URLDao", "com.x.URLDao", vec![marker("Repository")]);
        assert_eq!(stereotype_bean(&td, "f.java").unwrap().name, "URLDao");
    }

    #[test]
    fn component_decapitalizes() {
        let td = typed("Foo", "com.x.Foo", vec![marker("Component")]);
        assert_eq!(stereotype_bean(&td, "f.java").unwrap().name, "foo");
    }

    #[test]
    fn non_annotated_class_is_not_a_bean() {
        let td = typed("Plain", "com.x.Plain", Vec::new());
        assert!(stereotype_bean(&td, "f.java").is_none());
        // A non-stereotype annotation doesn't declare a bean either.
        let td = typed("Plain", "com.x.Plain", vec![marker("Deprecated")]);
        assert!(stereotype_bean(&td, "f.java").is_none());
    }

    #[test]
    fn collect_over_two_sources() {
        let sources = vec![
            (
                PathBuf::from("src/com/x/FooService.java"),
                "package com.x; @Service class FooService {}".to_string(),
            ),
            (
                PathBuf::from("src/com/x/BarRepo.java"),
                "package com.x; @Repository(\"barRepo\") class BarRepo {}".to_string(),
            ),
        ];
        let beans = collect_annotation_beans(&sources);
        assert_eq!(beans.len(), 2);

        let foo = beans.iter().find(|b| b.fqcn == "com.x.FooService").unwrap();
        assert_eq!(foo.name, "fooService");
        assert_eq!(foo.source_file, "src/com/x/FooService.java");

        let bar = beans.iter().find(|b| b.fqcn == "com.x.BarRepo").unwrap();
        assert_eq!(bar.name, "barRepo");
    }
}
