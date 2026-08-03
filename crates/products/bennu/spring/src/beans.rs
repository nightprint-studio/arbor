//! Deriving the bean registry, the injection points and the type index from a scan.
//!
//! Three sources of beans, one registry:
//!
//! - **stereotypes** — `@Service` / `@Component` / `@Repository` / `@Controller` /
//!   `@RestController` / `@Configuration`, plus the JSR-330 `@Named` / `@ManagedBean`;
//! - **factory methods** — `@Bean` inside a configuration class;
//! - **XML** — `<bean class="…">`, including the `parent=` chain.
//!
//! ## Where this is generous, and where it is not
//!
//! Generous: **type resolution**. A bean's class is recorded as written and matched by
//! simple name, so `OrderService` finds `OrderServiceImpl` without a full import
//! resolution. This drives navigation — a picker the user reads — where being too strict
//! costs the feature and being too loose costs a row.
//!
//! Not generous: **[`TypeInfo::properties_complete`]**. That flag gates the one check that
//! can call something an error (`<property name=>` naming nothing), and it is set to false
//! the moment the picture is incomplete — an unresolved supertype, a Lombok
//! `@Accessors(prefix)` we did not model. "I don't know" must never be reported as "that
//! doesn't exist".

use std::collections::BTreeMap;

use crate::model::{
    default_bean_name, line_at, simple_name, strip_generics, BeanCondition, BeanDef, BeanKind,
    InjectionKind, InjectionPoint, TypeInfo,
};
use crate::scan::{AnnFacts, JavaFacts, TypeFacts};
use crate::xml::XmlBeanFile;

/// One scanned Java file plus the text it was scanned from (needed for line numbers).
#[derive(Debug, Clone)]
pub struct JavaUnit {
    pub facts: JavaFacts,
    pub text: String,
}

/// Class annotations that register a bean, mapped to the badge shown for them.
const STEREOTYPES: &[&str] = &[
    "Component",
    "Service",
    "Repository",
    "Controller",
    "RestController",
    "Configuration",
    "ControllerAdvice",
    "RestControllerAdvice",
    "Named",
    "ManagedBean",
];

/// Field / parameter annotations that mark an injection point.
const INJECT_MARKERS: &[&str] = &["Autowired", "Inject", "Resource"];

/// Lombok annotations that generate an all-required-args constructor — which turns the
/// class's final fields into constructor injection points with no constructor in sight.
const LOMBOK_CTORS: &[&str] = &["RequiredArgsConstructor", "AllArgsConstructor"];

/// Lombok annotations whose effect on the writable-property set we DO model.
const LOMBOK_MODELLED: &[&str] = &[
    "Data",
    "Setter",
    "Getter",
    "Value",
    "Builder",
    "NoArgsConstructor",
    "AllArgsConstructor",
    "RequiredArgsConstructor",
    "ToString",
    "EqualsAndHashCode",
    "Slf4j",
    "NonNull",
    "SneakyThrows",
];

// Every one of these goes through `known`, which resolves the annotation's ORIGIN through the
// file's imports rather than trusting its simple name. `@Service` is not a reserved word: a
// project may declare `com.acme.Service`, and matching on the name alone would register a bean
// that does not exist.

fn has(anns: &[AnnFacts], facts: &JavaFacts, name: &str) -> bool {
    crate::known::has(anns, facts, name)
}

/// The first string value of `name`'s annotation, or empty.
fn ann_value(anns: &[AnnFacts], facts: &JavaFacts, name: &str) -> String {
    crate::known::find(anns, facts, name)
        .and_then(|a| a.value())
        .map(|s| s.value.clone())
        .unwrap_or_default()
}

/// The `@ConditionalOn…` annotations, and how each one reads.
///
/// `@ConditionalOnProperty` is the one that carries a *value* worth resolving, so its key is
/// pulled out; the rest are summarised for display. `prefix` is folded onto `name`, because
/// Spring joins them and showing half a key would be worse than showing none.
const CONDITIONS: &[&str] = &[
    "ConditionalOnProperty",
    "ConditionalOnBean",
    "ConditionalOnMissingBean",
    "ConditionalOnClass",
    "ConditionalOnMissingClass",
    "ConditionalOnExpression",
    "ConditionalOnWebApplication",
    "ConditionalOnNotWebApplication",
    "ConditionalOnResource",
    "ConditionalOnSingleCandidate",
    "ConditionalOnJava",
    "Conditional",
];

/// The full property key a `@ConditionalOnProperty` tests: `prefix` joined to `name` (or to the
/// bare value, which is an alias for `name`). Empty when it names none.
pub fn conditional_property_key(ann: &AnnFacts) -> String {
    let name = ann
        .strings_for("name")
        .next()
        .or_else(|| ann.strings_for("value").next())
        .or_else(|| ann.strings.iter().find(|s| s.element.is_empty()))
        .map(|s| s.value.clone())
        .unwrap_or_default();
    if name.is_empty() {
        return String::new();
    }
    match ann.strings_for("prefix").next() {
        Some(p) if !p.value.is_empty() => format!("{}.{}", p.value.trim_end_matches('.'), name),
        _ => name,
    }
}

/// Read the conditions on a declaration.
fn conditions_of(anns: &[AnnFacts], facts: &JavaFacts) -> Vec<BeanCondition> {
    anns.iter()
        .filter_map(|a| {
            let name = crate::known::is_any(a, facts, CONDITIONS)?;
            let key = if name == "ConditionalOnProperty" {
                conditional_property_key(a)
            } else {
                String::new()
            };
            Some(BeanCondition {
                summary: summarize(name, a, &key),
                property_key: key,
                name: name.to_string(),
            })
        })
        .collect()
}

/// A one-line reading of a condition, in the terms the user wrote it in.
fn summarize(name: &str, ann: &AnnFacts, key: &str) -> String {
    // The single type a `@ConditionalOnBean(Foo.class)` / `@ConditionalOnClass(Foo.class)` names,
    // written either positionally or as `value =` / `name =`.
    let subject = || {
        ann.positional
            .first()
            .cloned()
            .or_else(|| ann.pair("value").map(str::to_string))
            .or_else(|| ann.pair("type").map(str::to_string))
            .or_else(|| ann.strings.first().map(|s| s.value.clone()))
            .map(|t| simple_name(t.trim_end_matches(".class")).to_string())
            .unwrap_or_default()
    };
    match name {
        "ConditionalOnProperty" => {
            let having = ann
                .strings_for("havingValue")
                .next()
                .map(|s| s.value.clone())
                .unwrap_or_default();
            let missing_ok = ann.pair("matchIfMissing").map(|v| v.trim() == "true").unwrap_or(false);
            let base = if having.is_empty() {
                format!("{key} is set")
            } else {
                format!("{key} = {having}")
            };
            if missing_ok {
                format!("{base} (or absent)")
            } else {
                base
            }
        }
        "ConditionalOnBean" => format!("bean {} exists", subject()),
        "ConditionalOnMissingBean" => {
            let s = subject();
            if s.is_empty() {
                "no other bean of this type exists".to_string()
            } else {
                format!("no bean {s} exists")
            }
        }
        "ConditionalOnSingleCandidate" => format!("exactly one {} exists", subject()),
        "ConditionalOnClass" => format!("{} is on the classpath", subject()),
        "ConditionalOnMissingClass" => format!("{} is NOT on the classpath", subject()),
        "ConditionalOnExpression" => {
            let e = ann.value().map(|s| s.value.clone()).unwrap_or_default();
            format!("expression {e}")
        }
        "ConditionalOnWebApplication" => "a web application".to_string(),
        "ConditionalOnNotWebApplication" => "not a web application".to_string(),
        "ConditionalOnResource" => format!("resource {} exists", subject()),
        "ConditionalOnJava" => format!("Java {}", subject()),
        _ => "a custom condition".to_string(),
    }
}

/// The stereotype annotation on a type, if it carries a real one.
fn stereotype_of<'a>(t: &'a TypeFacts, facts: &JavaFacts) -> Option<&'a AnnFacts> {
    t.annotations.iter().find(|a| crate::known::is_any(a, facts, STEREOTYPES).is_some())
}

/// Resolve a written type name to a dotted FQCN using the file's imports, falling back to
/// the name as written. Never invents a package: an unimported simple name stays simple,
/// and simple-name matching takes it from there.
pub fn resolve_type(name: &str, facts: &JavaFacts) -> String {
    let bare = strip_generics(name);
    if bare.contains('.') {
        return bare;
    }
    let suffix = format!(".{bare}");
    if let Some(imp) = facts.imports.iter().find(|i| i.ends_with(&suffix)) {
        return imp.clone();
    }
    // Declared in this file? Then it is this package's.
    if facts.types.iter().any(|t| t.name == bare) && !facts.package.is_empty() {
        return format!("{}.{}", facts.package, bare);
    }
    bare
}

/// The beans a scan declares, from annotations and from `@Bean` factory methods.
pub fn annotation_beans(units: &[JavaUnit]) -> Vec<BeanDef> {
    let mut out = Vec::new();
    for u in units {
        for t in &u.facts.types {
            if let Some(ann) = stereotype_of(t, &u.facts) {
                // An interface or an abstract class carries the annotation for its
                // subclasses' benefit; it is not itself instantiated.
                if t.kind != "interface" && t.kind != "annotation" && !t.is_abstract {
                    let explicit = ann.value().map(|s| s.value.clone()).unwrap_or_default();
                    out.push(BeanDef {
                        name: if explicit.is_empty() {
                            default_bean_name(&t.fqcn)
                        } else {
                            explicit
                        },
                        fqcn: t.fqcn.clone(),
                        kind: BeanKind::Stereotype,
                        stereotype: format!("@{}", ann.name),
                        file: u.facts.file.clone(),
                        offset: t.name_offset,
                        line: line_at(&u.text, t.name_offset),
                        scope: ann_value(&t.annotations, &u.facts, "Scope"),
                        primary: has(&t.annotations, &u.facts, "Primary"),
                        profile: ann_value(&t.annotations, &u.facts, "Profile"),
                        lazy: has(&t.annotations, &u.facts, "Lazy"),
                        is_abstract: false,
                        supertypes: supertypes_of(t, &u.facts),
                        conditions: conditions_of(&t.annotations, &u.facts),
                    });
                }
            }
            // `@Bean` factory methods. Spring honours them inside any bean class, not only
            // `@Configuration`, so the method's own annotation is the whole condition.
            for m in &t.methods {
                let Some(ann) = crate::known::find(&m.annotations, &u.facts, "Bean") else {
                    continue;
                };
                let explicit = ann.value().map(|s| s.value.clone()).unwrap_or_default();
                let fqcn = resolve_type(&m.return_type, &u.facts);
                out.push(BeanDef {
                    name: if explicit.is_empty() { m.name.clone() } else { explicit },
                    fqcn,
                    kind: BeanKind::Factory,
                    stereotype: "@Bean".to_string(),
                    file: u.facts.file.clone(),
                    offset: m.name_offset,
                    line: line_at(&u.text, m.name_offset),
                    scope: ann_value(&m.annotations, &u.facts, "Scope"),
                    primary: has(&m.annotations, &u.facts, "Primary"),
                    profile: ann_value(&m.annotations, &u.facts, "Profile"),
                    lazy: has(&m.annotations, &u.facts, "Lazy"),
                    is_abstract: false,
                    // A factory method's return type is the only type information there
                    // is; its supertypes would need the resolver.
                    supertypes: Vec::new(),
                    // A `@Bean` method is gated by ITS own conditions plus the ones on the
                    // configuration class holding it — both have to hold for it to exist.
                    conditions: conditions_of(&m.annotations, &u.facts)
                        .into_iter()
                        .chain(conditions_of(&t.annotations, &u.facts))
                        .collect(),
                });
            }
        }
    }
    out
}

/// `extends` + `implements`, resolved through the file's imports where possible.
fn supertypes_of(t: &TypeFacts, facts: &JavaFacts) -> Vec<String> {
    let mut out = Vec::new();
    if !t.extends.is_empty() {
        out.push(resolve_type(&t.extends, facts));
    }
    out.extend(t.implements.iter().map(|i| resolve_type(i, facts)));
    out
}

/// The beans declared by XML, following `parent=` chains for an inherited class.
pub fn xml_beans(files: &[XmlBeanFile]) -> Vec<BeanDef> {
    // id → class, for the parent walk.
    let by_id: BTreeMap<&str, (&str, &str)> = files
        .iter()
        .flat_map(|f| f.beans.iter())
        .map(|b| (b.id.as_str(), (b.class.as_str(), b.parent.as_str())))
        .collect();

    let mut out = Vec::new();
    for f in files {
        for b in &f.beans {
            if b.id.is_empty() {
                continue; // an anonymous inner bean isn't addressable by name
            }
            out.push(BeanDef {
                name: b.id.clone(),
                fqcn: resolve_xml_class(b.class.as_str(), b.parent.as_str(), &by_id, 0),
                kind: BeanKind::Xml,
                stereotype: "<bean>".to_string(),
                file: f.path.clone(),
                offset: b.id_span.map(|(s, _)| s).unwrap_or(b.offset),
                line: b.line,
                scope: b.scope.clone(),
                primary: b.primary,
                profile: f.profile.clone(),
                lazy: b.lazy,
                is_abstract: b.is_abstract,
                supertypes: Vec::new(),
                // XML has its own conditional mechanism (`<beans profile=>`), already carried
                // by `profile` above.
                conditions: Vec::new(),
            });
        }
    }
    out
}

/// A bean's class, walking `parent=` when it declares none. Depth-capped against a cycle.
fn resolve_xml_class(
    class: &str,
    parent: &str,
    by_id: &BTreeMap<&str, (&str, &str)>,
    depth: usize,
) -> String {
    if !class.is_empty() {
        return class.to_string();
    }
    if depth > 16 || parent.is_empty() {
        return String::new();
    }
    match by_id.get(parent) {
        Some((c, p)) => resolve_xml_class(c, p, by_id, depth + 1),
        None => String::new(),
    }
}

/// Every point where a bean is asked for: annotated fields, annotated setters,
/// constructor parameters, and the final fields a Lombok-generated constructor injects.
pub fn injection_points(units: &[JavaUnit]) -> Vec<InjectionPoint> {
    let mut out = Vec::new();
    for u in units {
        for t in &u.facts.types {
            let is_bean = stereotype_of(t, &u.facts).is_some();
            for f in &t.fields {
                if f.is_static {
                    continue;
                }
                let injected = f
                    .annotations
                    .iter()
                    .any(|a| crate::known::is_any(a, &u.facts, INJECT_MARKERS).is_some());
                // Lombok's generated constructor injects the final fields of a bean class
                // — the constructor never appears in the source, so the field is the only
                // place this can be shown.
                let lombok_injected = is_bean
                    && f.is_final
                    && t.annotations
                        .iter()
                        .any(|a| crate::known::is_any(a, &u.facts, LOMBOK_CTORS).is_some());
                if !injected && !lombok_injected {
                    continue;
                }
                out.push(InjectionPoint {
                    owner_fqcn: t.fqcn.clone(),
                    member: f.name.clone(),
                    type_text: f.type_text.clone(),
                    qualifier: ann_value(&f.annotations, &u.facts, "Qualifier"),
                    kind: if injected { InjectionKind::Field } else { InjectionKind::Constructor },
                    file: u.facts.file.clone(),
                    offset: f.name_offset,
                    line: line_at(&u.text, f.name_offset),
                });
            }

            // Constructors: an explicitly annotated one, or — since Spring 4.3 — the only
            // constructor of a bean class, which needs no annotation at all.
            let ctors: Vec<_> = t.methods.iter().filter(|m| m.is_constructor).collect();
            let single_ctor = ctors.len() == 1 && is_bean;
            for c in &ctors {
                if !has(&c.annotations, &u.facts, "Autowired")
                    && !has(&c.annotations, &u.facts, "Inject")
                    && !single_ctor
                {
                    continue;
                }
                for p in &c.params {
                    out.push(InjectionPoint {
                        owner_fqcn: t.fqcn.clone(),
                        member: p.name.clone(),
                        type_text: p.type_text.clone(),
                        qualifier: ann_value(&p.annotations, &u.facts, "Qualifier"),
                        kind: InjectionKind::Constructor,
                        file: u.facts.file.clone(),
                        offset: p.name_offset,
                        line: line_at(&u.text, p.name_offset),
                    });
                }
            }

            // Annotated setters.
            for m in &t.methods {
                if m.is_constructor
                    || !m
                        .annotations
                        .iter()
                        .any(|a| crate::known::is_any(a, &u.facts, INJECT_MARKERS).is_some())
                {
                    continue;
                }
                for p in &m.params {
                    // A qualifier may sit on the parameter or on the setter itself.
                    let param_q = ann_value(&p.annotations, &u.facts, "Qualifier");
                    let qualifier = if param_q.is_empty() {
                        ann_value(&m.annotations, &u.facts, "Qualifier")
                    } else {
                        param_q
                    };
                    out.push(InjectionPoint {
                        owner_fqcn: t.fqcn.clone(),
                        member: p.name.clone(),
                        type_text: p.type_text.clone(),
                        qualifier,
                        kind: InjectionKind::Setter,
                        file: u.facts.file.clone(),
                        offset: p.name_offset,
                        line: line_at(&u.text, p.name_offset),
                    });
                }
            }
        }
    }
    out
}

/// The project's type index, with the writable-property set each `<property name=>` is
/// checked against. Supertype properties are folded in; a supertype outside the scan
/// clears [`TypeInfo::properties_complete`] instead.
pub fn type_index(units: &[JavaUnit]) -> BTreeMap<String, TypeInfo> {
    let mut index: BTreeMap<String, TypeInfo> = BTreeMap::new();
    // fqcn → the supertype as resolved, for the merge pass.
    let mut parents: BTreeMap<String, String> = BTreeMap::new();

    for u in units {
        for t in &u.facts.types {
            let (properties, property_offsets, modelled) = own_properties(t, &u.facts);
            if !t.extends.is_empty() {
                parents.insert(t.fqcn.clone(), resolve_type(&t.extends, &u.facts));
            }
            index.insert(
                t.fqcn.clone(),
                TypeInfo {
                    fqcn: t.fqcn.clone(),
                    file: u.facts.file.clone(),
                    offset: t.name_offset,
                    line: line_at(&u.text, t.name_offset),
                    properties,
                    property_offsets,
                    properties_complete: modelled,
                },
            );
        }
    }

    // Fold each supertype's properties down. A parent we never scanned means the set is
    // incomplete, and the flag says so rather than the list pretending otherwise.
    let fqcns: Vec<String> = index.keys().cloned().collect();
    for fqcn in fqcns {
        let mut merged: Vec<(String, usize)> = Vec::new();
        let mut complete = index[&fqcn].properties_complete;
        let mut cur = parents.get(&fqcn).cloned();
        let mut depth = 0;
        while let Some(parent) = cur {
            depth += 1;
            if depth > 16 {
                complete = false;
                break;
            }
            match lookup_by_name(&index, &parent) {
                Some(p) => {
                    complete &= p.properties_complete;
                    merged.extend(
                        p.properties.iter().cloned().zip(p.property_offsets.iter().copied()),
                    );
                    cur = parents.get(&p.fqcn).cloned();
                }
                // Not a project type: a framework base class, a library. We cannot know
                // what it contributes.
                None => {
                    complete = false;
                    break;
                }
            }
        }
        let entry = index.get_mut(&fqcn).expect("iterating our own keys");
        for (name, offset) in merged {
            if !entry.properties.contains(&name) {
                entry.properties.push(name);
                entry.property_offsets.push(offset);
            }
        }
        entry.properties_complete = complete;
    }
    index
}

/// Look a type up by FQCN, falling back to a UNIQUE simple-name match — a supertype is
/// usually written unqualified, and two classes with the same simple name make the answer
/// ambiguous rather than wrong.
fn lookup_by_name<'a>(
    index: &'a BTreeMap<String, TypeInfo>,
    name: &str,
) -> Option<&'a TypeInfo> {
    if let Some(t) = index.get(name) {
        return Some(t);
    }
    let simple = simple_name(name);
    let mut hits = index.values().filter(|t| simple_name(&t.fqcn) == simple);
    let first = hits.next()?;
    hits.next().is_none().then_some(first)
}

/// The properties a type declares itself: `setX` methods, public instance fields, and the
/// Lombok-generated setters. The third value is whether the set can be trusted as complete
/// — false when the type carries a Lombok annotation whose generated members we do not
/// model.
fn own_properties(t: &TypeFacts, facts: &JavaFacts) -> (Vec<String>, Vec<usize>, bool) {
    /// Record a property once — a field with both an explicit setter and a Lombok one is
    /// still one property, and the FIRST site recorded is the one go-to lands on.
    fn push(name: String, offset: usize, names: &mut Vec<String>, offsets: &mut Vec<usize>) {
        if !names.contains(&name) {
            names.push(name);
            offsets.push(offset);
        }
    }

    let mut names = Vec::new();
    let mut offsets = Vec::new();
    for m in &t.methods {
        if m.is_constructor || m.params.len() != 1 || !m.name.starts_with("set") || m.name.len() < 4
        {
            continue;
        }
        push(decapitalize(&m.name[3..]), m.name_offset, &mut names, &mut offsets);
    }

    // Lombok's own `@Data`/`@Setter`, verified through the imports: this ADDS properties, and a
    // same-named annotation from elsewhere generates nothing.
    let class_setter =
        has(&t.annotations, facts, "Data") || has(&t.annotations, facts, "Setter");
    for f in &t.fields {
        if f.is_static {
            continue;
        }
        if f.is_public {
            push(f.name.clone(), f.name_offset, &mut names, &mut offsets);
            continue;
        }
        // Lombok writes a setter for a non-final field when the class or the field asks.
        if !f.is_final && (class_setter || has(&f.annotations, facts, "Setter")) {
            push(f.name.clone(), f.name_offset, &mut names, &mut offsets);
        }
    }

    // `@Accessors` changes the generated names wholesale (fluent / prefix), and any other
    // unmodelled Lombok annotation may add members we cannot see. Either way the set stops
    // being authoritative — and only the authoritative case is ever reported on.
    //
    // This test stays on the NAME alone, deliberately: it decides whether to DISTRUST the
    // set, so a same-named annotation from another package costs a check that goes quiet.
    // That is the safe direction, and the opposite of the checks above.
    let complete = !t.annotations.iter().any(|a| a.name == "Accessors")
        && t.annotations.iter().all(|a| !is_lombok_ish(&a.name) || LOMBOK_MODELLED.contains(&a.name.as_str()));
    (names, offsets, complete)
}

/// Whether an annotation name is one Lombok is known to own. Used only to decide whether
/// an *unmodelled* one should make the property set untrustworthy, so the list is
/// deliberately about Lombok's shape rather than about a full catalogue.
fn is_lombok_ish(name: &str) -> bool {
    matches!(
        name,
        "Data"
            | "Value"
            | "Setter"
            | "Getter"
            | "Builder"
            | "SuperBuilder"
            | "With"
            | "Accessors"
            | "FieldDefaults"
            | "FieldNameConstants"
            | "Delegate"
            | "UtilityClass"
            | "NoArgsConstructor"
            | "AllArgsConstructor"
            | "RequiredArgsConstructor"
            | "ToString"
            | "EqualsAndHashCode"
            | "Slf4j"
            | "NonNull"
            | "SneakyThrows"
    )
}

fn decapitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_java;

    /// The imports a real Spring/Lombok file carries, on ONE line.
    ///
    /// Needed because `known` resolves each annotation's origin through the imports — a
    /// fixture without them is declaring its *own* `@Service`, which is exactly what that
    /// check rejects. One line so injecting it never shifts a line number a test asserts on.
    const IMPORTS: &str = "import org.springframework.stereotype.*; import org.springframework.context.annotation.*; import org.springframework.beans.factory.annotation.*; import lombok.*; import lombok.experimental.*;";

    /// Splice [`IMPORTS`] onto the `package` line (or the front, when there is none).
    fn with_imports(src: &str) -> String {
        match src.find('\n') {
            Some(nl) if src.trim_start().starts_with("package") => {
                format!("{}{IMPORTS}{}", &src[..nl], &src[nl..])
            }
            _ => format!("{IMPORTS}\n{src}"),
        }
    }

    fn unit(src: &str) -> JavaUnit {
        let text = with_imports(src);
        JavaUnit {
            facts: scan_java("/p/src/main/java/com/acme/T.java", &text).unwrap(),
            text,
        }
    }

    #[test]
    fn stereotype_beans_take_the_convention_name_unless_told_otherwise() {
        let u = unit(
            "package com.acme;\n@Service class OrderService {}\n@Repository(\"repo\") class OrderDao {}\n",
        );
        let beans = annotation_beans(&[u]);
        assert_eq!(beans[0].name, "orderService");
        assert_eq!(beans[0].fqcn, "com.acme.OrderService");
        assert_eq!(beans[0].stereotype, "@Service");
        assert_eq!(beans[1].name, "repo", "the explicit value wins");
    }

    #[test]
    fn a_projects_own_service_annotation_declares_no_bean() {
        // `@Service` is not a reserved word. This class carries an annotation with the same
        // simple name from the project's own package, and it must not become a Spring bean —
        // note the fixture bypasses `unit`, because the imports are the whole point.
        let src = "package com.acme;\nimport com.acme.annotations.Service;\n@Service class NotABean {}\n";
        let u = JavaUnit { facts: scan_java("/p/T.java", src).unwrap(), text: src.to_string() };
        assert!(annotation_beans(&[u]).is_empty());
    }

    #[test]
    fn an_interface_or_abstract_class_is_not_itself_a_bean() {
        let u = unit("package p;\n@Service interface Svc {}\n@Service abstract class Base {}\n");
        assert!(annotation_beans(&[u]).is_empty());
    }

    #[test]
    fn scope_primary_profile_and_lazy_are_carried() {
        let u = unit(
            "package p;\n@Service @Primary @Lazy @Scope(\"prototype\") @Profile(\"dev\") class S {}\n",
        );
        let b = &annotation_beans(&[u])[0];
        assert_eq!(b.scope, "prototype");
        assert_eq!(b.profile, "dev");
        assert!(b.primary && b.lazy);
    }

    #[test]
    fn bean_factory_methods_are_registered_by_method_name() {
        let u = unit(
            "package p;\nimport com.acme.Clock;\n@Configuration class Cfg {\n  @Bean Clock clock() { return null; }\n  @Bean(\"named\") Clock other() { return null; }\n}\n",
        );
        let beans = annotation_beans(&[u]);
        let clock = beans.iter().find(|b| b.name == "clock").expect("method-named bean");
        assert_eq!(clock.fqcn, "com.acme.Clock", "resolved through the import");
        assert_eq!(clock.stereotype, "@Bean");
        assert!(beans.iter().any(|b| b.name == "named"));
    }

    #[test]
    fn supertypes_make_an_impl_findable_by_its_interface() {
        let u = unit(
            "package com.acme;\n@Service class OrderServiceImpl implements OrderService, Auditable {}\n",
        );
        let b = &annotation_beans(&[u])[0];
        assert_eq!(b.supertypes, ["com.acme.OrderService", "com.acme.Auditable"]);
    }

    #[test]
    fn annotated_fields_constructors_and_setters_are_injection_points() {
        let u = unit(
            "package p;\n@Service class S {\n  @Autowired private Repo repo;\n  @Autowired void setClock(Clock c) {}\n  S(Dao dao) {}\n}\n",
        );
        let inj = injection_points(&[u]);
        let by_member = |n: &str| inj.iter().find(|i| i.member == n).cloned();
        assert_eq!(by_member("repo").unwrap().kind, InjectionKind::Field);
        assert_eq!(by_member("c").unwrap().kind, InjectionKind::Setter);
        // The single constructor of a bean class needs no annotation (Spring 4.3+).
        assert_eq!(by_member("dao").unwrap().kind, InjectionKind::Constructor);
    }

    #[test]
    fn a_lombok_generated_constructor_makes_final_fields_injection_points() {
        // The user's own style: no constructor is written, so the field is the only place
        // this can possibly be shown.
        let u = unit(
            "package p;\n@Service @RequiredArgsConstructor class S {\n  private final Repo repo;\n  private String notInjected;\n}\n",
        );
        let inj = injection_points(&[u]);
        assert_eq!(inj.len(), 1);
        assert_eq!(inj[0].member, "repo");
        assert_eq!(inj[0].kind, InjectionKind::Constructor);
    }

    #[test]
    fn a_qualifier_rides_along_with_its_injection_point() {
        let u = unit(
            "package p;\n@Service class S {\n  @Autowired @Qualifier(\"fast\") private Engine e;\n}\n",
        );
        assert_eq!(injection_points(&[u])[0].qualifier, "fast");
    }

    #[test]
    fn a_plain_class_constructor_is_not_an_injection_point() {
        let u = unit("package p;\nclass NotABean { NotABean(Dao d) {} }\n");
        assert!(injection_points(&[u]).is_empty());
    }

    #[test]
    fn xml_beans_follow_the_parent_chain_for_their_class() {
        let f = crate::xml::parse_bean_xml(
            "/p/beans.xml",
            r#"<beans>
                 <bean id="base" abstract="true" class="com.acme.Base"/>
                 <bean id="child" parent="base"/>
                 <bean class="com.acme.Anon"/>
               </beans>"#,
        )
        .unwrap();
        let beans = xml_beans(&[f]);
        assert_eq!(beans.len(), 2, "the anonymous bean has no name to register");
        assert_eq!(beans[1].name, "child");
        assert_eq!(beans[1].fqcn, "com.acme.Base", "inherited from the parent");
        assert!(beans[0].is_abstract);
    }

    #[test]
    fn writable_properties_come_from_setters_public_fields_and_lombok() {
        let idx = type_index(&[unit(
            "package p;\n@Data class Bean {\n  private String name;\n  private final String id;\n  public int count;\n  private String other;\n  public void setOther(String o) {}\n}\n",
        )]);
        let t = &idx["p.Bean"];
        let mut props = t.properties.clone();
        props.sort();
        assert_eq!(props, ["count", "name", "other"], "no setter for a final field");
        assert!(t.properties_complete);
    }

    #[test]
    fn an_unresolved_supertype_marks_the_property_set_incomplete() {
        // `HttpServlet` is not in the scan, so what it contributes is unknown — and the
        // `<property name=>` check must stay silent rather than guess.
        let idx = type_index(&[unit("package p;\nclass A extends HttpServlet { public void setX(String s) {} }\n")]);
        assert!(!idx["p.A"].properties_complete);
        assert!(idx["p.A"].properties.contains(&"x".to_string()));
    }

    #[test]
    fn a_scanned_supertype_folds_its_properties_down() {
        let child = "package p;\nclass Child extends Base { public void setOwn(String s) {} }\n";
        let units = vec![
            unit("package p;\nclass Base { public void setShared(String s) {} }\n"),
            JavaUnit {
                facts: scan_java("/p/B.java", child).unwrap(),
                text: child.to_string(),
            },
        ];
        let idx = type_index(&units);
        let child = &idx["p.Child"];
        assert!(child.properties.contains(&"own".to_string()));
        assert!(child.properties.contains(&"shared".to_string()));
        assert!(child.properties_complete);
    }

    #[test]
    fn accessors_makes_the_property_set_untrustworthy() {
        // `@Accessors(fluent = true)` renames every generated accessor; we do not model
        // that, so nothing may be reported against this class.
        let idx = type_index(&[unit(
            "package p;\n@Data @Accessors(fluent = true) class B { private String ngara; }\n",
        )]);
        assert!(!idx["p.B"].properties_complete);
    }

    #[test]
    fn type_resolution_uses_imports_then_falls_back_to_the_simple_name() {
        let f = scan_java(
            "/p/T.java",
            "package com.acme;\nimport com.other.Clock;\nclass T { }\n",
        )
        .unwrap();
        assert_eq!(resolve_type("Clock", &f), "com.other.Clock");
        assert_eq!(resolve_type("List<Foo>", &f), "List", "generics are stripped first");
        assert_eq!(resolve_type("Unknown", &f), "Unknown", "never invents a package");
        assert_eq!(resolve_type("java.util.Map", &f), "java.util.Map");
        assert_eq!(resolve_type("T", &f), "com.acme.T", "declared here → this package");
    }
}
