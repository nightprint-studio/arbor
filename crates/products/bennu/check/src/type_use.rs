//! Resolver-backed "type use" diagnostics — two compile errors that need the type resolver + type
//! inference to reason about a hierarchy:
//!
//!   * **incompatible `instanceof`** — `expr instanceof T` (with or without a pattern binding) where
//!     `type(expr)` and `T` are UNRELATED concrete classes (neither reaches the other). This is the
//!     exact "inconvertible types" rule `casts.rs` implements for `(T) expr` — `instanceof` is a
//!     runtime cast test, so the same soundness applies. Reuses `walk::reaches` /
//!     `walk::hierarchy_fully_known`, mirroring `casts::check_cast`.
//!   * **instantiating an abstract class / interface** — `new T(...)` where `T` resolves to a type
//!     whose `flags.is_abstract` or `flags.is_interface` is set: you can't `new` it (only an
//!     *anonymous* subclass `new T(...) { … }` is legal, and that is explicitly skipped).
//!
//! Extremely conservative (docs: NEVER a false positive). Every skip is spelled out at its site; the
//! guiding rule is "unresolvable / unknown ⇒ not an error": when a type doesn't resolve, when either
//! side of an `instanceof` is an interface (a subclass could implement it), or when a hierarchy isn't
//! fully known, we SKIP rather than risk a wrong report.

use bennu_java::prelude::{FileSymbols, InferCache, TypeResolver, infer_node_type_cached};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::check_id::CheckId;
use crate::members::simple_name;
use crate::resolve::type_binary;
use crate::walk::{hierarchy_fully_known, reaches};

/// Tree-driven entry point — matches `casts::type_compat_errors_in`'s signature so `check.rs` wires
/// it the same way: iterates the shared `nodes` slice, reuses `root` + `symbols` + the inference
/// `cache`.
pub fn type_use_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "instanceof_expression" => {
                check_instanceof(n, &root, source, bytes, symbols, resolver, cache, &mut out)
            }
            "object_creation_expression" => check_new_abstract(n, bytes, symbols, resolver, &mut out),
            _ => {}
        }
    }
    out
}

// ── 1. incompatible `instanceof` ─────────────────────────────────────────────

/// `expr instanceof T` / `expr instanceof T name` where `type(expr)` and `T` are unrelated concrete
/// classes → "incompatible types" (a value of `type(expr)` can NEVER be a `T`). Mirrors
/// `casts::check_cast`: infer the value type, resolve the written type, and flag ONLY when both are
/// concrete classes with fully-known hierarchies and neither reaches the other.
#[allow(clippy::too_many_arguments)]
fn check_instanceof(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    // `left` = the tested expression, `right` = the written type (0.23 grammar; `name` is the
    // optional pattern binding and doesn't affect either field).
    let Some(expr) = n.child_by_field_name("left") else { return };
    let Some(ty_node) = n.child_by_field_name("right") else { return };
    let Ok(type_text) = ty_node.utf8_text(bytes) else { return };

    // SKIP: the expression has no inferable nominal type (a literal, an unknown name, an un-modelled
    // call) — no value type ⇒ nothing to contradict.
    let Some(value_ty) = infer_node_type_cached(root, source, symbols, &expr, resolver, cache) else {
        return;
    };

    // SKIP: the written type `T` isn't a concrete class we can reason about. `concrete_class` returns
    // `None` for an INTERFACE (a subclass of `type(expr)` could still implement it, so `instanceof`
    // an interface is ALWAYS legal — never flag), a type variable, an array, a primitive, or an
    // unresolved name.
    let Some(target) = concrete_class(type_text, symbols, resolver) else { return };
    // SKIP: the value's type isn't a concrete class either (same interface/var/array/primitive/unknown
    // carve-outs) — if `type(expr)` is an interface, a concrete `T` could implement it.
    let Some(source_ty) = concrete_binary(value_ty.binary_name, resolver) else { return };

    // SKIP: an incomplete hierarchy on either side — an un-indexed base could establish the relation,
    // so we can't soundly say "unrelated". Only a fully-known pair lets us conclude.
    if !hierarchy_fully_known(resolver, &source_ty) || !hierarchy_fully_known(resolver, &target) {
        return;
    }

    // Flag ONLY when NEITHER type reaches the other: not a subtype either direction ⇒ no value of one
    // can ever be an instance of the other (the JLS "inconvertible types" error).
    if !reaches(resolver, &source_ty, &target) && !reaches(resolver, &target, &source_ty) {
        out.push(CheckId::IncompatibleInstanceof.at(
            ty_node,
            format!(
                "Incompatible types: `{}` can never be an instance of `{}`",
                simple_name(&source_ty),
                simple_name(&target)
            ),
        ));
    }
}

// ── 2. instantiating an abstract class / interface ───────────────────────────

/// `new T(...)` where `T` resolves to an abstract class or an interface → "cannot instantiate". SKIPs
/// the one legal case: an ANONYMOUS subclass `new T(...) { … }` (a `class_body` child), which
/// instantiates a fresh concrete subclass, not `T` itself.
fn check_new_abstract(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(ty_node) = n.child_by_field_name("type") else { return };

    // SKIP: anonymous class instantiation `new T(...) { … }` — instantiating an abstract/interface
    // type THIS way is legal (the body defines a concrete anonymous subclass). A `class_body` child
    // marks it. Explicit `for` loop, never `.any()` on `named_children` (borrow-checker).
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == "class_body" {
            return;
        }
    }

    let Ok(type_text) = ty_node.utf8_text(bytes) else { return };
    // SKIP: array creation (`new int[]`) is a different node kind, but a diamond / generic we can't
    // resolve, or any name that doesn't resolve, yields `None` here → skip. `type_binary` strips
    // generic arguments, so `new List<>()`-style text resolves to the raw binary when known.
    let Some(binary) = type_binary(type_text, symbols, resolver) else { return };

    // SKIP: the type isn't in the index — unknown flags, can't assert it's abstract.
    let Some(cm) = resolver.members_of(&binary) else { return };

    // Flag ONLY when definitely abstract or an interface (both un-instantiable directly).
    let name = simple_name(&binary);
    if cm.flags.is_interface {
        out.push(CheckId::InstantiateAbstract.at(ty_node, format!("Cannot instantiate the interface `{name}`")));
    } else if cm.flags.is_abstract {
        out.push(CheckId::InstantiateAbstract.at(ty_node, format!("Cannot instantiate the abstract type `{name}`")));
    }
}

// ── shared helpers (mirrors `casts.rs`) ──────────────────────────────────────

/// Resolve a **written** type name (`Foo`, `com.acme.Foo`) to a concrete-class binary name, or `None`
/// when it isn't one we can reason about (interface / type var / array / primitive / unknown).
fn concrete_class(text: &str, symbols: &FileSymbols, resolver: &dyn TypeResolver) -> Option<String> {
    let binary = type_binary(text, symbols, resolver)?;
    concrete_binary(binary, resolver)
}

/// Validate an already-resolved **binary** name as a concrete class: not a type variable, array,
/// primitive, interface, or unknown. Same carve-outs as `casts::concrete_binary`.
fn concrete_binary(binary: String, resolver: &dyn TypeResolver) -> Option<String> {
    if is_type_var(&binary) || binary.ends_with("[]") || is_primitive(&binary) {
        return None;
    }
    let cm = resolver.members_of(&binary)?;
    if cm.flags.is_interface {
        return None; // an interface is never "unrelated" — a subclass could implement it
    }
    Some(binary)
}

fn is_type_var(binary: &str) -> bool {
    binary.len() == 1 && binary.chars().all(|c| c.is_ascii_uppercase())
}

fn is_primitive(binary: &str) -> bool {
    matches!(
        binary,
        "int" | "long" | "short" | "byte" | "char" | "boolean" | "float" | "double" | "void"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef, extract_symbols};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tree_sitter::Parser;

    // Reuses the `MapResolver` mock shape from `casts.rs`.
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

    fn cls(flags: ClassFlags, superclass: Option<&str>, methods: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(str::to_string),
            interfaces: Vec::new(),
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

    fn getter(name: &str, ret: &str) -> Member {
        Member::method(name, TypeRef::simple(ret.to_string()), Vec::new())
    }

    /// Seed:
    ///   * `String` — `final` class;
    ///   * `Thread` — plain class (unrelated to `String`);
    ///   * `Shape` — `abstract` class;
    ///   * `Circle extends Shape` — concrete class;
    ///   * `Runnable` — interface.
    /// `Provider` exposes typed getters so a `p.foo()` receiver has an inferable nominal type.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cls(ClassFlags::default(), None, vec![]));
        members.insert(
            "java/lang/String".to_string(),
            cls(flags(|f| f.is_final = true), Some("java/lang/Object"), vec![]),
        );
        members.insert(
            "java/lang/Thread".to_string(),
            cls(ClassFlags::default(), Some("java/lang/Object"), vec![]),
        );
        members.insert(
            "com/acme/Shape".to_string(),
            cls(flags(|f| f.is_abstract = true), Some("java/lang/Object"), vec![]),
        );
        members.insert(
            "com/acme/Circle".to_string(),
            cls(ClassFlags::default(), Some("com/acme/Shape"), vec![]),
        );
        members.insert(
            "java/lang/Runnable".to_string(),
            cls(flags(|f| f.is_interface = true), None, vec![]),
        );
        members.insert(
            "com/acme/Provider".to_string(),
            cls(
                ClassFlags::default(),
                Some("java/lang/Object"),
                vec![
                    getter("text", "java/lang/String"),
                    getter("obj", "java/lang/Object"),
                    getter("shape", "com/acme/Shape"),
                ],
            ),
        );
        let simple = [
            ("Object", "java/lang/Object"),
            ("String", "java/lang/String"),
            ("Thread", "java/lang/Thread"),
            ("Shape", "com/acme/Shape"),
            ("Circle", "com/acme/Circle"),
            ("Runnable", "java/lang/Runnable"),
            ("Provider", "com/acme/Provider"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn run(source: &str) -> Vec<String> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = extract_symbols(source);
        type_use_errors_in(root, &nodes, source, &symbols, &resolver(), &InferCache::new())
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    /// Wrap a method body in a class whose field `p` is a `Provider`.
    fn body(b: &str) -> Vec<String> {
        run(&format!("class C {{ Provider p; void m() {{ {b} }} }}"))
    }

    // ── check 1: incompatible instanceof ───────────────────────────────────────

    #[test]
    fn unrelated_instanceof_is_flagged() {
        // `p.text()` is a String; `String instanceof Thread` — unrelated concrete classes → error.
        let d = body("if (p.text() instanceof Thread) {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("String") && d[0].contains("Thread"), "{d:?}");
        assert!(d[0].contains("never be an instance"), "{d:?}");
    }

    #[test]
    fn unrelated_instanceof_pattern_form_is_flagged() {
        // The pattern-binding form `instanceof Thread t` flags identically.
        let d = body("if (p.text() instanceof Thread t) {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("String") && d[0].contains("Thread"), "{d:?}");
    }

    #[test]
    fn instanceof_interface_is_not_flagged() {
        // `Object instanceof Runnable` — Runnable is an interface; a subclass could implement it, so
        // this is ALWAYS legal → never flagged.
        assert!(body("if (p.obj() instanceof Runnable) {}").is_empty());
    }

    #[test]
    fn related_instanceof_is_not_flagged() {
        // `Shape instanceof Circle` — Circle is-a Shape (downcast test) → legal.
        assert!(body("if (p.shape() instanceof Circle) {}").is_empty());
    }

    #[test]
    fn unresolved_instanceof_type_is_not_flagged() {
        // The written type doesn't resolve → SKIP.
        assert!(body("if (p.text() instanceof Unknown) {}").is_empty());
    }

    #[test]
    fn uninferable_instanceof_value_is_not_flagged() {
        // `unknownLocal` has no inferable type → SKIP (no value type to contradict).
        assert!(body("if (unknownLocal instanceof Thread) {}").is_empty());
    }

    // ── check 2: instantiating an abstract class / interface ───────────────────

    #[test]
    fn new_abstract_class_is_flagged() {
        let d = body("Object o = new Shape();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("abstract") && d[0].contains("Shape"), "{d:?}");
    }

    #[test]
    fn new_interface_is_flagged() {
        let d = body("Object o = new Runnable();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("interface") && d[0].contains("Runnable"), "{d:?}");
    }

    #[test]
    fn new_anonymous_abstract_is_not_flagged() {
        // `new Runnable() { … }` — anonymous concrete subclass, LEGAL → never flagged.
        assert!(body("Object o = new Runnable() { public void run() {} };").is_empty());
    }

    #[test]
    fn new_anonymous_abstract_class_is_not_flagged() {
        // Same for an abstract class with an anonymous body.
        assert!(body("Object o = new Shape() {};").is_empty());
    }

    #[test]
    fn new_concrete_class_is_not_flagged() {
        assert!(body("Object o = new Circle();").is_empty());
    }

    #[test]
    fn new_unknown_type_is_not_flagged() {
        // `Unknown` doesn't resolve → SKIP.
        assert!(body("Object o = new Unknown();").is_empty());
    }
}
