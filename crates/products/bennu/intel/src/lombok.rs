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

    // `@UtilityClass` generates a `private` constructor (one that throws) so the class can't be
    // instantiated. The rest of what it does — making every member `static` and the class `final` —
    // is a rewrite of existing members, applied by the caller (see [`is_utility_class`]).
    if is_utility_class(td, imports) && !existing_methods.contains("<init>") {
        methods.push(Member {
            name: "<init>".to_string(),
            kind: MemberKind::Method,
            return_type: type_text_to_ref("void", imports, project_types, is_project),
            params: Vec::new(),
            is_static: false,
            is_abstract: false,
            is_default: false,
            is_final: false,
            visibility: Visibility::Private,
            raw_signature: format!("private {}()", td.name),
            throws: Vec::new(),
        });
    }

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
            let name = if acc.fluent {
                f.name.clone()
            } else {
                // Same rule as the getter, so a `boolean isRunning` gets `setRunning` (Lombok runs
                // both through one `toAccessorName`).
                accessor_name("set", &f.name, is_primitive_boolean(&f.type_text))
            };
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
            let name = accessor_name("with", &f.name, is_primitive_boolean(&f.type_text));
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

/// The field names a Lombok accessor method could be wrapping, best guess first — for go-to
/// redirection from a generated accessor to the field it wraps. Empty when the name is no accessor
/// shape at all.
///
/// Several candidates rather than one, because [`accessor_name`]'s mapping is not injective: the
/// `is`-stripping rule for boolean fields means `setRunning` may wrap `isRunning`, and a getter can
/// carry the field's own name verbatim — either because the field was already `is`-prefixed
/// (`is_attivo()` wraps `is_attivo`) or because `@Accessors(fluent = true)` drops the prefix entirely
/// (`customer()` wraps `customer`).
///
/// The CALLER must still verify each candidate is a real field, and takes the first that is. That is
/// what keeps the extra candidates safe: a name with no matching field simply doesn't redirect. The
/// caller also only reaches here when no *declared* method has this name, so a hand-written `run()`
/// beside a field `run` is never redirected.
pub(crate) fn backing_field_candidates(accessor: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |name: String| {
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    };
    // The plain shape: drop the prefix, lowercase what's left (`getId` → `id`).
    for prefix in ["get", "set", "is", "with"] {
        if let Some(rest) = accessor.strip_prefix(prefix) {
            if let Some(first) = rest.chars().next() {
                add(first.to_ascii_lowercase().to_string() + &rest[first.len_utf8()..]);
            }
            // A boolean field that kept its own `is`: `setRunning`/`withRunning` wrap `isRunning`.
            if prefix != "is" && !rest.is_empty() {
                add(format!("is{rest}"));
            }
        }
    }
    // The accessor named exactly after its field — a fluent accessor, or an `is`-prefixed boolean
    // whose getter is the field name.
    add(accessor.to_string());
    out
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

/// Whether `td` is a Lombok **`@UtilityClass`**.
///
/// Unlike every other annotation this module handles, `@UtilityClass` mostly *rewrites* what is
/// already there rather than adding to it (JLS-visible effects, per Lombok's contract):
///
/// * every method becomes `static`,
/// * every field becomes `static`,
/// * every nested type becomes `static`,
/// * the class becomes `final`,
/// * a `private` constructor is generated (one that throws, so nobody instantiates it).
///
/// Only the constructor is a new member, so [`synthesize`] can't express the rest — the promotion
/// is applied by the member builder in `java_index`, which is what owns the `is_static` / `is_final`
/// mapping. Exported for that caller.
///
/// Gated on the import like everything else here: somebody's own `@UtilityClass` in another package
/// generates nothing, and inventing `static` where the compiler didn't would turn a correct
/// instance call into a false error — the exact mirror of the bug this fixes.
pub fn is_utility_class(td: &TypeDecl, imports: &[Import]) -> bool {
    file_imports_lombok(imports) && has_lombok(&td.annotations, imports, &["UtilityClass"])
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

/// Lombok's accessor-name rule (`HandlerUtil.toAccessorName`), for a non-fluent accessor. `prefix` is
/// `"get"` / `"is"` / `"set"` / `"with"`.
///
/// Normally the prefix is glued onto the capitalised field name. The wrinkle is a primitive `boolean`
/// field whose name **already begins with `is`**: Lombok strips that `is` before applying the prefix,
/// so `isRunning` yields `isRunning` and `setRunning` — not `isIsRunning` / `setIsRunning`.
///
/// The condition for stripping is "`is` followed by something that is not a **lowercase letter**",
/// which is wider than "followed by an uppercase letter". An underscore qualifies: a field named
/// `is_attivo` gets the getter `is_attivo()`, and reading the rule as *uppercase* instead produced
/// `isIs_attivo` — so a getter that genuinely exists was reported as unresolvable at every call site.
/// A field named `isattivo` (lowercase after `is`) does NOT strip: its getter is `isIsattivo`.
fn accessor_name(prefix: &str, field: &str, is_bool: bool) -> String {
    if is_bool && already_is_prefixed(field) {
        // Byte slicing is safe: `starts_with("is")` pins bytes 0..2 to ASCII, so 2 is a char boundary.
        return format!("{prefix}{}", &field[2..]);
    }
    format!("{prefix}{}", capitalize(field))
}

/// Whether a boolean field's name already carries the `is` that Lombok would otherwise prepend — `is`
/// followed by any character that is not a lowercase letter.
fn already_is_prefixed(field: &str) -> bool {
    field.starts_with("is") && field.chars().nth(2).is_some_and(|c| !c.is_lowercase())
}

/// The Lombok getter name for `field`: `getFoo`, or `isFoo` for a primitive `boolean`.
fn getter_name(field: &str, is_bool: bool) -> String {
    accessor_name(if is_bool { "is" } else { "get" }, field, is_bool)
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
    fn backing_field_candidates_invert_accessors() {
        assert_eq!(backing_field_candidates("getId").first().map(String::as_str), Some("id"));
        assert_eq!(backing_field_candidates("setCustomer").first().map(String::as_str), Some("customer"));
        assert_eq!(backing_field_candidates("isShipped").first().map(String::as_str), Some("shipped"));
        // A boolean that kept its own `is`: the setter's field is `isRunning`, not `running`.
        assert!(backing_field_candidates("setRunning").contains(&"isRunning".to_string()));
        // A fluent accessor (and an `is_`-prefixed getter) is named exactly after its field.
        assert!(backing_field_candidates("customer").contains(&"customer".to_string()));
        assert!(backing_field_candidates("is_attivo").contains(&"is_attivo".to_string()));
    }

    // ── the boolean `is…` naming rule ────────────────────────────────────────────

    /// The reported bug. Lombok strips a field's own `is` when what follows is not a **lowercase
    /// letter** — an underscore counts — so `private final boolean is_attivo` gets the getter
    /// `is_attivo()`. Reading the rule as "followed by an uppercase letter" produced `isIs_attivo`,
    /// and every `x.is_attivo()` in correct code was reported as unresolvable.
    #[test]
    fn boolean_field_named_is_underscore_keeps_its_name() {
        let td = type_with(&["Getter"], vec![field("is_attivo", "boolean")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"is_attivo"), "getter keeps the field's name, got {names:?}");
        assert!(!names.contains(&"isIs_attivo"), "no doubled `is`, got {names:?}");
    }

    #[test]
    fn boolean_field_named_is_camel_keeps_its_name() {
        let td = type_with(&["Data"], vec![field("isRunning", "boolean")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"isRunning"), "got {names:?}");
        // The SETTER drops the field's `is` too — Lombok runs both through one naming function.
        assert!(names.contains(&"setRunning"), "setter drops the field's `is`, got {names:?}");
        assert!(!names.contains(&"setIsRunning"), "got {names:?}");
    }

    /// The rule does NOT apply when a lowercase letter follows `is` — `isattivo` is just a field whose
    /// name happens to start with those two letters, so the prefix is added normally.
    #[test]
    fn boolean_field_named_is_lowercase_gets_the_prefix() {
        let td = type_with(&["Getter"], vec![field("isattivo", "boolean")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"isIsattivo"), "lowercase after `is` → prefixed, got {names:?}");
    }

    /// The `is`-stripping is for the primitive `boolean` only: a `Boolean` wrapper field, or any other
    /// type, is a plain `getX`.
    #[test]
    fn non_boolean_field_starting_with_is_is_not_stripped() {
        let td = type_with(&["Getter"], vec![field("isOwner", "Boolean"), field("island", "String")]);
        let m = synthesize(&td, &lombok(), &BTreeMap::new(), &HashSet::new(), &|_: &str| false);
        let names: Vec<&str> = m.methods.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"getIsOwner"), "wrapper Boolean uses getX, got {names:?}");
        assert!(names.contains(&"getIsland"), "got {names:?}");
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
