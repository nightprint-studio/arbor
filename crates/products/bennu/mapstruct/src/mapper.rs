//! Mappers and their mapping methods, analysed against the project.
//!
//! One analysis serves every question — diagnostics, fixes, completion, the panel — so they cannot
//! disagree about what a method's target is or which policy applies to it.

use bennu_facts::prelude::{AnnFacts, AnnotationTable, JavaFacts, KnownAnnotation, MethodFacts, TypeFacts};
use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use crate::annotations::{method_node, read_method, MethodAnns};
use crate::model::Knowledge;
use crate::table::{Scope, TypeRef};

const PKG: &[&str] = &["org.mapstruct"];

/// The MapStruct annotations this crate reasons about, pinned to their package.
pub const ANNOTATIONS: AnnotationTable = AnnotationTable::new(&[
    KnownAnnotation { simple: "Mapper", packages: PKG },
    KnownAnnotation { simple: "MapperConfig", packages: PKG },
    KnownAnnotation { simple: "Mapping", packages: PKG },
    KnownAnnotation { simple: "Mappings", packages: PKG },
    KnownAnnotation { simple: "BeanMapping", packages: PKG },
    KnownAnnotation { simple: "InheritConfiguration", packages: PKG },
    KnownAnnotation { simple: "InheritInverseConfiguration", packages: PKG },
    KnownAnnotation { simple: "MappingTarget", packages: PKG },
    KnownAnnotation { simple: "Context", packages: PKG },
    KnownAnnotation { simple: "TargetType", packages: PKG },
    KnownAnnotation { simple: "Named", packages: PKG },
    KnownAnnotation { simple: "IterableMapping", packages: PKG },
    KnownAnnotation { simple: "MapMapping", packages: PKG },
    KnownAnnotation { simple: "ValueMapping", packages: PKG },
    KnownAnnotation { simple: "ValueMappings", packages: PKG },
]);

/// The method-level annotations that are MapStruct's — every one of them in [`ANNOTATIONS`].
const METHOD_LEVEL: &[&str] = &[
    "Mapping", "Mappings", "BeanMapping", "InheritConfiguration", "InheritInverseConfiguration", "Named",
    "IterableMapping", "MapMapping", "ValueMapping", "ValueMappings",
];
/// `java.lang` annotations that say nothing about a mapping.
const HARMLESS: &[&str] = &["Override", "Deprecated", "SuppressWarnings"];

/// `unmappedTargetPolicy`, as far as it can be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Ignore,
    Warn,
    Error,
    /// A constant, a config that cannot be found, a processor option on a build this crate cannot
    /// read. Treated as IGNORE — not knowing is not a reason to warn.
    Unknown,
}

impl Policy {
    pub fn parse(raw: &str) -> Policy {
        match raw.trim().rsplit('.').next().unwrap_or("") {
            "IGNORE" => Policy::Ignore,
            "WARN" => Policy::Warn,
            "ERROR" => Policy::Error,
            _ => Policy::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceParam {
    pub name: String,
    pub type_text: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    /// Returns a new target.
    Mapping,
    /// Writes into a `@MappingTarget` parameter.
    Update,
    /// Iterates: a collection, array, map, stream or generic on either side.
    Collection,
}

impl MethodKind {
    pub fn label(self) -> &'static str {
        match self {
            MethodKind::Mapping => "mapping",
            MethodKind::Update => "update",
            MethodKind::Collection => "collection",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingMethod {
    pub name: String,
    pub name_offset: usize,
    pub kind: MethodKind,
    /// Every parameter's type, qualifiers dropped — for display.
    pub param_types: Vec<String>,
    pub sources: Vec<SourceParam>,
    pub target_text: String,
    pub target: TypeRef,
    pub anns: MethodAnns,
    pub policy: Policy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapper {
    pub fqcn: String,
    pub name: String,
    pub name_offset: usize,
    pub component_model: Option<String>,
    pub methods: Vec<MappingMethod>,
}

/// Every `@Mapper` declared in a file, analysed.
pub fn mappers_in(facts: &JavaFacts, source: &str, known: &Knowledge) -> Vec<Mapper> {
    let declared: Vec<&TypeFacts> = facts
        .types
        .iter()
        .filter(|t| matches!(t.kind, "interface" | "class") && ANNOTATIONS.has(&t.annotations, facts, "Mapper"))
        .collect();
    if declared.is_empty() {
        return Vec::new();
    }
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    declared.into_iter().map(|t| mapper(t, facts, source, root, known)).collect()
}

/// What analysing one method needs, gathered so the helpers take one argument rather than seven.
struct Ctx<'a, 't> {
    facts: &'a JavaFacts,
    source: &'a str,
    root: Node<'t>,
    scope: Scope<'a>,
    known: &'a Knowledge,
    policy: Policy,
}

fn mapper(t: &TypeFacts, facts: &JavaFacts, source: &str, root: Node<'_>, known: &Knowledge) -> Mapper {
    let scope = Scope { package: &facts.package, imports: &facts.imports, owner: Some(t.fqcn.as_str()) };
    let ann = ANNOTATIONS.find(&t.annotations, facts, "Mapper");
    let ctx = Ctx { facts, source, root, scope, known, policy: mapper_policy(ann, scope, known) };
    let methods = t
        .methods
        .iter()
        .filter(|m| !m.has_body && !m.is_static && !m.is_constructor)
        .filter_map(|m| mapping_method(m, &ctx))
        .collect();
    Mapper {
        fqcn: t.fqcn.clone(),
        name: t.name.clone(),
        name_offset: t.name_offset,
        component_model: ann.and_then(|a| a.pair("componentModel")).map(component_label).filter(|c| c.as_str() != "default"),
        methods,
    }
}

/// The policy a mapper declares, through its config when it names one. MapStruct's own order: the
/// mapper's element, then the config's, then the build's default.
fn mapper_policy(ann: Option<&AnnFacts>, scope: Scope<'_>, known: &Knowledge) -> Policy {
    let Some(ann) = ann else { return Policy::Unknown };
    let config = match ann.pair("config") {
        None => None,
        Some(raw) => {
            let written = raw.trim().trim_end_matches(".class").trim();
            let TypeRef::Project(fqcn) = known.table.resolve(written, scope) else { return Policy::Unknown };
            match known.configs.get(&fqcn) {
                Some(config) => Some(config),
                None => return Policy::Unknown,
            }
        }
    };
    // Inherited prototype mappings may cover any target property — nothing can be said about the rest.
    if ann.pair("mappingInheritanceStrategy").is_some() || config.is_some_and(|c| c.inherits) {
        return Policy::Unknown;
    }
    if let Some(raw) = ann.pair("unmappedTargetPolicy") {
        return Policy::parse(raw);
    }
    config.and_then(|c| c.policy).unwrap_or(known.default_policy)
}

fn component_label(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(inner) = raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return inner.to_string();
    }
    // `MappingConstants.ComponentModel.SPRING` — the constant's name is the value, lowercased.
    raw.rsplit('.').next().unwrap_or(raw).to_ascii_lowercase()
}

fn mapping_method(m: &MethodFacts, ctx: &Ctx<'_, '_>) -> Option<MappingMethod> {
    let node = method_node(ctx.root, m.name_offset)?;
    let facts = ctx.facts;
    let anns = read_method(node, ctx.source, &|written| method_annotation(written, facts));

    let mut target_text = None;
    let mut sources = Vec::new();
    for p in &m.params {
        if ANNOTATIONS.has(&p.annotations, facts, "MappingTarget") {
            target_text = Some(p.type_text.clone());
        } else if !ANNOTATIONS.has(&p.annotations, facts, "Context")
            && !ANNOTATIONS.has(&p.annotations, facts, "TargetType")
        {
            let ty = ctx.known.table.resolve(&p.type_text, ctx.scope);
            sources.push(SourceParam { name: p.name.clone(), type_text: p.type_text.clone(), ty });
        }
    }
    let update = target_text.is_some();
    let target_text = match target_text {
        Some(t) => t,
        None if m.return_type.trim() == "void" => return None,
        None => m.return_type.clone(),
    };
    if sources.is_empty() {
        return None;
    }
    let target = ctx.known.table.resolve(&target_text, ctx.scope);
    let kind = if target == TypeRef::Collection || sources.iter().any(|s| s.ty == TypeRef::Collection) {
        MethodKind::Collection
    } else if update {
        MethodKind::Update
    } else {
        MethodKind::Mapping
    };
    let policy = anns.policy.as_deref().map(Policy::parse).unwrap_or(ctx.policy);
    Some(MappingMethod {
        name: m.name.clone(),
        name_offset: m.name_offset,
        kind,
        param_types: m.params.iter().map(|p| simple_type(&p.type_text)).collect(),
        sources,
        target_text,
        target,
        anns,
        policy,
    })
}

/// Which annotation a method-level name written in this file is — `None` for anything this crate
/// does not recognise.
fn method_annotation(written: &str, facts: &JavaFacts) -> Option<&'static str> {
    let simple = written.rsplit('.').next().unwrap_or(written);
    if written == simple || written.starts_with("java.lang.") {
        if let Some(h) = HARMLESS.iter().copied().find(|h| *h == simple) {
            return Some(h);
        }
    }
    let probe = AnnFacts {
        name: simple.to_string(),
        qualified: written.to_string(),
        start: 0,
        end: 0,
        strings: Vec::new(),
        args: Vec::new(),
        positional: Vec::new(),
    };
    METHOD_LEVEL.iter().copied().find(|n| ANNOTATIONS.is(&probe, facts, n))
}

/// A type as written, with every package qualifier dropped: `java.util.List<com.acme.User>` →
/// `List<User>`.
pub fn simple_type(written: &str) -> String {
    let written = written.replace("...", "[]");
    let mut out = String::new();
    let mut word = String::new();
    for c in written.chars() {
        if c.is_alphanumeric() || c == '_' || c == '$' {
            word.push(c);
            continue;
        }
        if c == '.' {
            word.clear();
            continue;
        }
        out.push_str(&word);
        word.clear();
        if c == ',' {
            out.push_str(", ");
        } else if c.is_whitespace() {
            if out.chars().last().is_some_and(|l| l.is_alphanumeric() || l == '?') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out.push_str(&word);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_policy_reads_through_its_qualifier() {
        assert_eq!(Policy::parse("ReportingPolicy.ERROR"), Policy::Error);
        assert_eq!(Policy::parse("org.mapstruct.ReportingPolicy.IGNORE"), Policy::Ignore);
        assert_eq!(Policy::parse("WARN"), Policy::Warn);
        assert_eq!(Policy::parse("POLICY"), Policy::Unknown);
    }

    #[test]
    fn a_type_is_shown_without_its_packages() {
        assert_eq!(simple_type("java.util.List<com.acme.User>"), "List<User>");
        assert_eq!(simple_type("Map<String,com.a.B>"), "Map<String, B>");
        assert_eq!(simple_type("String..."), "String[]");
        assert_eq!(simple_type("List<? extends a.X>"), "List<? extends X>");
    }

    #[test]
    fn a_component_model_reads_as_a_string_or_a_constant() {
        assert_eq!(component_label("\"spring\""), "spring");
        assert_eq!(component_label("MappingConstants.ComponentModel.CDI"), "cdi");
    }
}
