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

use std::collections::{BTreeMap, HashSet};

use bennu_java::prelude::{Import, Member, MemberKind, TypeDecl, TypeRef, Visibility};

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
) -> LombokMembers {
    let mut methods = Vec::new();
    let mut fields = Vec::new();

    // Class-level flags. `@Data` = getters + setters; `@Value` = getters only (immutable).
    let cls_getter = has(&td.annotations, &["Getter", "Data", "Value"]);
    let is_value = has(&td.annotations, &["Value"]);
    let cls_setter = has(&td.annotations, &["Setter", "Data"]) && !is_value;

    for f in &td.fields {
        // Lombok does not generate accessors for static fields.
        if f.is_static {
            continue;
        }
        let want_getter = cls_getter || has(&f.annotations, &["Getter"]);
        let want_setter =
            (cls_setter || has(&f.annotations, &["Setter"])) && !is_value && !f.is_final;

        if want_getter {
            let name = getter_name(&f.name, is_primitive_boolean(&f.type_text));
            if !existing_methods.contains(&name) {
                let ret = type_text_to_ref(&f.type_text, imports, project_types);
                methods.push(Member {
                    name: name.clone(),
                    kind: MemberKind::Method,
                    return_type: ret,
                    params: Vec::new(),
                    is_static: false,
                    visibility: Visibility::Public,
                    raw_signature: format!("{} {}()", f.type_text, name),
                });
            }
        }
        if want_setter {
            let name = format!("set{}", capitalize(&f.name));
            if !existing_methods.contains(&name) {
                let param = type_text_to_ref(&f.type_text, imports, project_types);
                methods.push(Member {
                    name: name.clone(),
                    kind: MemberKind::Method,
                    return_type: TypeRef::simple("void"),
                    params: vec![param],
                    is_static: false,
                    visibility: Visibility::Public,
                    raw_signature: format!("void {}({})", name, f.type_text),
                });
            }
        }
    }

    // Logging annotations inject a `private static final <Logger> log;` field. Skip when the type
    // already declares a `log` field of its own.
    if let Some(logger_binary) = logger_type(&td.annotations) {
        let already = td.fields.iter().any(|f| f.name == "log");
        if !already {
            fields.push(Member {
                name: "log".to_string(),
                kind: MemberKind::Field,
                return_type: TypeRef::simple(logger_binary),
                params: Vec::new(),
                is_static: true,
                visibility: Visibility::Private,
                raw_signature: format!("{logger_binary} log"),
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

/// Whether `annotations` contains any of `wanted` (simple names).
fn has(annotations: &[String], wanted: &[&str]) -> bool {
    annotations.iter().any(|a| wanted.contains(&a.as_str()))
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

/// The binary name of the logger a Lombok logging annotation injects, if any.
fn logger_type(annotations: &[String]) -> Option<&'static str> {
    for a in annotations {
        let binary = match a.as_str() {
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
        return Some(binary);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn type_with(annotations: &[&str], fields: Vec<bennu_java::prelude::FieldDecl>) -> TypeDecl {
        TypeDecl {
            name: "Order".to_string(),
            fqn: "shop.Order".to_string(),
            methods: Vec::new(),
            fields,
            extends: None,
            implements: Vec::new(),
            annotations: annotations.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn getter_setter_from_data() {
        let td = type_with(&["Data"], vec![field("id", "long"), field("active", "boolean")]);
        let m = synthesize(&td, &[], &BTreeMap::new(), &HashSet::new());
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"getId"), "got {names:?}");
        assert!(names.contains(&"setId"), "got {names:?}");
        assert!(names.contains(&"isActive"), "boolean uses isX, got {names:?}");
        assert!(names.contains(&"setActive"), "got {names:?}");
    }

    #[test]
    fn value_is_immutable_getters_only() {
        let td = type_with(&["Value"], vec![field("id", "long")]);
        let m = synthesize(&td, &[], &BTreeMap::new(), &HashSet::new());
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"getId"));
        assert!(!names.iter().any(|n| n.starts_with("set")), "@Value has no setters, got {names:?}");
    }

    #[test]
    fn existing_getter_is_not_duplicated() {
        let td = type_with(&["Getter"], vec![field("id", "long")]);
        let mut existing = HashSet::new();
        existing.insert("getId".to_string());
        let m = synthesize(&td, &[], &BTreeMap::new(), &existing);
        assert!(m.methods.iter().all(|x| x.name != "getId"), "user getId() suppresses the synthetic");
    }

    #[test]
    fn final_field_gets_no_setter() {
        let mut f = field("id", "long");
        f.is_final = true;
        let td = type_with(&["Data"], vec![f]);
        let m = synthesize(&td, &[], &BTreeMap::new(), &HashSet::new());
        assert!(m.methods.iter().any(|x| x.name == "getId"));
        assert!(m.methods.iter().all(|x| x.name != "setId"), "final field → no setter");
    }

    #[test]
    fn slf4j_injects_log_field() {
        let td = type_with(&["Slf4j"], vec![]);
        let m = synthesize(&td, &[], &BTreeMap::new(), &HashSet::new());
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
        f.annotations = vec!["Getter".to_string()];
        let td = type_with(&[], vec![f, field("name", "String")]);
        let m = synthesize(&td, &[], &BTreeMap::new(), &HashSet::new());
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"getId"), "field-level @Getter, got {names:?}");
        assert!(!names.contains(&"getName"), "the other field has no @Getter, got {names:?}");
    }
}
