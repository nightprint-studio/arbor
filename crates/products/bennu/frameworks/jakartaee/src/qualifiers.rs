//! Which annotations are qualifiers — and the honest third answer, *cannot tell*.
//!
//! A qualifier is any annotation type meta-annotated `@Qualifier`, and a project's are read like any
//! other type. A library's are not: `@RestClient`, `@ConfigProperty` and a vendor's own are qualifiers
//! nobody here has seen declared. So an annotation from outside the project is classified only when
//! its package settles it — the platform's own namespaces declare no qualifier beyond the few known
//! by name — and is otherwise [`AnnClass::Unclassified`], which every check downstream reads as "this
//! bean's qualifiers are not fully known".

use bennu_facts::prelude::{AnnFacts, JavaFacts};

use crate::known;
use crate::text::decapitalize;
use crate::types::{resolve, Stereotype, TypeRef, TypeView};

/// `java.lang` annotations — used everywhere, written with no import, and certainly not qualifiers.
const JAVA_LANG: &[&str] = &["Override", "Deprecated", "SuppressWarnings", "SafeVarargs", "FunctionalInterface"];

/// A project qualifier as written: its type, and its members as source text (`""` for a marker).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomQualifier {
    pub fqcn: String,
    pub args: String,
}

/// What one annotation is, as far as bean resolution cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnClass {
    /// `@Named`, with its value when one is written.
    Named(Option<String>),
    Default,
    Any,
    Custom(CustomQualifier),
    Stereotype(Stereotype),
    /// Known not to be a qualifier.
    NotQualifier,
    /// Could be anything.
    Unclassified,
}

/// A declaration's qualifiers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Qualifiers {
    /// The EL name, defaults applied.
    pub named: Option<String>,
    pub custom: Vec<CustomQualifier>,
    pub explicit_default: bool,
    pub any: bool,
    /// An annotation that could not be classified.
    pub unclassified: bool,
    /// A `@Named` whose default could not be derived (a parameter's, which CDI requires a value for).
    pub unnamed: bool,
    /// `@New` — a legacy qualifier with rules of its own; never judged.
    pub legacy_new: bool,
}

/// Classify one annotation.
pub fn classify(ann: &AnnFacts, facts: &JavaFacts, view: &TypeView<'_>) -> AnnClass {
    if known::is(ann, facts, "Named") {
        return AnnClass::Named(ann.value_str().map(str::to_string));
    }
    if known::is(ann, facts, "Default") {
        return AnnClass::Default;
    }
    if known::is(ann, facts, "Any") {
        return AnnClass::Any;
    }
    if known::is(ann, facts, "New") {
        return AnnClass::Unclassified;
    }
    if known::is_catalogued(ann, facts) {
        return AnnClass::NotQualifier;
    }
    let written = if ann.qualified.is_empty() { ann.name.as_str() } else { ann.qualified.as_str() };
    match resolve(written, facts, &view.names()) {
        TypeRef::Project(fqcn) => match view.get(&fqcn) {
            Some(row) if row.qualifier => AnnClass::Custom(CustomQualifier { fqcn, args: args_text(ann) }),
            Some(row) => row.stereotype.clone().map(AnnClass::Stereotype).unwrap_or(AnnClass::NotQualifier),
            None => AnnClass::Unclassified,
        },
        TypeRef::Library(name) => classify_library(&name, facts),
        TypeRef::Unresolved(_) => AnnClass::Unclassified,
    }
}

fn classify_library(name: &str, facts: &JavaFacts) -> AnnClass {
    let settled = |dotted: &str| known::is_platform_name(dotted) || dotted.starts_with("lombok.");
    if name.contains('.') {
        return if settled(name) { AnnClass::NotQualifier } else { AnnClass::Unclassified };
    }
    if JAVA_LANG.contains(&name) {
        return AnnClass::NotQualifier;
    }
    // A simple name no import names: it came through a wildcard. When every wildcard is the
    // platform's, so is the annotation.
    let wildcards: Vec<&str> = facts.imports.iter().filter_map(|i| i.strip_suffix(".*")).collect();
    match !wildcards.is_empty() && wildcards.iter().all(|w| settled(&format!("{w}."))) {
        true => AnnClass::NotQualifier,
        false => AnnClass::Unclassified,
    }
}

/// A qualifier's members as comparable text, whitespace removed.
fn args_text(ann: &AnnFacts) -> String {
    let mut parts: Vec<String> = ann.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
    parts.extend(ann.positional.iter().cloned());
    parts.extend(ann.strings.iter().filter(|s| s.element.is_empty()).map(|s| format!("\"{}\"", s.value)));
    parts.join(",").chars().filter(|c| !c.is_whitespace()).collect()
}

/// Read a declaration's qualifiers. `default_name` is what an empty `@Named` means here — `None`
/// where CDI demands a value.
pub fn qualifiers_of(
    anns: &[AnnFacts],
    facts: &JavaFacts,
    view: &TypeView<'_>,
    default_name: Option<&str>,
) -> (Qualifiers, Vec<Stereotype>) {
    let mut q = Qualifiers::default();
    let mut stereotypes = Vec::new();
    for ann in anns {
        match classify(ann, facts, view) {
            AnnClass::Named(value) => match value.filter(|v| !v.is_empty()) {
                Some(v) => q.named = Some(v),
                None => match default_name {
                    Some(d) => q.named = Some(d.to_string()),
                    None => q.unnamed = true,
                },
            },
            AnnClass::Default => q.explicit_default = true,
            AnnClass::Any => q.any = true,
            AnnClass::Custom(c) => q.custom.push(c),
            AnnClass::Stereotype(s) => stereotypes.push(s),
            AnnClass::NotQualifier => {}
            AnnClass::Unclassified => {
                if known::is(ann, facts, "New") {
                    q.legacy_new = true;
                }
                q.unclassified = true;
            }
        }
    }
    // A stereotype declaring an empty `@Named` gives its beans the default name.
    if q.named.is_none() && stereotypes.iter().any(|s| s.named) {
        q.named = default_name.map(str::to_string);
    }
    (q, stereotypes)
}

/// The default EL name of a producer: the method's name, or the property a getter reads.
pub fn producer_default_name(member: &str) -> String {
    match member.strip_prefix("get") {
        Some(rest) if rest.starts_with(|c: char| c.is_uppercase()) => decapitalize(rest),
        _ => member.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TypeTable, Unit};

    fn setup(project: &[(&str, &str)]) -> TypeTable {
        let units: Vec<Unit> = project.iter().map(|(p, s)| Unit::new(p, s).unwrap()).collect();
        TypeTable::build(&units, Vec::<String>::new())
    }

    fn field_quals(table: &TypeTable, src: &str) -> Qualifiers {
        let u = Unit::new("/p/a/C.java", src).unwrap();
        let view = TypeView::of(table);
        let f = &u.facts.types[0].fields[0];
        qualifiers_of(&f.annotations, &u.facts, &view, Some(f.name.as_str())).0
    }

    const FAST: &str = "package a;\nimport jakarta.inject.Qualifier;\n@Qualifier public @interface Fast {}\n";

    #[test]
    fn a_project_qualifier_is_read_from_its_declaration() {
        let t = setup(&[("/p/a/Fast.java", FAST)]);
        let q = field_quals(&t, "package a;\nimport jakarta.inject.Inject;\nclass C { @Inject @Fast Engine e; }\n");
        assert_eq!(q.custom, [CustomQualifier { fqcn: "a.Fast".into(), args: String::new() }]);
        assert!(!q.unclassified);
    }

    #[test]
    fn named_takes_the_member_name_when_empty() {
        let t = setup(&[]);
        let q = field_quals(&t, "package a;\nimport jakarta.inject.*;\nclass C { @Inject @Named Engine engine; }\n");
        assert_eq!(q.named.as_deref(), Some("engine"));
        let q = field_quals(&t, "package a;\nimport jakarta.inject.*;\nclass C { @Inject @Named(\"x\") Engine engine; }\n");
        assert_eq!(q.named.as_deref(), Some("x"));
    }

    #[test]
    fn a_library_annotation_outside_the_platform_cannot_be_classified() {
        let t = setup(&[]);
        let q = field_quals(&t, "package a;\nimport jakarta.inject.Inject;\nimport org.eclipse.microprofile.rest.client.inject.RestClient;\nclass C { @Inject @RestClient Api api; }\n");
        assert!(q.unclassified);
        let q = field_quals(&t, "package a;\nimport jakarta.inject.Inject;\nimport jakarta.transaction.Transactional;\nclass C { @Inject @Transactional Api api; }\n");
        assert!(!q.unclassified, "the platform's own namespaces declare no unknown qualifier");
    }

    #[test]
    fn a_getter_producer_is_named_after_its_property() {
        assert_eq!(producer_default_name("getCurrentUser"), "currentUser");
        assert_eq!(producer_default_name("clock"), "clock");
    }
}
