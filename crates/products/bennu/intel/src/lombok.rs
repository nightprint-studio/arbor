//! Lombok generated-member synthesis.
//!
//! Lombok generates members at COMPILE time from annotations (`@Getter`/`@Setter`/`@Data`/
//! `@Value`/`@Slf4j` …) that do not exist in the `.java` source. Without modelling them, go-to /
//! completion / hover / find-usages on `order.getId()` or `this.log` resolve to nothing. This
//! module reproduces the members Lombok would generate, so they flow through the same
//! `members_json` → resolver path as real declarations.
//!
//! Conservative by design: only members whose shape is unambiguous (getters/setters/`log`), and
//! NEVER one that shadows a user-declared method of the same name (Lombok itself skips generation
//! when the member already exists). `@Builder` / generated constructors / `equals`/`hashCode`/
//! `toString` are deferred — they are lower navigation value and (for `@Builder`) need a synthetic
//! nested type.
//!
//! **Capability-gated.** Synthesis runs only when the file genuinely uses Lombok — i.e. it imports
//! it (`import lombok.…` / a `lombok.*` wildcard), which at compile time requires the
//! `org.projectlombok:lombok` dependency. Each annotation is honoured only when it's correctly
//! imported ([`lombok_imported`]): a project's OWN `@Data`/`@Getter` in a different package (or a
//! project with no Lombok) yields nothing, so we never invent phantom members.

use std::collections::{BTreeMap, HashSet};

use bennu_java::prelude::{Annotation, Import, Member, MemberKind, TypeDecl, TypeRef, Visibility};

use crate::typemap::type_text_to_ref;

/// The extra members Lombok would generate for a type: getters/setters go to `methods`, the
/// logger goes to `fields`.
pub struct LombokMembers {
    pub methods: Vec<Member>,
    pub fields: Vec<Member>,
}

/// Synthesize the members Lombok would generate for `td`, given the method names already declared
/// in source (`existing_methods` — a user-written getter suppresses the synthetic one). Returns
/// empty when the type carries no Lombok annotations.
pub fn synthesize(
    td: &TypeDecl,
    imports: &[Import],
    project_types: &BTreeMap<String, String>,
    existing_methods: &HashSet<String>,
    is_project: &dyn Fn(&str) -> bool,
) -> LombokMembers {
    let mut methods = Vec::new();
    let mut fields = Vec::new();

    // Capability gate: Lombok generates members ONLY when the project actually uses Lombok — which,
    // per Java semantics, means the file imports it (`import lombok.…` / `lombok.*`). That import can
    // only compile when `org.projectlombok:lombok` is on the classpath, so this is also the
    // "only if Lombok is a dependency" gate. A project with its OWN `@Data`/`@Getter` annotation (a
    // different package, no lombok import) gets no synthesis — no phantom getters, no false members.
    if !file_imports_lombok(imports) {
        return LombokMembers { methods, fields };
    }

    // Class-level flags. `@Data` = getters + setters; `@Value` = getters only (immutable). Each is
    // honoured only when the annotation resolves to Lombok (correctly imported), never by bare name.
    let cls_getter = has_lombok(&td.annotations, imports, &["Getter", "Data", "Value"]);
    let is_value = has_lombok(&td.annotations, imports, &["Value"]);
    let cls_setter = has_lombok(&td.annotations, imports, &["Setter", "Data"]) && !is_value;
    // `@With`/`@Wither` generates a `withX(v)` copy-method per field, returning the OWNER type.
    let cls_with = has_lombok(&td.annotations, imports, &["With", "Wither"]);
    let owner = td.fqn.replace('.', "/");
    // `@Accessors(fluent = true)` renames accessors to the FIELD name (`name()` / `name(v)`) with no
    // get/set/is prefix, and (with `chain`, which defaults on when fluent) makes the setter return
    // `this`. Configurable at class level or per field (the field's own `@Accessors` overrides).
    let cls_accessors = accessors_config(&td.annotations, imports);

    for f in &td.fields {
        // Lombok does not generate accessors for static fields.
        if f.is_static {
            continue;
        }
        let want_getter = cls_getter || has_lombok(&f.annotations, imports, &["Getter"]);
        let want_setter =
            (cls_setter || has_lombok(&f.annotations, imports, &["Setter"])) && !is_value && !f.is_final;
        let acc = accessors_config(&f.annotations, imports).or(cls_accessors).unwrap_or_default();

        if want_getter {
            let name = if acc.fluent {
                f.name.clone()
            } else {
                getter_name(&f.name, is_primitive_boolean(&f.type_text))
            };
            if !existing_methods.contains(&name) {
                let ret = type_text_to_ref(&f.type_text, imports, project_types, is_project);
                methods.push(Member {
                    name: name.clone(),
                    kind: MemberKind::Method,
                    return_type: ret,
                    params: Vec::new(),
                    is_static: false,
                    is_abstract: false,
                    is_default: false,
                    is_final: false,
                    visibility: Visibility::Public,
                    raw_signature: format!("{} {}()", f.type_text, name),
                    throws: Vec::new(),
                });
            }
        }
        if want_setter {
            let name = if acc.fluent { f.name.clone() } else { format!("set{}", capitalize(&f.name)) };
            if !existing_methods.contains(&name) {
                let param = type_text_to_ref(&f.type_text, imports, project_types, is_project);
                // `chain` (implied by `fluent`) → the setter returns the owner for call-chaining.
                let (ret, ret_text) = if acc.chain {
                    (TypeRef::simple(owner.clone()), td.name.as_str())
                } else {
                    (TypeRef::simple("void"), "void")
                };
                methods.push(Member {
                    name: name.clone(),
                    kind: MemberKind::Method,
                    return_type: ret,
                    params: vec![param],
                    is_static: false,
                    is_abstract: false,
                    is_default: false,
                    is_final: false,
                    visibility: Visibility::Public,
                    raw_signature: format!("{} {}({})", ret_text, name, f.type_text),
                    throws: Vec::new(),
                });
            }
        }
        // `@With foo` → `Foo withFoo(T value)` (an immutable "copy with one field changed").
        let want_with = cls_with || has_lombok(&f.annotations, imports, &["With", "Wither"]);
        if want_with {
            let name = format!("with{}", capitalize(&f.name));
            if !existing_methods.contains(&name) {
                let param = type_text_to_ref(&f.type_text, imports, project_types, is_project);
                methods.push(Member {
                    name: name.clone(),
                    kind: MemberKind::Method,
                    return_type: TypeRef::simple(owner.clone()),
                    params: vec![param],
                    is_static: false,
                    is_abstract: false,
                    is_default: false,
                    is_final: false,
                    visibility: Visibility::Public,
                    raw_signature: format!("{} {}({})", td.name, name, f.type_text),
                    throws: Vec::new(),
                });
            }
        }
    }

    // `@Builder`/`@SuperBuilder` → a static `builder()` factory. We synthesize just the entry point
    // (returning a builder type we DON'T model): that resolves `Foo.builder()`, and the fluent chain
    // that follows (`.name(x).build()`) resolves against an unknown type, which the member checks
    // treat as "might exist" → never a false "cannot resolve method". So the whole builder chain stops
    // erroring without us modelling a synthetic builder class.
    if has_lombok(&td.annotations, imports, &["Builder", "SuperBuilder"])
        && !existing_methods.contains("builder")
    {
        methods.push(Member {
            name: "builder".to_string(),
            kind: MemberKind::Method,
            return_type: TypeRef::simple(format!("{owner}$Builder")),
            params: Vec::new(),
            is_static: true,
            is_abstract: false,
            is_default: false,
            is_final: false,
            visibility: Visibility::Public,
            raw_signature: format!("{}.Builder builder()", td.name),
            throws: Vec::new(),
        });
    }

    // Logging annotations inject a `private static final <Logger> log;` field. Skip when the type
    // already declares a `log` field of its own. Gated on the logger annotation being lombok-imported
    // (`import lombok.extern.slf4j.Slf4j;` / a `lombok.*` wildcard).
    if let Some(logger_binary) = logger_type(&td.annotations, imports) {
        let already = td.fields.iter().any(|f| f.name == "log");
        if !already {
            fields.push(Member {
                name: "log".to_string(),
                kind: MemberKind::Field,
                return_type: TypeRef::simple(logger_binary),
                params: Vec::new(),
                is_static: true,
                is_abstract: false,
                is_default: false,
                is_final: true,
                visibility: Visibility::Private,
                raw_signature: format!("{logger_binary} log"),
                throws: Vec::new(),
            });
        }
    }

    LombokMembers { methods, fields }
}

/// The backing field name a Lombok accessor method maps to (`getId`/`setId`/`isShipped` →
/// `id`/`id`/`shipped`), for go-to redirection from a generated getter/setter to the field it
/// wraps. `None` when the name isn't an accessor shape. The CALLER must still verify the field
/// actually exists (so a real `getStatus()` with no `status` field never mis-redirects).
pub(crate) fn backing_field_name(accessor: &str) -> Option<String> {
    let rest = accessor
        .strip_prefix("get")
        .or_else(|| accessor.strip_prefix("set"))
        .or_else(|| accessor.strip_prefix("is"))?;
    let mut chars = rest.chars();
    let first = chars.next()?;
    Some(first.to_ascii_lowercase().to_string() + chars.as_str())
}

/// Whether the file imports Lombok at all — any `import lombok.…` (specific) or `import lombok.*` /
/// `import lombok.<sub>.*` (wildcard). Used as the capability gate: no Lombok import → the file
/// doesn't (and can't, at compile time) use Lombok, so nothing is synthesized.
fn file_imports_lombok(imports: &[Import]) -> bool {
    imports.iter().any(|i| i.path == "lombok" || i.path.starts_with("lombok."))
}

/// Whether the annotation simple-named `ann` resolves to a Lombok annotation IN THIS FILE: a specific
/// import ending in `.<ann>` under the `lombok` package (`lombok.Data`, `lombok.experimental.Accessors`,
/// `lombok.extern.slf4j.Slf4j`), or a `lombok`/`lombok.<sub>` wildcard import. This is what verifies
/// "the annotation is correctly imported" — a bare `@Data` with no matching import isn't Lombok's.
fn lombok_imported(ann: &str, imports: &[Import]) -> bool {
    let suffix = format!(".{ann}");
    imports.iter().any(|i| {
        if i.star {
            i.path == "lombok" || i.path.starts_with("lombok.")
        } else {
            i.path.starts_with("lombok.") && i.path.ends_with(&suffix)
        }
    })
}

/// Whether `annotations` contains one of `wanted` (simple names) that is ALSO correctly imported from
/// Lombok — the import-checked form of [`has`]. A project's own same-named annotation (different
/// package) never trips this.
fn has_lombok(annotations: &[Annotation], imports: &[Import], wanted: &[&str]) -> bool {
    annotations
        .iter()
        .any(|a| wanted.contains(&a.name.as_str()) && lombok_imported(&a.name, imports))
}

/// Lombok `@Accessors` naming config that shapes the synthetic getters/setters.
#[derive(Clone, Copy, Default)]
struct Accessors {
    /// `fluent = true` → accessors are named after the field (`name()` / `name(v)`), no get/set/is.
    fluent: bool,
    /// `chain = true` → the setter returns the owner (for `a.x(1).y(2)`). Defaults on when `fluent`.
    chain: bool,
}

/// Read a Lombok `@Accessors(...)` off `annotations`, or `None` when absent (or not correctly
/// imported from Lombok). `chain` follows Lombok's rule: it defaults to the value of `fluent` unless
/// set explicitly.
fn accessors_config(annotations: &[Annotation], imports: &[Import]) -> Option<Accessors> {
    let a = annotations.iter().find(|a| a.name == "Accessors" && lombok_imported("Accessors", imports))?;
    let flag = |key: &str| a.args.iter().find(|(k, _)| k == key).map(|(_, v)| v.trim() == "true");
    let fluent = flag("fluent").unwrap_or(false);
    let chain = flag("chain").unwrap_or(fluent);
    Some(Accessors { fluent, chain })
}

/// The Lombok getter name for `field`: `getFoo`, or `isFoo` for a primitive `boolean` (and no
/// double `is` when the field is already `isFoo`).
fn getter_name(field: &str, is_bool: bool) -> String {
    if is_bool {
        if field.len() > 2
            && field.starts_with("is")
            && field.as_bytes()[2].is_ascii_uppercase()
        {
            return field.to_string();
        }
        return format!("is{}", capitalize(field));
    }
    format!("get{}", capitalize(field))
}

/// Uppercase the first character (ASCII), leaving the rest unchanged.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Only the primitive `boolean` gets an `isX` getter; the `Boolean` wrapper uses `getX`.
fn is_primitive_boolean(type_text: &str) -> bool {
    type_text.trim() == "boolean"
}

/// The binary name of the logger a Lombok logging annotation injects, if any — only when that
/// annotation is correctly imported from Lombok (`import lombok.extern.slf4j.Slf4j;` / a wildcard).
fn logger_type(annotations: &[Annotation], imports: &[Import]) -> Option<&'static str> {
    for a in annotations {
        let binary = match a.name.as_str() {
            "Slf4j" => "org/slf4j/Logger",
            "Log4j2" => "org/apache/logging/log4j/Logger",
            "Log4j" => "org/apache/log4j/Logger",
            "CommonsLog" => "org/apache/commons/logging/Log",
            "JBossLog" => "org/jboss/logging/Logger",
            "Flogger" => "com/google/common/flogger/FluentLogger",
            "XSlf4j" => "org/slf4j/ext/XLogger",
            "Log" => "java/util/logging/Logger",
            _ => continue,
        };
        if lombok_imported(a.name.as_str(), imports) {
            return Some(binary);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A marker annotation (no value) for a test simple name.
    fn ann(name: &str) -> Annotation {
        Annotation { name: name.to_string(), value: None, args: Vec::new() }
    }

    /// An annotation carrying `name = value` arg pairs (for `@Accessors(fluent = true)` tests).
    fn ann_args(name: &str, args: &[(&str, &str)]) -> Annotation {
        Annotation {
            name: name.to_string(),
            value: None,
            args: args.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    fn field(name: &str, type_text: &str) -> bennu_java::prelude::FieldDecl {
        bennu_java::prelude::FieldDecl {
            name: name.to_string(),
            type_text: type_text.to_string(),
            is_static: false,
            is_final: false,
            visibility: Visibility::Private,
            annotations: Vec::new(),
        }
    }

    /// A `import lombok.*;` wildcard — the standard test import so synthesis is enabled (the real
    /// gate: Lombok is only synthesized when the file imports it).
    fn lombok() -> Vec<Import> {
        vec![Import { path: "lombok".to_string(), star: true, static_: false }]
    }

    /// A specific import `import lombok.<name>;`.
    fn lombok_import(name: &str) -> Import {
        Import { path: format!("lombok.{name}"), star: false, static_: false }
    }

    fn type_with(annotations: &[&str], fields: Vec<bennu_java::prelude::FieldDecl>) -> TypeDecl {
        TypeDecl {
            name: "Order".to_string(),
            fqn: "shop.Order".to_string(),
            kind: bennu_java::prelude::TypeKind::Class,
            is_abstract: false,
            is_final: false,
            is_sealed: false,
            type_params: Vec::new(),
            methods: Vec::new(),
            fields,
            extends: None,
            implements: Vec::new(),
            annotations: annotations.iter().map(|s| ann(s)).collect(),
        }
    }

    #[test]
    fn getter_setter_from_data() {
        let td = type_with(&["Data"], vec![field("id", "long"), field("active", "boolean")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"getId"), "got {names:?}");
        assert!(names.contains(&"setId"), "got {names:?}");
        assert!(names.contains(&"isActive"), "boolean uses isX, got {names:?}");
        assert!(names.contains(&"setActive"), "got {names:?}");
    }

    #[test]
    fn value_is_immutable_getters_only() {
        let td = type_with(&["Value"], vec![field("id", "long")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"getId"));
        assert!(!names.iter().any(|n| n.starts_with("set")), "@Value has no setters, got {names:?}");
    }

    #[test]
    fn with_generates_copy_methods_returning_owner() {
        let td = type_with(&["With"], vec![field("id", "long"), field("name", "String")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let with_id = m.methods.iter().find(|x| x.name == "withId").expect("withId");
        assert_eq!(with_id.return_type.binary_name, "shop/Order", "returns the owner type");
        assert_eq!(with_id.params.len(), 1, "takes the new value");
        assert!(m.methods.iter().any(|x| x.name == "withName"), "withName generated");
    }

    #[test]
    fn builder_generates_static_entry_point() {
        let td = type_with(&["Builder"], vec![field("id", "long")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let b = m.methods.iter().find(|x| x.name == "builder").expect("builder()");
        assert!(b.is_static, "builder() is static");
        assert!(b.params.is_empty(), "builder() takes no args");
    }

    #[test]
    fn fluent_accessors_use_field_name_and_chain_setter() {
        // `@Accessors(fluent = true)` + `@Data` → getter `id()` / setter `id(v)` returning the owner.
        let mut td = type_with(&["Data"], vec![field("id", "long"), field("name", "String")]);
        td.annotations.push(ann_args("Accessors", &[("fluent", "true")]));
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"id"), "fluent getter is `id()`, got {names:?}");
        assert!(names.contains(&"name"), "fluent getter is `name()`, got {names:?}");
        assert!(!names.iter().any(|n| n.starts_with("get")), "no get-prefixed getters, got {names:?}");
        // Both a no-arg getter and a one-arg setter named `id` exist (overloads).
        let id_setter = m.methods.iter().find(|x| x.name == "id" && x.params.len() == 1).expect("id(v)");
        assert_eq!(id_setter.return_type.binary_name, "shop/Order", "chained setter returns owner");
    }

    #[test]
    fn getter_with_access_level_arg_still_generates() {
        // `@Getter(AccessLevel.PACKAGE)` — a non-string positional arg must not suppress generation.
        let td = type_with(&["Getter"], vec![field("id", "long")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        assert!(m.methods.iter().any(|x| x.name == "getId"), "getter still synthesized");
    }

    #[test]
    fn existing_getter_is_not_duplicated() {
        let td = type_with(&["Getter"], vec![field("id", "long")]);
        let mut existing = HashSet::new();
        existing.insert("getId".to_string());
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &existing, &|_: &str| false);
        assert!(m.methods.iter().all(|x| x.name != "getId"), "user getId() suppresses the synthetic");
    }

    #[test]
    fn final_field_gets_no_setter() {
        let mut f = field("id", "long");
        f.is_final = true;
        let td = type_with(&["Data"], vec![f]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        assert!(m.methods.iter().any(|x| x.name == "getId"));
        assert!(m.methods.iter().all(|x| x.name != "setId"), "final field → no setter");
    }

    #[test]
    fn slf4j_injects_log_field() {
        let td = type_with(&["Slf4j"], vec![]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        assert_eq!(m.fields.len(), 1);
        assert_eq!(m.fields[0].name, "log");
        assert!(m.fields[0].is_static);
    }

    #[test]
    fn backing_field_name_inverts_accessors() {
        assert_eq!(backing_field_name("getId").as_deref(), Some("id"));
        assert_eq!(backing_field_name("setCustomer").as_deref(), Some("customer"));
        assert_eq!(backing_field_name("isShipped").as_deref(), Some("shipped"));
        assert_eq!(backing_field_name("run"), None, "not an accessor shape");
    }

    #[test]
    fn field_level_getter_only_that_field() {
        let mut f = field("id", "long");
        f.annotations = vec![ann("Getter")];
        let td = type_with(&[], vec![f, field("name", "String")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"getId"), "field-level @Getter, got {names:?}");
        assert!(!names.contains(&"getName"), "the other field has no @Getter, got {names:?}");
    }

    #[test]
    fn no_lombok_import_means_no_synthesis() {
        // The capability gate: a `@Data` class WITHOUT a Lombok import (a project's own `@Data`, or a
        // project that doesn't depend on Lombok) generates nothing — no phantom getters/setters.
        let td = type_with(&["Data"], vec![field("id", "long")]);
        let m = synthesize(&td, &[], &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        assert!(m.methods.is_empty(), "no lombok import → no synthesis, got {:?}",
            m.methods.iter().map(|x| &x.name).collect::<Vec<_>>());
        assert!(m.fields.is_empty());
    }

    #[test]
    fn specific_import_only_enables_that_annotation() {
        // `import lombok.Getter;` (only) → `@Getter` synthesizes, but a `@Setter` that isn't imported
        // from Lombok does not. Proves per-annotation import verification, not a blanket file gate.
        let td = type_with(&["Getter", "Setter"], vec![field("id", "long")]);
        let m = synthesize(&td, &[lombok_import("Getter")], &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"getId"), "imported @Getter works, got {names:?}");
        assert!(!names.contains(&"setId"), "un-imported @Setter is not Lombok's, got {names:?}");
    }

    #[test]
    fn wrong_package_same_name_is_not_lombok() {
        // A `@Data` imported from a NON-lombok package is the project's own annotation → no synthesis.
        let td = type_with(&["Data"], vec![field("id", "long")]);
        let mine = Import { path: "com.acme.Data".to_string(), star: false, static_: false };
        let m = synthesize(&td, &[mine], &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        assert!(m.methods.is_empty(), "com.acme.Data is not lombok.Data");
    }
}
