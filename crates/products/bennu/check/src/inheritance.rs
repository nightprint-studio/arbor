//! Inheritance-legality diagnostics, powered by the class-level [`ClassFlags`](bennu_java::prelude::ClassFlags)
//! decoded from bytecode. Two checks:
//!
//!   * **`inheritance_errors`** — an illegal `extends` / `implements`: a class extending a `final`
//!     type, a record, an enum, or an interface; a class implementing a non-interface; an interface
//!     extending a non-interface.
//!   * **`missing_abstract_impls`** — a concrete class that leaves an inherited abstract method
//!     unimplemented (`class X implements Runnable {}` with no `run()`).
//!
//! Conservative (docs: never a false positive):
//!   * a supertype the resolver doesn't know is skipped — its flags are unknown;
//!   * `missing_abstract_impls` runs only when the class's **whole** hierarchy is resolvable (an
//!     un-indexed base could supply the implementation, or hide the abstractness);
//!   * `sealed` supertypes are intentionally *not* flagged — the `permits` list isn't consulted yet,
//!     so a legally-permitted subclass must not be mis-reported;
//!   * `java.lang.Object` methods (`equals`, `hashCode`, …) are never treated as unimplemented —
//!     every class inherits their concrete versions.
//!
//! Today these fire against **library / JDK** supertypes (flags come from bytecode). Project-source
//! supertypes carry default flags until the symbol model grows a type-kind, so they're a
//! conservative miss, never a false positive.

use std::collections::HashSet;

use bennu_java::prelude::{extract_symbols, ClassMembers, FileSymbols, Member, MemberKind, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::members::simple_name;
use crate::resolve::type_binary;
use crate::walk::{for_each_supertype, hierarchy_fully_known};

// ── extends / implements legality ────────────────────────────────────────────

/// Parse `source` and flag illegal `extends` / `implements` clauses.
pub fn inheritance_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let symbols = extract_symbols(source);
    with_parse(source, |root| {
        inheritance_errors_in(&crate::check::collect_nodes(root), source, &symbols, resolver)
    })
}

/// Tree-driven core: iterates the shared `nodes` + reuses the caller's `symbols`.
pub fn inheritance_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "class_declaration" | "enum_declaration" | "record_declaration" => {
                check_class_supertypes(n, bytes, symbols, resolver, &mut out);
            }
            "interface_declaration" => {
                for (text, node) in extends_types(n, bytes) {
                    if let Some(cm) = resolve_members(&text, symbols, resolver) {
                        if !cm.flags.is_interface {
                            out.push(err(
                                format!(
                                    "An interface can only extend interfaces, not `{}`",
                                    simple_name(&binary_of(&text, symbols, resolver))
                                ),
                                node,
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// A class/enum/record's `extends` (single) + `implements` (many) legality.
fn check_class_supertypes(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // `extends S` — only classes have a superclass node (enums/records can't).
    for (text, node) in superclass_type(n, bytes) {
        let Some(cm) = resolve_members(&text, symbols, resolver) else { continue };
        let name = simple_name(&binary_of(&text, symbols, resolver)).to_string();
        let msg = if cm.flags.is_interface {
            Some(format!("Class cannot extend interface `{name}` (use `implements`)"))
        } else if cm.flags.is_record {
            Some(format!("Cannot inherit from record `{name}` (records are final)"))
        } else if cm.flags.is_enum {
            Some(format!("Cannot inherit from enum `{name}` (enums are final)"))
        } else if cm.flags.is_final {
            Some(format!("Cannot inherit from final `{name}`"))
        } else {
            None
        };
        if let Some(m) = msg {
            out.push(err(m, node));
        }
    }
    // `implements I, J` — each must be an interface.
    for (text, node) in implements_types(n, bytes) {
        let Some(cm) = resolve_members(&text, symbols, resolver) else { continue };
        if !cm.flags.is_interface {
            out.push(err(
                format!(
                    "Cannot implement `{}`: not an interface",
                    simple_name(&binary_of(&text, symbols, resolver))
                ),
                node,
            ));
        }
    }
}

// ── missing abstract implementations ─────────────────────────────────────────

/// Parse `source` and flag concrete classes that don't implement an inherited abstract method.
pub fn missing_abstract_impls(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let symbols = extract_symbols(source);
    with_parse(source, |root| {
        missing_abstract_impls_in(&crate::check::collect_nodes(root), source, &symbols, resolver)
    })
}

/// Tree-driven core: iterates the shared `nodes` + reuses the caller's `symbols`.
pub fn missing_abstract_impls_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let object_methods = object_method_names(resolver);
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "class_declaration" && !is_abstract(n, bytes) {
            check_missing_impls(n, bytes, symbols, resolver, &object_methods, &mut out);
        }
    }
    out
}

fn check_missing_impls(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    object_methods: &HashSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    // Direct supertypes (extends + implements) as binary names.
    let mut supers: Vec<String> = Vec::new();
    for (text, _) in superclass_type(n, bytes).into_iter().chain(implements_types(n, bytes)) {
        if let Some(bin) = type_binary(&text, symbols, resolver) {
            supers.push(bin);
        } else {
            return; // an unresolvable supertype → can't assert anything
        }
    }
    if supers.is_empty() {
        return;
    }
    // Every reachable supertype must be known, else the requirement/provision sets are incomplete.
    if !supers.iter().all(|s| hierarchy_fully_known(resolver, s)) {
        return;
    }

    // Required abstract method names across all supertypes, and the concrete ones already provided.
    let mut required: HashSet<String> = HashSet::new();
    let mut provided: HashSet<String> = object_methods.clone();
    for s in &supers {
        for_each_supertype(resolver, s, &mut |_bn, cm| {
            for m in &cm.methods {
                if m.kind != MemberKind::Method || is_ctor(&m.name) {
                    continue;
                }
                if is_abstract_requirement(cm, m) {
                    required.insert(m.name.clone());
                } else {
                    provided.insert(m.name.clone());
                }
            }
        });
    }
    // The class's own declared methods satisfy requirements too.
    let Some(td) = symbols.types.iter().find(|t| Some(t.name.as_str()) == class_name(n, bytes)) else {
        return;
    };
    for m in &td.methods {
        provided.insert(m.name.clone());
    }

    let name_node = n.child_by_field_name("name");
    let cls = class_name(n, bytes).unwrap_or("this class");
    let mut missing: Vec<&String> = required.difference(&provided).collect();
    missing.sort();
    for m in missing {
        out.push(err(
            format!("`{cls}` is not abstract and does not implement abstract method `{m}()`"),
            name_node.unwrap_or(n),
        ));
    }
}

/// Whether `m` is an abstract method a concrete subclass must implement: an `abstract` class method,
/// or an interface method that isn't `default`/`static`. Shared with the functional-interface check.
pub(crate) fn is_abstract_requirement(cm: &ClassMembers, m: &Member) -> bool {
    m.is_abstract || (cm.flags.is_interface && !m.is_default && !m.is_static)
}

/// The method names declared on `java/lang/Object` (satisfied by every class). Falls back to the
/// well-known set when Object isn't resolvable, so the exclusion always holds. Shared with the
/// functional-interface check (Object methods never count toward a SAM).
pub(crate) fn object_method_names(resolver: &dyn TypeResolver) -> HashSet<String> {
    if let Some(cm) = resolver.members_of("java/lang/Object") {
        let mut s: HashSet<String> = cm.methods.iter().map(|m| m.name.clone()).collect();
        s.extend(FALLBACK_OBJECT.iter().map(|s| s.to_string()));
        return s;
    }
    FALLBACK_OBJECT.iter().map(|s| s.to_string()).collect()
}

const FALLBACK_OBJECT: &[&str] = &[
    "equals", "hashCode", "toString", "clone", "finalize", "getClass", "wait", "notify",
    "notifyAll",
];

pub(crate) fn is_ctor(name: &str) -> bool {
    name == "<init>" || name == "<clinit>"
}

// ── CST helpers ──────────────────────────────────────────────────────────────

/// The `extends` type of a class (`superclass` wrapper) — a 0-or-1 element list of (text, node).
fn superclass_type<'t>(n: Node<'t>, bytes: &[u8]) -> Vec<(String, Node<'t>)> {
    n.child_by_field_name("superclass")
        .and_then(|w| first_type_under(w, bytes))
        .into_iter()
        .collect()
}

/// The `implements` types of a class (`interfaces`/`super_interfaces` → `type_list`).
fn implements_types<'t>(n: Node<'t>, bytes: &[u8]) -> Vec<(String, Node<'t>)> {
    match n.child_by_field_name("interfaces") {
        Some(w) => types_under_list(w, bytes),
        None => Vec::new(),
    }
}

/// The `extends` types of an interface (`extends_interfaces` → `type_list`).
fn extends_types<'t>(n: Node<'t>, bytes: &[u8]) -> Vec<(String, Node<'t>)> {
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == "extends_interfaces" {
            return types_under_list(ch, bytes);
        }
    }
    Vec::new()
}

/// The first type node under a wrapper (`superclass`), as (text, node).
fn first_type_under<'t>(wrapper: Node<'t>, bytes: &[u8]) -> Option<(String, Node<'t>)> {
    let mut c = wrapper.walk();
    for ch in wrapper.named_children(&mut c) {
        if is_type_node(ch.kind()) {
            return ch.utf8_text(bytes).ok().map(|t| (t.to_string(), ch));
        }
    }
    None
}

/// Every type node under a `type_list` (possibly nested in `interfaces`/`extends_interfaces`).
fn types_under_list<'t>(wrapper: Node<'t>, bytes: &[u8]) -> Vec<(String, Node<'t>)> {
    let mut out = Vec::new();
    let mut stack = vec![wrapper];
    while let Some(node) = stack.pop() {
        if node.kind() == "type_list" || node.kind() == "interface_type_list" {
            let mut c = node.walk();
            for ch in node.named_children(&mut c) {
                if is_type_node(ch.kind()) {
                    if let Ok(t) = ch.utf8_text(bytes) {
                        out.push((t.to_string(), ch));
                    }
                }
            }
        } else {
            let mut c = node.walk();
            for ch in node.named_children(&mut c) {
                stack.push(ch);
            }
        }
    }
    out
}

fn is_type_node(kind: &str) -> bool {
    matches!(kind, "type_identifier" | "scoped_type_identifier" | "generic_type")
}

fn is_abstract(n: Node, bytes: &[u8]) -> bool {
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() && m.utf8_text(bytes) == Ok("abstract") {
                    return true;
                }
            }
        }
    }
    false
}

fn class_name<'a>(n: Node, bytes: &'a [u8]) -> Option<&'a str> {
    n.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok())
}

fn resolve_members(
    text: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<std::sync::Arc<ClassMembers>> {
    let binary = type_binary(text, symbols, resolver)?;
    resolver.members_of(&binary)
}

fn binary_of(text: &str, symbols: &FileSymbols, resolver: &dyn TypeResolver) -> String {
    type_binary(text, symbols, resolver).unwrap_or_else(|| text.replace('.', "/"))
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic { message, severity: "error".to_string(), code: String::new(), start: node.start_byte(), end: node.end_byte() }
}

fn with_parse(source: &str, f: impl FnOnce(Node) -> Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    match parser.parse(source, None) {
        Some(tree) => f(tree.root_node()),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, Import, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn abstract_method(name: &str) -> Member {
        Member::method(name, TypeRef::simple("void"), Vec::new()).abstract_()
    }

    fn cm(flags: ClassFlags, superclass: Option<&str>, ifaces: &[&str], methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(str::to_string),
            interfaces: ifaces.iter().map(|s| s.to_string()).collect(),
            methods,
            fields: Vec::new(),
            flags,
        }
    }

    fn flags(builder: impl FnOnce(&mut ClassFlags)) -> ClassFlags {
        let mut f = ClassFlags::default();
        builder(&mut f);
        f
    }

    /// Object; a `final` Foo; a `Runnable`-like interface `Task` (abstract `run`); an interface with a
    /// default method `Def`; an `enum` E; a `record` R.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cm(ClassFlags::default(), None, &[], vec![]));
        members.insert(
            "com/acme/Foo".to_string(),
            cm(flags(|f| f.is_final = true), Some("java/lang/Object"), &[], vec![]),
        );
        members.insert(
            "com/acme/Task".to_string(),
            cm(flags(|f| f.is_interface = true), None, &[], vec![abstract_method("run")]),
        );
        members.insert(
            "com/acme/Def".to_string(),
            cm(flags(|f| f.is_interface = true), None, &[], {
                let mut m = abstract_method("provided");
                m.is_abstract = false;
                m.is_default = true;
                vec![m]
            }),
        );
        members.insert(
            "com/acme/E".to_string(),
            cm(flags(|f| { f.is_enum = true; f.is_final = true; }), Some("java/lang/Enum"), &[], vec![]),
        );
        members.insert(
            "com/acme/R".to_string(),
            cm(flags(|f| { f.is_record = true; f.is_final = true; }), Some("java/lang/Record"), &[], vec![]),
        );
        let simple = [
            ("Foo", "com/acme/Foo"),
            ("Task", "com/acme/Task"),
            ("Def", "com/acme/Def"),
            ("E", "com/acme/E"),
            ("R", "com/acme/R"),
            ("Object", "java/lang/Object"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn inh(src: &str) -> Vec<String> {
        inheritance_errors(src, &resolver()).into_iter().map(|d| d.message).collect()
    }
    fn abs(src: &str) -> Vec<String> {
        missing_abstract_impls(src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    // ── extends / implements legality ──────────────────────────────────────────

    #[test]
    fn extends_final_is_flagged() {
        let d = inh("class X extends Foo {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("final") && d[0].contains("Foo"), "{d:?}");
    }

    #[test]
    fn extends_record_and_enum_are_flagged() {
        assert!(inh("class X extends R {}")[0].contains("record"), "record");
        assert!(inh("class X extends E {}")[0].contains("enum"), "enum");
    }

    #[test]
    fn class_extends_interface_is_flagged() {
        let d = inh("class X extends Task {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("use `implements`"), "{d:?}");
    }

    #[test]
    fn implements_non_interface_is_flagged() {
        // implementing a (final) class is illegal.
        let d = inh("class X implements Foo {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("not an interface"), "{d:?}");
    }

    #[test]
    fn interface_extends_class_is_flagged() {
        let d = inh("interface X extends Foo {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("only extend interfaces"), "{d:?}");
    }

    #[test]
    fn legal_implements_is_ok() {
        assert!(inh("class X implements Task { public void run() {} }").is_empty());
    }

    #[test]
    fn unknown_supertype_is_not_flagged() {
        assert!(inh("class X extends Unknown {}").is_empty());
    }

    // ── missing abstract implementations ───────────────────────────────────────

    #[test]
    fn missing_impl_is_flagged() {
        let d = abs("class X implements Task {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("run") && d[0].contains("not abstract"), "{d:?}");
    }

    #[test]
    fn provided_impl_is_ok() {
        assert!(abs("class X implements Task { public void run() {} }").is_empty());
    }

    #[test]
    fn abstract_class_need_not_implement() {
        assert!(abs("abstract class X implements Task {}").is_empty());
    }

    #[test]
    fn default_method_is_not_required() {
        // `Def.provided` is a default method → satisfied, nothing to implement.
        assert!(abs("class X implements Def {}").is_empty());
    }

    #[test]
    fn object_methods_are_never_required() {
        // A `Task`-like interface that re-declares an Object method must not force an override.
        let mut r = resolver();
        r.members.insert(
            "com/acme/Cmp".to_string(),
            cm(flags(|f| f.is_interface = true), None, &[], vec![abstract_method("equals")]),
        );
        r.simple.insert("Cmp".to_string(), "com/acme/Cmp".to_string());
        let d: Vec<String> =
            missing_abstract_impls("class X implements Cmp {}", &r).into_iter().map(|x| x.message).collect();
        assert!(d.is_empty(), "equals is an Object method, satisfied: {d:?}");
    }

    #[test]
    fn unknown_hierarchy_is_not_flagged() {
        assert!(abs("class X extends Mystery implements Task {}").is_empty());
    }
}
