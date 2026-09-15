//! Inheritance-legality diagnostics, powered by the class-level [`ClassFlags`](bennu_java::prelude::ClassFlags)
//! decoded from bytecode. Two checks:
//!
//!   * **`inheritance_errors`** — an illegal `extends` / `implements`: a class extending a `final`
//!     type, a record, an enum, or an interface; a class implementing a non-interface; an interface
//!     extending a non-interface.
//!     A generic class whose superclass is a `Throwable` is refused here too (JLS §8.1.2).
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

use std::collections::{HashMap, HashSet};

use bennu_java::prelude::{extract_symbols, ClassMembers, FileSymbols, Member, TypeResolver, Visibility};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::hierarchy::obligations::{declared_provision, indexed_provision, obligations, unmet, Provision};
use crate::support::nodes::simple_name;
use crate::support::supertypes;
use crate::support::walk::hierarchy_fully_known;

const THROWABLE: &str = "java/lang/Throwable";

// ── extends / implements legality ────────────────────────────────────────────

/// Parse `source` and flag illegal `extends` / `implements` clauses.
pub fn inheritance_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let symbols = extract_symbols(source);
    with_parse(source, |root| {
        inheritance_errors_in(&crate::engine::check::collect_nodes(root), source, &symbols, resolver)
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
            // `new T() { … }` declares a subclass of `T`, and a `final` class has none.
            "object_creation_expression" if class_body_of(n).is_some() && !crate::support::nodes::is_qualified_creation(n) => {
                let Some(ty) = n.child_by_field_name("type") else { continue };
                let Ok(text) = ty.utf8_text(bytes) else { continue };
                let Some(binary) = crate::support::resolve::type_binary_at(text, n, bytes, symbols, resolver) else {
                    continue;
                };
                let Some(cm) = resolver.members_of(&binary) else { continue };
                if !cm.flags.is_interface && (cm.flags.is_final || cm.flags.is_record) {
                    out.push(CheckId::IllegalInheritance.at(
                        ty,
                        format!("Cannot inherit from final `{}` — not even anonymously", simple_name(&binary)),
                    ));
                }
            }
            "interface_declaration" => {
                for sup in supertypes::interfaces(n, bytes) {
                    if let Some(cm) = resolve_members(&sup.text, n, bytes, symbols, resolver) {
                        if !cm.flags.is_interface {
                            out.push(CheckId::IllegalInheritance.at(
                                sup.node,
                                format!(
                                    "An interface can only extend interfaces, not `{}`",
                                    simple_name(&binary_of(&sup.text, n, bytes, symbols, resolver))
                                ),
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
    if let Some(sup) = supertypes::superclass(n, bytes) {
        if let Some(cm) = resolve_members(&sup.text, n, bytes, symbols, resolver) {
            let name = simple_name(&binary_of(&sup.text, n, bytes, symbols, resolver)).to_string();
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
                out.push(CheckId::IllegalInheritance.at(sup.node, m));
            }
            // A generic class may not be a `Throwable` (JLS §8.1.2): a `catch` could not tell its
            // parameterizations apart at run time.
            if n.child_by_field_name("type_parameters").is_some() {
                let binary = binary_of(&sup.text, n, bytes, symbols, resolver);
                if binary == THROWABLE || crate::support::walk::reaches(resolver, &binary, THROWABLE) {
                    out.push(CheckId::IllegalGenericUsage.at(
                        sup.node,
                        format!("A generic class cannot extend `{}`, a `Throwable`", simple_name(&binary)),
                    ));
                }
            }
        }
    }
    // `implements I, J` — each must be an interface.
    for sup in supertypes::interfaces(n, bytes) {
        let Some(cm) = resolve_members(&sup.text, n, bytes, symbols, resolver) else { continue };
        if !cm.flags.is_interface {
            out.push(CheckId::IllegalInheritance.at(
                sup.node,
                format!(
                    "Cannot implement `{}`: not an interface",
                    simple_name(&binary_of(&sup.text, n, bytes, symbols, resolver))
                ),
            ));
        }
    }
}

/// Whether `n` names an interface in `extends` or a class in `implements` — the shapes
/// [`check_class_supertypes`] reports.
fn clause_kind_is_wrong(n: Node, bytes: &[u8], symbols: &FileSymbols, resolver: &dyn TypeResolver) -> bool {
    let extends_interface = supertypes::superclass(n, bytes)
        .and_then(|sup| resolve_members(&sup.text, n, bytes, symbols, resolver))
        .is_some_and(|cm| cm.flags.is_interface);
    extends_interface
        || supertypes::interfaces(n, bytes)
            .iter()
            .filter_map(|sup| resolve_members(&sup.text, n, bytes, symbols, resolver))
            .any(|cm| !cm.flags.is_interface)
}

// ── missing abstract implementations ─────────────────────────────────────────

/// Parse `source` and flag concrete classes that don't implement an inherited abstract method.
pub fn missing_abstract_impls(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let symbols = extract_symbols(source);
    with_parse(source, |root| {
        missing_abstract_impls_in(&crate::engine::check::collect_nodes(root), source, &symbols, resolver)
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
        match n.kind() {
            "class_declaration" if !is_abstract(n, bytes) => {
                check_missing_impls(n, bytes, symbols, resolver, &object_methods, &mut out)
            }
            "object_creation_expression" => {
                check_anonymous_impls(n, bytes, symbols, resolver, &object_methods, &mut out)
            }
            _ => {}
        }
    }
    out
}

/// `new T() { … }` — an anonymous class is never abstract, so it owes every abstract method `T`'s
/// hierarchy leaves, exactly like a named concrete class, and is judged the same way: by signature.
fn check_anonymous_impls(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    object_methods: &HashSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    if crate::support::nodes::is_qualified_creation(n) {
        return;
    }
    let Some(body) = class_body_of(n) else { return };
    let Some(ty) = n.child_by_field_name("type") else { return };
    let Ok(text) = ty.utf8_text(bytes) else { return };
    let Some(binary) = crate::support::resolve::type_binary(text, symbols, resolver) else { return };
    // An annotated member may be one that invents methods (Lombok): what the body provides is then
    // not readable off the tree.
    if !hierarchy_fully_known(resolver, &binary) || has_annotated_member(body) {
        return;
    }
    let (required, mut provided) = obligations(std::slice::from_ref(&binary), resolver, object_methods);
    add_declared(body, bytes, symbols, resolver, &mut provided);
    for m in unmet(&required, &provided, resolver) {
        out.push(CheckId::MissingAbstractMethod.at(
            ty,
            format!(
                "Anonymous `{}` does not implement abstract method `{}()`",
                simple_name(&binary),
                m.name
            ),
        ));
    }
}

/// Every method a type body declares, as what it provides.
fn add_declared(
    body: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    provided: &mut HashMap<String, Vec<Provision>>,
) {
    let mut c = body.walk();
    let methods: Vec<Node> = body.named_children(&mut c).filter(|m| m.kind() == "method_declaration").collect();
    for member in methods {
        if let Some(name) = member.child_by_field_name("name").and_then(|m| m.utf8_text(bytes).ok()) {
            provided
                .entry(name.to_string())
                .or_default()
                .push(declared_provision(member, bytes, symbols, resolver));
        }
    }
}

fn class_body_of(n: Node) -> Option<Node> {
    let mut c = n.walk();
    let body = n.named_children(&mut c).find(|child| child.kind() == "class_body");
    body
}

/// Whether any member of an anonymous `body` carries an annotation.
fn has_annotated_member(body: Node) -> bool {
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        let mut mc = member.walk();
        for part in member.named_children(&mut mc) {
            if part.kind() != "modifiers" {
                continue;
            }
            let mut pc = part.walk();
            if part.named_children(&mut pc).any(|m| matches!(m.kind(), "annotation" | "marker_annotation")) {
                return true;
            }
        }
    }
    false
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
    // An unresolvable supertype → can't assert anything: it is exactly where the implementation
    // this is about to call missing might be declared.
    let Some(supers) = supertypes::binaries_complete(n, bytes, symbols, resolver) else { return };
    if supers.is_empty() || clause_kind_is_wrong(n, bytes, symbols, resolver) {
        // A class that `extends` an interface or `implements` a class has one error, and it is
        // that: reading the wrong clause's methods as obligations reports a second one javac
        // never gets to.
        return;
    }
    // Every reachable supertype must be known, else the requirement/provision sets are incomplete.
    if !supers.iter().all(|s| hierarchy_fully_known(resolver, s)) {
        return;
    }

    // Required abstract methods across all supertypes, and the concrete ones already provided.
    let (required, mut provided) = obligations(&supers, resolver, object_methods);
    // The class's own declared methods satisfy requirements too — read off THIS declaration's body.
    // Found by POSITION: guava's `Maps.java` declares two classes called `KeySet` and
    // `ConcurrentHashMultiset.java` two called `EntrySet`, so a search by simple name read one class's
    // methods as the other's — and eight guava classes were reported for not implementing methods
    // they declare on themselves.
    let Some(td) = bennu_java::prelude::type_decl_at(&n, symbols) else { return };
    if let Some(body) = n.child_by_field_name("body") {
        add_declared(body, bytes, symbols, resolver, &mut provided);
    }
    // And the members nobody wrote. A Lombok accessor exists only in the INDEX — `@Getter` with
    // `@Accessors(fluent = true)` on a field `alias` is the method `alias()`, and the tree above
    // has no trace of it — so a class implementing an interface through its generated accessors was
    // reported for not implementing methods it does implement.
    //
    // Asked by the class's own fully-qualified name rather than by simple name: that is what keeps
    // the two `EntrySet`s of one file apart, which is the reason this reads the tree in the first
    // place.
    if let Some(cm) = resolver.members_of(&td.fqn.replace('.', "/")) {
        // Only the names the body does not declare: for those the tree above already answered, with
        // their return types — the index's copy would count as a match whatever they return.
        let declared: HashSet<&str> = td.methods.iter().map(|m| m.name.as_str()).collect();
        for m in cm.methods.iter().filter(|m| !declared.contains(m.name.as_str())) {
            provided.entry(m.name.clone()).or_default().push(indexed_provision(m, resolver));
        }
    } else if generates_methods(n, bytes) {
        // The index cannot answer for this class — a buffer that has not been indexed yet — and it
        // carries an annotation that invents methods. What it provides is not knowable here, and
        // this check's rule throughout is that an incomplete picture says nothing.
        return;
    }

    let name_node = n.child_by_field_name("name");
    let cls = class_name(n, bytes).unwrap_or("this class");
    for m in unmet(&required, &provided, resolver) {
        out.push(CheckId::MissingAbstractMethod.at(
            name_node.unwrap_or(n),
            format!("`{cls}` is not abstract and does not implement abstract method `{}()`", m.name),
        ));
    }
}

/// Whether `decl` carries a Lombok annotation that invents METHODS — the ones that can satisfy an
/// interface. Constructors and `log` fields cannot, so they are deliberately not in this set: a
/// `@Slf4j` class is checked like any other.
fn generates_methods(decl: Node, bytes: &[u8]) -> bool {
    const METHOD_GENERATING: &[&str] = &[
        "Data", "Value", "Getter", "Setter", "With", "Accessors", "Builder", "SuperBuilder",
    ];
    let imports = crate::support::lombok::imports_from_root(root_of(decl), bytes);
    if crate::support::lombok::has_lombok_annotation(decl, bytes, &imports, |a| {
        METHOD_GENERATING.contains(&a.simple)
    }) {
        return true;
    }
    // Field-level `@Getter` / `@Setter`, which is how a single accessor is asked for.
    let Some(body) = decl.child_by_field_name("body") else { return false };
    let mut c = body.walk();
    for m in body.named_children(&mut c) {
        if m.kind() == "field_declaration"
            && crate::support::lombok::has_lombok_annotation(m, bytes, &imports, |a| {
                METHOD_GENERATING.contains(&a.simple)
            })
        {
            return true;
        }
    }
    false
}

/// The compilation unit `decl` lives in — where the imports are.
fn root_of(decl: Node) -> Node {
    let mut n = decl;
    while let Some(p) = n.parent() {
        n = p;
    }
    n
}

/// Whether `m` is an abstract method a concrete subclass must implement: an `abstract` class method,
/// or an interface method that isn't `default`/`static`. Shared with the functional-interface check.
pub(crate) fn is_abstract_requirement(cm: &ClassMembers, m: &Member) -> bool {
    // A `private` interface method (Java 9) always has a body — it is a helper for the default
    // methods, never something an implementation owes. Counting it made an interface with one
    // abstract method and a private helper "not a functional interface".
    m.is_abstract
        || (cm.flags.is_interface && !m.is_default && !m.is_static && m.visibility != Visibility::Private)
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

fn is_abstract(n: Node, bytes: &[u8]) -> bool {
    crate::support::nodes::has_keyword(n, bytes, "abstract")
}

fn class_name<'a>(n: Node, bytes: &'a [u8]) -> Option<&'a str> {
    n.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok())
}

/// The members of a supertype written in `decl`'s header — `None` when it does not resolve, which
/// every caller treats as "say nothing".
fn resolve_members(
    text: &str,
    decl: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<std::sync::Arc<ClassMembers>> {
    let binary = supertypes::binary(text, decl, bytes, symbols, resolver)?;
    resolver.members_of(&binary)
}

/// The same name for a MESSAGE: falls back to the written spelling when nothing binds it.
fn binary_of(
    text: &str,
    decl: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> String {
    supertypes::binary(text, decl, bytes, symbols, resolver)
        .unwrap_or_else(|| text.replace('.', "/"))
}

fn with_parse(source: &str, f: impl FnOnce(Node) -> Vec<Diagnostic>) -> Vec<Diagnostic> {
    match bennu_java::prelude::parse_java(source) {
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
            superclass: superclass.map(TypeRef::simple),
            interfaces: ifaces.iter().map(|s| TypeRef::simple(*s)).collect(),
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
        // A class whose ONLY implementation of `Task.run` is a Lombok accessor: nothing in its
        // source declares `run`, and the index has it because `@Getter` on a field named `run`
        // (with `@Accessors(fluent = true)`) generates exactly that.
        members.insert(
            "com/acme/Generated".to_string(),
            cm(ClassFlags::default(), Some("java/lang/Object"), &["com/acme/Task"], {
                let mut m = abstract_method("run");
                m.is_abstract = false;
                vec![m]
            }),
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

    /// **A class that implements its interface through a Lombok-generated accessor.**
    ///
    /// `@Accessors(fluent = true)` names the getter after the field, so `@Getter` on `run` IS the
    /// implementation of `Task.run()` — and nothing in the source says so. Reading only the tree,
    /// the class looked like it implemented nothing, and a correct class was reported for a method
    /// it does provide. The index knows, because that is where a generated member lives.
    #[test]
    fn an_interface_implemented_by_a_lombok_accessor_is_not_reported() {
        let src = "package com.acme;\n\
                   import lombok.Getter;\n\
                   import lombok.experimental.Accessors;\n\
                   @Getter @Accessors(fluent = true)\n\
                   class Generated implements Task { private String run; }";
        assert!(abs(src).is_empty(), "{:?}", abs(src));
    }

    /// And when the index cannot answer — a buffer nobody has indexed yet — a class that generates
    /// methods says nothing rather than guessing, which is this check's rule everywhere else.
    #[test]
    fn a_lombok_class_the_index_has_never_seen_is_left_alone() {
        let src = "package com.acme;\n\
                   import lombok.Getter;\n\
                   import lombok.experimental.Accessors;\n\
                   @Getter @Accessors(fluent = true)\n\
                   class NotIndexedYet implements Task { private String run; }";
        assert!(abs(src).is_empty(), "{:?}", abs(src));
    }

    /// The gate is Lombok, not "any class we cannot look up": a plain class that really does not
    /// implement the interface is still reported.
    #[test]
    fn a_plain_class_that_implements_nothing_is_still_reported() {
        let d = abs("package com.acme;\nclass Plain implements Task { }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("run()"), "{d:?}");
    }

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

#[cfg(test)]
mod generic_throwable_tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, Import, TypeRef};
    use std::sync::Arc;

    struct Resolver(HashMap<String, ClassMembers>);

    impl TypeResolver for Resolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            ["Object", "Throwable", "Exception"].contains(&name).then(|| format!("java/lang/{name}"))
        }
    }

    fn class(superclass: Option<&str>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(TypeRef::simple),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    fn resolver() -> Resolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), class(None));
        members.insert("java/lang/Throwable".to_string(), class(Some("java/lang/Object")));
        members.insert("java/lang/Exception".to_string(), class(Some("java/lang/Throwable")));
        Resolver(members)
    }

    fn messages(source: &str) -> Vec<String> {
        inheritance_errors(source, &resolver()).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn a_generic_class_extending_an_exception_is_flagged() {
        let d = messages("class Failure<X> extends Exception { }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("generic class cannot extend `Exception`"), "{d:?}");
    }

    #[test]
    fn a_plain_exception_or_a_generic_non_throwable_is_ok() {
        assert!(messages("class Failure extends Exception { }").is_empty());
        assert!(messages("class Box<X> extends Object { }").is_empty());
    }
}
