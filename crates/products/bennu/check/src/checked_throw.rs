//! Resolver-backed **unhandled checked exception** diagnostic (`"error"`). A directly-thrown checked
//! exception — `throw new SomeCheckedException(...)` — that is neither caught by an enclosing `try`
//! nor declared in the enclosing method/constructor's `throws` clause is a compile error.
//!
//! This is the NARROW, SOUND subset of Java's definite-assignment-style exception analysis: we only
//! reason about a LITERAL `throw new T(...)`, where `T` is syntactic. We deliberately do NOT analyze
//! exceptions propagated by CALLED methods (that needs each callee's `throws`, which we won't chase),
//! so the check can never over-report a "method call might throw" case it can't see.
//!
//! Soundness (docs: NEVER a false positive; under-reporting is fine — this is exception flow, so we
//! are EXTRA conservative). Every positive conclusion rests on a FULLY-KNOWN hierarchy:
//!
//!   * the thrown type must resolve AND its whole hierarchy up to `java/lang/Throwable` must be known
//!     ([`hierarchy_fully_known`]) — otherwise we can't classify checked-vs-unchecked → SKIP;
//!   * "checked" means the type reaches `Throwable` but reaches NEITHER `RuntimeException` NOR `Error`.
//!     [`reaches`] is conservative (an unknown link short-circuits to `true`), so gating on
//!     `hierarchy_fully_known` first makes the negative facts ("does NOT reach RuntimeException/Error")
//!     trustworthy — no unknown link could have hidden an unchecked ancestor;
//!   * "caught" and "declared" tests use [`reaches`] on a FULLY-KNOWN thrown type as well: if the
//!     thrown type reaches (== or subtype of) any catch/declared type, it's handled → SKIP.
//!
//! We only handle a `throw` whose NEAREST enclosing callable is a plain `method_declaration` /
//! `constructor_declaration`. Anything trickier (lambda, anonymous/local class, initializer block)
//! → SKIP, because the throws-ability there depends on machinery we don't model.

use bennu_java::prelude::{FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::members::simple_name;
use crate::resolve::type_binary;
use crate::walk::{hierarchy_fully_known, reaches};

/// The root of the checked-exception classification: reaching this (but not the two unchecked roots
/// below) is what makes a type "checked".
const THROWABLE: &str = "java/lang/Throwable";
/// Reaching either of these two roots makes a type UNCHECKED → never flagged.
const RUNTIME_EXCEPTION: &str = "java/lang/RuntimeException";
const ERROR: &str = "java/lang/Error";

/// Parse `source` and flag directly-thrown checked exceptions that are neither caught nor declared.
pub fn checked_throw_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let nodes = crate::check::collect_nodes(tree.root_node());
    checked_throw_errors_in(&nodes, source, &symbols, resolver)
}

/// Tree-driven core: iterates the shared `nodes` + reuses the caller's `symbols`. Mirrors
/// `exceptions::exception_errors_in`.
pub fn checked_throw_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "throw_statement" {
            check_throw(n, bytes, symbols, resolver, &mut out);
        }
    }
    out
}

/// Classify one `throw_statement` and, if it's an unhandled checked `throw new T(...)`, flag it.
fn check_throw(
    throw: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    // RULE 1: the thrown expression must be a literal `new T(...)` so the type is syntactic. Any other
    // `throw expr` (a variable, a method call result, a cast, a rethrow of a caught param) → SKIP: we
    // can't read the runtime type off the AST, and inferring it risks a false positive.
    let Some(creation) = throw_object_creation(throw) else { return };
    // The `type` field of the object-creation is the thrown class as written (`new IOException(...)`).
    let Some(type_node) = creation.child_by_field_name("type") else { return };
    let Ok(text) = type_node.utf8_text(bytes) else { return };

    // RULE 2a: the type must resolve to a binary name. Unresolvable → SKIP (we don't know what it is).
    let Some(thrown) = type_binary(text, symbols, resolver) else { return };
    // RULE 2b: the WHOLE hierarchy up to Throwable must be known. A gap means an unknown ancestor could
    // be RuntimeException/Error (making it unchecked) — so we couldn't trust the "checked" verdict → SKIP.
    if !hierarchy_fully_known(resolver, &thrown) {
        return;
    }
    // RULE 3: must be CHECKED — reaches Throwable, but NOT RuntimeException and NOT Error.
    //   * doesn't reach Throwable → not an exception type we understand → SKIP;
    //   * reaches RuntimeException or Error → UNCHECKED → SKIP.
    // (Gated by fully-known above, so these `reaches` negatives are real, not conservative-`true`.)
    if !is_checked(resolver, &thrown) {
        return;
    }

    // RULE 6: the NEAREST enclosing callable must be a plain method/constructor. If a lambda or an
    // anonymous/local class sits between the throw and that callable, SKIP: throws-ability there is
    // governed by the functional interface's SAM / the inner method's own `throws`, which we don't
    // analyze. `enclosing_callable` returns the nearest boundary and its kind so we can gate on it.
    let Some(callable) = enclosing_callable(throw) else { return }; // no enclosing callable → SKIP
    if !matches!(callable.kind(), "method_declaration" | "constructor_declaration") {
        // A lambda_expression (or, defensively, anything else that boundaries a scope) → SKIP.
        return;
    }
    // The callable is a real method/ctor, but if it's a member of an ANONYMOUS or LOCAL class its
    // `throws` contract isn't authoritative (an anonymous `new Runnable(){ run(){…} }` implements a
    // SAM whose signature — which may itself declare `throws` — we don't resolve; a local class's
    // method is equally out of our simple model). `enclosing_callable` returns the nearest callable,
    // which for `new Runnable(){ public void run(){ throw … } }` is `run` (found before its enclosing
    // anonymous body), so we must check the callable's OWN enclosing type here. → SKIP.
    if callable_in_synthetic_type(callable) {
        return;
    }
    // RULE 7 (implied by the above): a static/instance initializer block is NOT a
    // method_declaration/constructor_declaration, so its throw never reaches here — initializers SKIP.

    // RULE 4: handled by an enclosing `try` whose `catch` catches `T` or a supertype of `T`? If so → SKIP.
    // We walk only the `try`s that ENCLOSE the throw via their try BLOCK (not via a catch/finally of
    // that same try), stopping at the callable boundary. A nested try closer to the throw is checked
    // first, but since ALL enclosing trys are consulted, "any catches it" short-circuits correctly.
    if caught_by_enclosing_try(throw, callable, bytes, symbols, resolver, &thrown) {
        return;
    }

    // RULE 5: declared by the enclosing method/constructor's `throws` clause (`T` or a supertype)? → SKIP.
    if declared_by_callable(callable, bytes, symbols, resolver, &thrown) {
        return;
    }

    // Survived every SKIP: a checked exception thrown directly, not caught, not declared → error.
    out.push(err(
        format!(
            "Unhandled exception: `{}` must be caught or declared to be thrown",
            simple_name(&thrown)
        ),
        throw,
    ));
}

/// The `object_creation_expression` a `throw` throws directly, i.e. `throw new T(...)`. `None` for any
/// other thrown expression (variable, call, cast, …) — RULE 1: only a literal `new` gives a syntactic type.
fn throw_object_creation(throw: Node) -> Option<Node> {
    // A `throw_statement` wraps the thrown expression as its single named child.
    let mut c = throw.walk();
    for ch in throw.named_children(&mut c) {
        if ch.kind() == "object_creation_expression" {
            return Some(ch);
        }
        // Any other expression kind as the thrown value → not a literal `new T(...)` → SKIP.
        return None;
    }
    None
}

/// Whether `thrown` is a CHECKED exception: reaches `Throwable`, but reaches neither `RuntimeException`
/// nor `Error`. Caller MUST have confirmed `hierarchy_fully_known(thrown)` so these `reaches` results
/// are trustworthy (no unknown link short-circuited a `reaches` to `true`, and no unchecked ancestor
/// could be hidden behind a gap).
pub(crate) fn is_checked(resolver: &dyn TypeResolver, thrown: &str) -> bool {
    reaches(resolver, thrown, THROWABLE)
        && !reaches(resolver, thrown, RUNTIME_EXCEPTION)
        && !reaches(resolver, thrown, ERROR)
}

/// The nearest enclosing scope boundary of `throw`. Returns the FIRST ancestor that is a callable
/// (`method_declaration`/`constructor_declaration`), a `lambda_expression`, or a nested type body —
/// so the caller can tell "plain method/ctor" (handle) from "lambda / inner class in between" (SKIP).
/// An `object_creation_expression` with an anonymous `class_body` and a local type declaration both
/// count as boundaries: a `throw` inside them belongs to an inner method/SAM, not the outer callable.
/// Whether `callable` (a `method_declaration`/`constructor_declaration`) is declared inside an
/// ANONYMOUS class (`new T(){ … }`) or a LOCAL class (a type declared inside a method body). In both
/// the method's effective `throws` contract is governed by machinery we don't model (the SAM it
/// implements, or an enclosing capture), so a checked throw there is SKIPped to stay sound. Walks up
/// to the callable's own enclosing type body and inspects who owns it.
pub(crate) fn callable_in_synthetic_type(callable: Node) -> bool {
    let mut cur = callable.parent();
    while let Some(n) = cur {
        if matches!(n.kind(), "class_body" | "enum_body") {
            let Some(owner) = n.parent() else { return false };
            return match owner.kind() {
                // `new Runnable() { … }` — anonymous class.
                "object_creation_expression" | "enum_constant" => true,
                // A named type whose parent is a `block` is a LOCAL class (declared inside a method).
                "class_declaration" | "enum_declaration" | "record_declaration" => {
                    owner.parent().map(|p| p.kind() == "block").unwrap_or(false)
                }
                // A normal top-level or nested member type → the callable's contract is authoritative.
                _ => false,
            };
        }
        cur = n.parent();
    }
    false
}

pub(crate) fn enclosing_callable(throw: Node) -> Option<Node> {
    let mut cur = throw.parent();
    while let Some(n) = cur {
        match n.kind() {
            // The two callables we actually handle.
            "method_declaration" | "constructor_declaration" => return Some(n),
            // A boundary we DON'T handle — returning it lets the caller SKIP (its kind isn't a
            // method/ctor). A lambda's throws-ability depends on the target functional interface.
            "lambda_expression" => return Some(n),
            // An anonymous/local class between the throw and the outer callable: the throw is inside
            // some inner member, whose throws we don't model. Return the boundary node so the caller,
            // seeing a non-callable kind, SKIPs. (Covers `new Runnable(){ public void run(){ throw…} }`
            // and any local `class`/`interface`/`enum`/`record`.)
            "class_body"
            | "interface_body"
            | "enum_body"
            | "annotation_type_body"
            | "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
            // Initializer blocks (RULE 7) — a checked throw here is technically an error, but we stay
            // safe and only handle method/ctor bodies, so treat the initializer as a non-callable
            // boundary → SKIP.
            | "static_initializer" => return Some(n),
            _ => cur = n.parent(),
        }
    }
    None
}

/// RULE 4: whether some `try` enclosing `throw` (up to, but not past, `callable`) catches `thrown`
/// via one of its `catch` clauses — with the throw located in that try's BLOCK (a throw sitting in a
/// try's own catch/finally is NOT protected by that try). Consults every enclosing try; a nested one
/// naturally comes first but any match suffices.
pub(crate) fn caught_by_enclosing_try(
    throw: Node,
    callable: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    thrown: &str,
) -> bool {
    // Walk ancestors; remember the child we came UP through so we can tell whether we entered a
    // `try_statement` via its BLOCK (protected) vs its catch/finally (not protected).
    let mut child = throw;
    let mut cur = throw.parent();
    while let Some(n) = cur {
        // Stop at the callable boundary — a try outside the callable can't catch a throw inside it via
        // normal flow (and we've already established `callable` is the nearest callable).
        if n.id() == callable.id() {
            return false;
        }
        if matches!(n.kind(), "try_statement" | "try_with_resources_statement") {
            // Only protected if `child` is the try's BLOCK (field `body`), not a catch/finally clause.
            if is_try_body(n, child) && try_catches(n, bytes, symbols, resolver, thrown) {
                return true;
            }
            // If we entered via a catch/finally of this try, this try does NOT protect the throw; keep
            // walking outward (an OUTER try still might catch it).
        }
        child = n;
        cur = n.parent();
    }
    false
}

/// Whether `child` is the try's protected BLOCK (its `body`), as opposed to a `catch_clause` /
/// `finally_clause`. A throw in the try's own catch/finally is not caught by that try.
fn is_try_body(try_node: Node, child: Node) -> bool {
    match try_node.child_by_field_name("body") {
        Some(body) => body.id() == child.id(),
        // `try_with_resources_statement` may not expose a `body` field on every grammar build; fall
        // back to "the child is a `block` and not a catch/finally clause". Conservative: if we can't
        // be sure it's the body, `child.kind() == "block"` still excludes catch/finally clauses.
        None => child.kind() == "block",
    }
}

/// Whether any `catch_clause` of `try_node` catches `thrown` — i.e. one of its catch alternatives
/// resolves to `thrown` itself or a supertype of `thrown` (`reaches(thrown, catchType)`). Unresolvable
/// catch types are ignored (we can't prove they catch it → don't rely on them, stay conservative:
/// under-catching here can only ADD a diagnostic, so to avoid a false positive we must be careful —
/// but note this direction is safe because a missed catch means we'd flag; hence we only treat a catch
/// as protective when the thrown type PROVABLY reaches it over the fully-known thrown hierarchy).
fn try_catches(
    try_node: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    thrown: &str,
) -> bool {
    let mut c = try_node.walk();
    for clause in try_node.named_children(&mut c) {
        if clause.kind() != "catch_clause" {
            continue;
        }
        for catch_binary in clause_catch_types(clause, bytes, symbols, resolver) {
            // `thrown` reaches `catch_binary` ⟺ thrown is catch_binary or a subtype ⟺ this catch
            // handles it. `thrown`'s hierarchy is fully known (checked earlier), so a `reaches` `true`
            // is a real is-a, and a `false` is a real "not caught" — no conservative over-catch.
            if reaches(resolver, thrown, &catch_binary) {
                return true;
            }
        }
    }
    false
}

/// The resolved binary names of one `catch_clause`'s alternatives (mirrors `exceptions::clause_types`,
/// but we only need the binary strings here). Unresolvable alternatives are dropped.
fn clause_catch_types(
    clause: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<String> {
    let mut out = Vec::new();
    let Some(param) = child_of_kind(clause, "catch_formal_parameter") else { return out };
    let Some(catch_type) = child_of_kind(param, "catch_type") else { return out };
    let mut c = catch_type.walk();
    for ty in catch_type.named_children(&mut c) {
        if !is_type_node(ty.kind()) {
            continue;
        }
        let Ok(text) = ty.utf8_text(bytes) else { continue };
        if let Some(binary) = type_binary(text, symbols, resolver) {
            out.push(binary);
        }
    }
    out
}

/// RULE 5: whether the enclosing method/constructor declares `thrown` (or a supertype) in its `throws`
/// clause — `reaches(thrown, declared)` for any declared type. The `throws` clause is a `throws` child
/// node listing type nodes. Unresolvable declared types are ignored (can't prove they cover it).
pub(crate) fn declared_by_callable(
    callable: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    thrown: &str,
) -> bool {
    let Some(throws_node) = child_of_kind(callable, "throws") else { return false };
    let mut c = throws_node.walk();
    for ty in throws_node.named_children(&mut c) {
        if !is_type_node(ty.kind()) {
            continue;
        }
        let Ok(text) = ty.utf8_text(bytes) else { continue };
        let Some(declared) = type_binary(text, symbols, resolver) else { continue };
        // `thrown` reaches `declared` ⟺ thrown is declared or a subtype of it ⟺ declaring `declared`
        // covers throwing `thrown`. Fully-known thrown hierarchy → this negative/positive is trustworthy.
        if reaches(resolver, thrown, &declared) {
            return true;
        }
    }
    false
}

// ── CST helpers (mirrors exceptions.rs) ───────────────────────────────────────────────────────────

/// The first direct named child of `n` with the given kind.
fn child_of_kind<'t>(n: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == kind {
            return Some(ch);
        }
    }
    None
}

/// A type node that names a class/interface (a catch/throws alternative).
fn is_type_node(kind: &str) -> bool {
    matches!(kind, "type_identifier" | "scoped_type_identifier" | "generic_type")
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic {
        message,
        severity: "error".to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// The same mock-resolver shape as `exceptions.rs`.
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

    fn cm(superclass: Option<&str>, ifaces: &[&str], is_interface: bool) -> ClassMembers {
        let flags = ClassFlags { is_interface, ..ClassFlags::default() };
        ClassMembers {
            superclass: superclass.map(str::to_string),
            interfaces: ifaces.iter().map(|s| s.to_string()).collect(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags,
        }
    }

    /// `Object ← Throwable ← Exception ← {IOException, SQLException}` (checked), plus the unchecked
    /// branch `Exception ← RuntimeException ← IllegalStateException`. `RunawayEx` has an unknown base
    /// so its hierarchy is NOT fully known (used to prove the resolve-gap SKIP).
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".into(), cm(None, &[], false));
        members.insert("java/lang/Throwable".into(), cm(Some("java/lang/Object"), &[], false));
        members.insert("java/lang/Exception".into(), cm(Some("java/lang/Throwable"), &[], false));
        members.insert("java/io/IOException".into(), cm(Some("java/lang/Exception"), &[], false));
        members.insert("java/sql/SQLException".into(), cm(Some("java/lang/Exception"), &[], false));
        // Unchecked branch: RuntimeException extends Exception; IllegalStateException extends it.
        members.insert(
            "java/lang/RuntimeException".into(),
            cm(Some("java/lang/Exception"), &[], false),
        );
        members.insert(
            "java/lang/IllegalStateException".into(),
            cm(Some("java/lang/RuntimeException"), &[], false),
        );
        // Unknown-base type → hierarchy not fully known.
        members.insert("com/acme/RunawayEx".into(), cm(Some("com/acme/UnknownBase"), &[], false));

        let simple = [
            ("Exception", "java/lang/Exception"),
            ("Throwable", "java/lang/Throwable"),
            ("IOException", "java/io/IOException"),
            ("SQLException", "java/sql/SQLException"),
            ("RuntimeException", "java/lang/RuntimeException"),
            ("IllegalStateException", "java/lang/IllegalStateException"),
            ("RunawayEx", "com/acme/RunawayEx"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// Run against a whole class source (so methods/try parse cleanly).
    fn diags(src: &str) -> Vec<String> {
        checked_throw_errors(src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    // ── positives ─────────────────────────────────────────────────────────────────

    #[test]
    fn checked_throw_undeclared_is_flagged() {
        let d = diags("class C { void m() { throw new IOException(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(
            d[0].contains("Unhandled exception") && d[0].contains("IOException"),
            "{d:?}"
        );
    }

    #[test]
    fn checked_throw_in_plain_method_not_declared_is_flagged() {
        // Method declares nothing → still flagged.
        let d = diags("class C { void run() { int x = 1; throw new SQLException(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("SQLException"), "{d:?}");
    }

    #[test]
    fn checked_throw_in_constructor_undeclared_is_flagged() {
        let d = diags("class C { C() { throw new IOException(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("IOException"), "{d:?}");
    }

    #[test]
    fn caught_by_nested_try_but_rethrow_supertype_undeclared_is_flagged() {
        // The throw is caught by `SQLException` (a sibling), which does NOT catch IOException → flagged.
        let d = diags(
            "class C { void m() { try { throw new IOException(); } catch (SQLException e) {} } }",
        );
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("IOException"), "{d:?}");
    }

    // ── negatives ─────────────────────────────────────────────────────────────────

    #[test]
    fn declared_exact_type_is_ok() {
        assert!(
            diags("class C { void m() throws IOException { throw new IOException(); } }").is_empty()
        );
    }

    #[test]
    fn declared_supertype_is_ok() {
        // `throws Exception` covers throwing IOException (subtype).
        assert!(
            diags("class C { void m() throws Exception { throw new IOException(); } }").is_empty()
        );
    }

    #[test]
    fn caught_by_supertype_is_ok() {
        // `catch (Exception e)` catches IOException → handled.
        assert!(diags(
            "class C { void m() { try { throw new IOException(); } catch (Exception e) {} } }"
        )
        .is_empty());
    }

    #[test]
    fn caught_by_exact_type_is_ok() {
        assert!(diags(
            "class C { void m() { try { throw new IOException(); } catch (IOException e) {} } }"
        )
        .is_empty());
    }

    #[test]
    fn unchecked_illegal_state_is_ok() {
        assert!(diags("class C { void m() { throw new IllegalStateException(); } }").is_empty());
    }

    #[test]
    fn unchecked_runtime_exception_is_ok() {
        assert!(diags("class C { void m() { throw new RuntimeException(); } }").is_empty());
    }

    #[test]
    fn unresolvable_type_is_not_flagged() {
        // `Mystery` doesn't resolve → we can't classify it → SKIP.
        assert!(diags("class C { void m() { throw new Mystery(); } }").is_empty());
    }

    #[test]
    fn unknown_hierarchy_is_not_flagged() {
        // `RunawayEx` resolves but extends an UNKNOWN base → not fully known → can't classify → SKIP.
        assert!(diags("class C { void m() { throw new RunawayEx(); } }").is_empty());
    }

    #[test]
    fn throw_of_variable_is_not_flagged() {
        // Not a literal `new T(...)` — the thrown value is a variable → SKIP (RULE 1).
        assert!(diags(
            "class C { void m() { IOException e = null; throw e; } }"
        )
        .is_empty());
    }

    #[test]
    fn checked_throw_inside_lambda_is_not_flagged() {
        // The nearest enclosing callable is a lambda, not a method → SKIP (RULE 6). The lambda's
        // throws-ability depends on the target functional interface, which we don't analyze.
        assert!(diags(
            "class C { void m() { Runnable r = () -> { throw new IOException(); }; } }"
        )
        .is_empty());
    }

    #[test]
    fn checked_throw_inside_anonymous_class_is_not_flagged() {
        // The throw sits inside an anonymous class body's method → inner boundary → SKIP (RULE 6).
        assert!(diags(
            "class C { void m() { Runnable r = new Runnable() { public void run() { throw new IOException(); } }; } }"
        )
        .is_empty());
    }

    #[test]
    fn throw_in_try_but_caught_by_outer_try_is_ok() {
        // Nested trys: inner catches SQLException (miss), outer catches Exception (hit) → handled.
        assert!(diags(
            "class C { void m() { try { try { throw new IOException(); } catch (SQLException e) {} } catch (Exception e) {} } }"
        )
        .is_empty());
    }
}
