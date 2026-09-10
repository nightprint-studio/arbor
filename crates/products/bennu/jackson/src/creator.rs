//! `@JsonCreator` without names, on a build that does not keep them.
//!
//! ## A defect that lives in two files, neither suspicious alone
//!
//! ```java
//! @JsonCreator
//! public Ordine(String codice, BigDecimal totale) { … }
//! ```
//!
//! That is a property-based creator: Jackson has to match each argument to a JSON field, and it
//! does that **by name**. Java does not keep parameter names in the class file unless the compiler
//! is told to — `javac -parameters`, or `<maven.compiler.parameters>true</…>` in the pom. Without
//! it, the names are `arg0` and `arg1`, and deserialisation fails with
//!
//! > *Argument #0 of constructor has no property name annotation; must have name when multiple-parameter constructor annotated as Creator*
//!
//! Nothing about the constructor is wrong. Nothing about the pom is wrong. The defect is the pair,
//! and the two halves are in different files that nobody reads together — which is why it survives
//! code review and turns up the first time somebody POSTs to that endpoint.
//!
//! ## What is deliberately not reported
//!
//! - **A single-argument creator.** With no name to go on, Jackson reads it as *delegating* — the
//!   whole payload becomes the one argument — which is legal and is what plenty of value types do.
//! - **An explicit `mode = Mode.DELEGATING`**, for the same reason, at any arity.
//! - **A record.** Jackson reads a record's components from the class file itself, `-parameters` or
//!   not.
//! - **`@ConstructorProperties`**, which carries the names as data. Lombok writes it when asked to.
//! - **Any project whose compiler settings cannot be read** — a Gradle build, a pom this cannot
//!   find. Not knowing is not evidence, and reporting it would be reporting our own blind spot.

use bennu_java::prelude::{
    annotation_named, annotation_value_text, annotations_of, node_text, parse_java, simple_name,
    type_declarations,
};
use bennu_xml::prelude::Doc;
use tree_sitter::Node;

/// Whether the build keeps parameter names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ParameterNames {
    /// The compiler is told to keep them — nothing here can go wrong.
    Kept,
    /// It is not, and a property-based creator needs `@JsonProperty` on every argument.
    Dropped,
    /// No pom was readable. Not knowing is not evidence: the check stays quiet.
    #[default]
    Unknown,
}

/// A creator whose arguments Jackson will not be able to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnnamedCreator {
    /// The type declaring it.
    pub owner: String,
    /// How many arguments it takes.
    pub arity: usize,
    /// Byte span of the `@JsonCreator` annotation.
    pub start: usize,
    pub end: usize,
}

/// Read a source for property-based creators with no argument names.
///
/// `names` is the project's compiler setting; anything but [`ParameterNames::Dropped`] yields
/// nothing, because on those builds there is nothing to report.
pub fn creators_in(source: &str, names: ParameterNames) -> Vec<UnnamedCreator> {
    if names != ParameterNames::Dropped || !source.contains("JsonCreator") {
        return Vec::new();
    }
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    for type_decl in type_declarations(tree.root_node()) {
        // A record's components are in the class file whatever the compiler was told.
        if type_decl.kind() == "record_declaration" {
            continue;
        }
        let owner = type_decl
            .child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string())
            .unwrap_or_default();
        let Some(body) = type_decl.child_by_field_name("body") else { continue };
        let mut cursor = body.walk();
        for member in body.named_children(&mut cursor) {
            if !matches!(member.kind(), "constructor_declaration" | "method_declaration") {
                continue;
            }
            if let Some(found) = check_member(member, source, &owner) {
                out.push(found);
            }
        }
    }
    out.sort_by_key(|c| c.start);
    out
}

fn check_member(member: Node<'_>, source: &str, owner: &str) -> Option<UnnamedCreator> {
    let creator = annotation_named(member, source, "JsonCreator")?;

    // Delegating: the whole payload becomes the one argument, and no names are wanted.
    if let Some(mode) = annotation_value_text(creator, source, "mode") {
        if mode.ends_with("DELEGATING") {
            return None;
        }
    }
    // The names are carried as data.
    if annotation_named(member, source, "ConstructorProperties").is_some() {
        return None;
    }

    let params = member.child_by_field_name("parameters")?;
    let mut cursor = params.walk();
    let parameters: Vec<Node<'_>> = params
        .named_children(&mut cursor)
        .filter(|p| matches!(p.kind(), "formal_parameter" | "spread_parameter"))
        .collect();

    // One argument with no name is read as delegating, which is legal and common.
    if parameters.len() < 2 {
        return None;
    }
    // A single named argument is enough to make this a property-based creator that Jackson can
    // resolve — the ones missing a name then get their real ones from `-parameters`, and reporting
    // the mixture would be a different (and weaker) claim than this one.
    let named = parameters.iter().any(|p| {
        annotations_of(*p, source)
            .iter()
            .any(|(n, _)| matches!(simple_name(n), "JsonProperty" | "JacksonInject"))
    });
    if named {
        return None;
    }

    Some(UnnamedCreator {
        owner: owner.to_string(),
        arity: parameters.len(),
        start: creator.start_byte(),
        end: creator.end_byte(),
    })
}

/// Whether this pom tells the compiler to keep parameter names.
///
/// Four spellings, because a project uses whichever one it inherited: the property, the plugin's
/// `<parameters>`, a raw `-parameters` in `<compilerArgs>`, and — the one that matters most in
/// practice — `spring-boot-starter-parent`, which sets the property for you. A project on Boot's
/// parent is the majority of new Java, and reporting all of it would sink the check.
pub fn keeps_parameter_names(pom_xml: &str) -> bool {
    if pom_xml.contains("-parameters") {
        return true;
    }
    let doc = Doc::new(pom_xml);
    let Some(root) = doc.root() else { return false };

    if let Some(parent) = doc.child(root, "parent") {
        if doc.child_text(parent, "artifactId") == "spring-boot-starter-parent" {
            return true;
        }
    }
    if let Some(props) = doc.child(root, "properties") {
        if doc.child_text(props, "maven.compiler.parameters") == "true" {
            return true;
        }
    }
    // `<build><plugins><plugin>maven-compiler-plugin<configuration><parameters>true`.
    let Some(build) = doc.child(root, "build") else { return false };
    let Some(plugins) = doc.child(build, "plugins") else { return false };
    doc.children(plugins)
        .into_iter()
        .filter(|p| doc.name(*p) == "plugin")
        .filter(|p| doc.child_text(*p, "artifactId") == "maven-compiler-plugin")
        .filter_map(|p| doc.child(p, "configuration"))
        .any(|c| doc.child_text(c, "parameters") == "true")
}

/// The project's setting, from every pom it has. One pom that keeps the names is not enough —
/// a reactor compiles module by module — but this is the honest approximation, and the
/// alternative (per-module) would report on a module whose own pom says nothing while its parent
/// says everything.
pub fn parameter_names(poms: &[String]) -> ParameterNames {
    if poms.is_empty() {
        return ParameterNames::Unknown;
    }
    if poms.iter().any(|p| keeps_parameter_names(p)) {
        ParameterNames::Kept
    } else {
        ParameterNames::Dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"public class Ordine {

    @JsonCreator
    public Ordine(String codice, BigDecimal totale) { }

    @JsonCreator
    public static Ordine of(String codice, BigDecimal totale) { return null; }
}
"#;

    #[test]
    fn a_multi_argument_creator_with_no_names_is_the_defect() {
        let found = creators_in(SRC, ParameterNames::Dropped);
        assert_eq!(found.len(), 2, "the constructor and the factory: {found:?}");
        assert_eq!(found[0].owner, "Ordine");
        assert_eq!(found[0].arity, 2);
        assert_eq!(&SRC[found[0].start..found[0].end], "@JsonCreator");
    }

    /// The other half of the pair. On a build that keeps the names there is nothing wrong here at
    /// all, and this is the whole reason the check needs the pom.
    #[test]
    fn the_same_class_is_fine_on_a_build_that_keeps_the_names() {
        assert!(creators_in(SRC, ParameterNames::Kept).is_empty());
        assert!(creators_in(SRC, ParameterNames::Unknown).is_empty());
    }

    #[test]
    fn a_named_argument_makes_it_resolvable() {
        let src = r#"class A {
            @JsonCreator public A(@JsonProperty("a") String a, String b) { }
        }"#;
        assert!(creators_in(src, ParameterNames::Dropped).is_empty());
    }

    /// One argument with no name is a delegating creator, which is legal and common.
    #[test]
    fn a_single_argument_creator_is_delegating_and_legal() {
        let src = "class A { @JsonCreator public A(String whole) { } }";
        assert!(creators_in(src, ParameterNames::Dropped).is_empty());
    }

    #[test]
    fn an_explicit_delegating_mode_is_left_alone_at_any_arity() {
        let src = r#"class A {
            @JsonCreator(mode = JsonCreator.Mode.DELEGATING) public A(String a, String b) { }
        }"#;
        assert!(creators_in(src, ParameterNames::Dropped).is_empty());
    }

    #[test]
    fn a_record_carries_its_component_names_itself() {
        let src = r#"record Ordine(String codice, BigDecimal totale) {
            @JsonCreator public Ordine { }
        }"#;
        assert!(creators_in(src, ParameterNames::Dropped).is_empty());
    }

    #[test]
    fn constructor_properties_carries_the_names_as_data() {
        let src = r#"class A {
            @JsonCreator @ConstructorProperties({"a", "b"}) public A(String a, String b) { }
        }"#;
        assert!(creators_in(src, ParameterNames::Dropped).is_empty());
    }

    // ── reading the pom ──────────────────────────────────────────────────────

    #[test]
    fn the_property_spelling_is_recognised() {
        let pom = "<project><properties><maven.compiler.parameters>true</maven.compiler.parameters></properties></project>";
        assert!(keeps_parameter_names(pom));
    }

    #[test]
    fn the_plugin_configuration_is_recognised() {
        let pom = r#"<project><build><plugins><plugin>
            <artifactId>maven-compiler-plugin</artifactId>
            <configuration><parameters>true</parameters></configuration>
        </plugin></plugins></build></project>"#;
        assert!(keeps_parameter_names(pom));
    }

    #[test]
    fn a_raw_compiler_argument_is_recognised() {
        let pom = r#"<project><build><plugins><plugin>
            <artifactId>maven-compiler-plugin</artifactId>
            <configuration><compilerArgs><arg>-parameters</arg></compilerArgs></configuration>
        </plugin></plugins></build></project>"#;
        assert!(keeps_parameter_names(pom));
    }

    /// The one that matters most in practice: Boot's parent sets it for you, and a check that
    /// reported every Boot project would be turned off the same afternoon.
    #[test]
    fn boots_parent_sets_it_for_you() {
        let pom = r#"<project><parent>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-parent</artifactId>
            <version>3.2.0</version>
        </parent><artifactId>app</artifactId></project>"#;
        assert!(keeps_parameter_names(pom));
    }

    #[test]
    fn a_plain_pom_does_not_keep_them() {
        let pom = "<project><artifactId>app</artifactId></project>";
        assert!(!keeps_parameter_names(pom));
        assert_eq!(parameter_names(&[pom.to_string()]), ParameterNames::Dropped);
    }

    /// No pom is not evidence of anything — a Gradle build, or a scan that has not found one.
    #[test]
    fn no_pom_means_no_claim() {
        assert_eq!(parameter_names(&[]), ParameterNames::Unknown);
    }

    /// A `<parameters>` under some other plugin is not the compiler's.
    #[test]
    fn another_plugins_configuration_is_not_the_compilers() {
        let pom = r#"<project><build><plugins><plugin>
            <artifactId>maven-surefire-plugin</artifactId>
            <configuration><parameters>true</parameters></configuration>
        </plugin></plugins></build></project>"#;
        assert!(!keeps_parameter_names(pom));
    }
}
