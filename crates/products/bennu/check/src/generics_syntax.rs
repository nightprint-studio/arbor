//! Generics- & static-context **syntax** errors — pure-AST rules that are hard compile errors the
//! Java grammar still parses cleanly, so tree-sitter accepts them but `javac` rejects them. All are
//! structurally detectable without a resolver, so (docs: never a false positive) they're computed
//! from node kinds + a locally-computed set of in-scope type-parameter names alone:
//!
//!   1. **Generic array creation** — `new List<String>[n]`: the array element type carries type
//!      arguments. `new String[]`, `new int[]`, `new List[]` (raw) are fine.
//!   2. **Instantiating a type parameter** — `new T(...)` where `T` is a type parameter in scope.
//!   3. **Generics in `instanceof`** — `x instanceof List<String>`; a lone unbounded `?` is legal.
//!   4. **Parameterized `catch` type** — `catch (Foo<X> e)`; exceptions can't be generic.
//!   5. **`this`/`super` in a static context** — inside a `static` method / initializer, with the
//!      inner-class carve-out so `this` bound to an inner instance stays legal.
//!
//! Soundness bias: every check flags ONLY the structurally-unambiguous case and SKIPs the moment the
//! shape is uncertain (see the per-check comments).

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// All generics/static-context syntax errors in `root`.
pub fn generics_syntax_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    generics_syntax_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn generics_syntax_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "array_creation_expression" => check_generic_array_creation(n, &mut out),
            "object_creation_expression" => check_type_param_instantiation(n, bytes, &mut out),
            "instanceof_expression" => check_instanceof_generics(n, &mut out),
            "catch_type" => check_catch_generics(n, &mut out),
            "this" | "super" => check_static_this_super(n, &mut out),
            _ => {}
        }
    }
    out
}

fn err(node: Node, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        message: message.into(),
        severity: "error".to_string(),
        code: String::new(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

// ── 1. generic array creation ────────────────────────────────────────────────

/// `new List<String>[...]` — flag when the array element type (`type` field) is itself a
/// `generic_type` (it CARRIES type arguments). A raw `new List[]` has a plain `type_identifier`
/// element type, `new String[]` / `new int[]` likewise, and `new @Ann String[]` keeps the annotation
/// on the `array_creation_expression` (not inside the element type) — none is a `generic_type`, so
/// none is flagged. Only the explicit-type-arguments shape is ever a `generic_type` here.
fn check_generic_array_creation(n: Node, out: &mut Vec<Diagnostic>) {
    if let Some(ty) = n.child_by_field_name("type") {
        if ty.kind() == "generic_type" {
            out.push(err(ty, "Generic array creation is not allowed"));
        }
    }
}

// ── 2. instantiating a type parameter ────────────────────────────────────────

/// `new T(...)` where `T` is a type parameter of an enclosing declaration. The created type (`type`
/// field) must be a BARE simple name (`type_identifier`) — a `generic_type`/`scoped_type_identifier`
/// is a real class, never a type variable — AND that name must be in the type-parameter set gathered
/// from every enclosing `*_declaration`. `new ArrayList<String>()` has a `generic_type` (skipped),
/// and `new Foo()` where `Foo` isn't a type param isn't in the set (skipped): both leave the check
/// silent. If no enclosing declaration exposes a computable set the name simply isn't found → SKIP.
fn check_type_param_instantiation(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(ty) = n.child_by_field_name("type") else { return };
    // Anonymous class body / diamond / qualified `new` all keep a richer `type` node — only a bare
    // `type_identifier` can denote a type variable.
    if ty.kind() != "type_identifier" {
        return;
    }
    let Ok(name) = ty.utf8_text(bytes) else { return };
    if type_params_in_scope(n, bytes).iter().any(|p| p == name) {
        out.push(err(ty, format!("Cannot instantiate the type parameter `{name}`")));
    }
}

/// The set of type-parameter names visible at `node`: the `type_identifier` name of each
/// `type_parameter` under a `type_parameters` list on ANY enclosing declaration
/// (class/interface/method/constructor/enum/record). Walking parents and reading the `type_parameters`
/// child directly (rather than a named field) keeps this robust across the declaration kinds.
fn type_params_in_scope(node: Node, bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cur = node.parent();
    while let Some(p) = cur {
        if p.kind().ends_with("_declaration") {
            collect_type_param_names(p, bytes, &mut names);
        }
        cur = p.parent();
    }
    names
}

/// Push the name of each `type_parameter` found in `decl`'s direct `type_parameters` child.
fn collect_type_param_names(decl: Node, bytes: &[u8], out: &mut Vec<String>) {
    let mut c = decl.walk();
    for ch in decl.children(&mut c) {
        if ch.kind() != "type_parameters" {
            continue;
        }
        let mut tc = ch.walk();
        for tp in ch.named_children(&mut tc) {
            if tp.kind() == "type_parameter" {
                // The name is the FIRST `type_identifier` child (`type_bound` holds the bound's own
                // `type_identifier`s, so pick the direct name, not the bound).
                let mut nc = tp.walk();
                let mut name = None;
                for x in tp.named_children(&mut nc) {
                    if x.kind() == "type_identifier" {
                        name = Some(x);
                        break;
                    }
                }
                if let Some(name) = name {
                    if let Ok(t) = name.utf8_text(bytes) {
                        out.push(t.to_string());
                    }
                }
            }
        }
    }
}

// ── 3. generics in `instanceof` ──────────────────────────────────────────────

/// `x instanceof List<String>` — flag when the type operand (`right` field) is a `generic_type` whose
/// type arguments are NOT a single unbounded wildcard. `List<?>` (the only legal generic instanceof)
/// has exactly one `wildcard` argument with no bound → skipped. `String` / `List` (raw) aren't
/// `generic_type` → skipped.
fn check_instanceof_generics(n: Node, out: &mut Vec<Diagnostic>) {
    let Some(ty) = n.child_by_field_name("right") else { return };
    if ty.kind() != "generic_type" {
        return;
    }
    if type_arguments_are_all_unbounded_wildcards(ty) {
        return;
    }
    out.push(err(
        ty,
        "Cannot use generics in an `instanceof` check (only unbounded wildcards are allowed)",
    ));
}

/// Whether every argument of a `generic_type`'s `type_arguments` is an UNBOUNDED wildcard (`?` with
/// no `extends`/`super` bound). Returns `true` only when there is at least one argument and all are
/// bare wildcards — so a concrete `List<String>` or a bounded `List<? extends X>` returns `false` and
/// gets flagged. A bounded wildcard is still a compile error for `instanceof`, so treating it as
/// "flag" is correct; only the lone bare `?` is exempt.
fn type_arguments_are_all_unbounded_wildcards(generic_ty: Node) -> bool {
    let mut c = generic_ty.walk();
    let Some(args) = generic_ty.children(&mut c).find(|ch| ch.kind() == "type_arguments") else {
        return false;
    };
    let mut ac = args.walk();
    let mut any = false;
    for arg in args.named_children(&mut ac) {
        any = true;
        // Only a `wildcard` argument can be exempt; a concrete `_type` argument disqualifies at once.
        if arg.kind() != "wildcard" || wildcard_is_bounded(arg) {
            return false;
        }
    }
    any
}

/// Whether a `wildcard` carries an `extends`/`super` bound — i.e. it's NOT the bare unbounded `?`.
/// A bound shows up as a `_type` child (`? extends X`) or a `super` child (`? super X`); a plain `?`
/// (even with a leading annotation) has neither, so it stays exempt.
fn wildcard_is_bounded(wildcard: Node) -> bool {
    // Any named child that isn't a leading annotation is a bound (`_type` for `extends`, or a `super`
    // token for `? super X`). A bare `?` (optionally annotated) has none → unbounded.
    let mut c = wildcard.walk();
    for ch in wildcard.named_children(&mut c) {
        if !matches!(ch.kind(), "annotation" | "marker_annotation") {
            return true;
        }
    }
    false
}

// ── 4. parameterized `catch` type ────────────────────────────────────────────

/// `catch (Foo<X> e)` — exceptions can't be generic. A `catch_type`'s children are the alternatives
/// (`_unannotated_type`); flag each that is a `generic_type`. Multi-catch `catch (A<X> | B e)` flags
/// only the parameterized member `A<X>`, leaving the plain `B` alone.
fn check_catch_generics(n: Node, out: &mut Vec<Diagnostic>) {
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        if ch.kind() == "generic_type" {
            out.push(err(ch, "Cannot use a parameterized type in a `catch` clause"));
        }
    }
}

// ── 5. `this` / `super` in a static context ──────────────────────────────────

/// A `this`/`super` used where no enclosing instance exists: inside a `static` method or a
/// `static_initializer` block (a `static` field initializer lives inside a `field_declaration` with a
/// `static` modifier — reached the same way).
///
/// Soundness carve-out: if an `object_creation_expression` with an anonymous class body OR a nested
/// type `*_declaration` sits BETWEEN this node and the static member, the `this`/`super` refers to
/// that inner instance (legal) — so we SKIP. We also skip a `super` that is part of a `wildcard`
/// (`? super X`), which is a type, not an expression. When the enclosing static-ness can't be
/// determined we SKIP.
fn check_static_this_super(n: Node, out: &mut Vec<Diagnostic>) {
    // `? super X` — the `super` here is a type-argument token, never an expression.
    if n.parent().map(|p| p.kind() == "wildcard").unwrap_or(false) {
        return;
    }
    let mut cur = n.parent();
    while let Some(p) = cur {
        match p.kind() {
            // An intervening inner instance context → this/super binds to it, legal → SKIP.
            "object_creation_expression" if has_anonymous_body(p) => return,
            "class_declaration" | "interface_declaration" | "enum_declaration"
            | "record_declaration" | "annotation_type_declaration" => return,
            // A lambda does NOT introduce a new `this` — keep walking (a `this` in a lambda inside a
            // static method is still illegal).
            "static_initializer" => {
                out.push(err(n, "Cannot use `this`/`super` in a static context"));
                return;
            }
            "method_declaration" => {
                if is_static_decl(p) {
                    out.push(err(n, "Cannot use `this`/`super` in a static context"));
                }
                // Reached the nearest enclosing method: static-ness resolved either way → stop.
                return;
            }
            // A static field initializer: `static Foo f = this;` — the `this` is under the
            // `field_declaration`, outside any method/initializer block.
            "field_declaration" if is_static_decl(p) => {
                out.push(err(n, "Cannot use `this`/`super` in a static context"));
                return;
            }
            // A constructor/instance-initializer block always has an instance → legal → SKIP.
            "constructor_declaration" | "compact_constructor_declaration" => return,
            _ => {}
        }
        cur = p.parent();
    }
    // Fell off the top without a decisive enclosing member → SKIP (never assume static).
}

/// Whether an `object_creation_expression` declares an anonymous class body (`new Foo() { ... }`) —
/// only then does it introduce a fresh `this`.
fn has_anonymous_body(n: Node) -> bool {
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        if ch.kind() == "class_body" {
            return true;
        }
    }
    false
}

/// Whether a declaration node carries the `static` keyword modifier (anonymous token inside its
/// `modifiers` child) — the same shape `declarations.rs` reads.
fn is_static_decl(decl: Node) -> bool {
    let mut c = decl.walk();
    for ch in decl.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() && m.kind() == "static" {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn errs(src: &str) -> Vec<String> {
        let tree = parse(src);
        generics_syntax_errors(tree.root_node(), src).into_iter().map(|d| d.message).collect()
    }

    fn has(src: &str, needle: &str) -> bool {
        errs(src).iter().any(|m| m.contains(needle))
    }

    // ── 1. generic array creation ──────────────────────────────────────────────

    #[test]
    fn generic_array_creation_flagged() {
        assert!(has("class C { Object m() { return new java.util.List<String>[0]; } }", "Generic array"));
        assert!(has("class C { Object m() { return new Map<K, V>[3]; } }", "Generic array"));
    }

    #[test]
    fn plain_and_raw_array_creation_not_flagged() {
        // String[], int[], raw List[] — none carries type arguments.
        assert!(!has("class C { Object m() { return new String[4]; } }", "Generic array"));
        assert!(!has("class C { Object m() { return new int[3]; } }", "Generic array"));
        assert!(!has("class C { Object m() { return new java.util.List[2]; } }", "Generic array"));
        // Annotated element type stays on the array node, not the element type.
        assert!(!has("class C { Object m() { return new @Ann String[1]; } }", "Generic array"));
    }

    // ── 2. instantiating a type parameter ──────────────────────────────────────

    #[test]
    fn instantiating_type_param_flagged() {
        assert!(has("class C<T> { T make() { return new T(); } }", "instantiate the type parameter `T`"));
        assert!(has(
            "class C { <U> U make() { return new U(); } }",
            "instantiate the type parameter `U`",
        ));
    }

    #[test]
    fn instantiating_real_type_not_flagged() {
        // ArrayList is NOT a type param → skip; and with a generic_type it isn't even a bare name.
        assert!(!has(
            "class C<T> { java.util.List<String> m() { return new java.util.ArrayList<String>(); } }",
            "instantiate the type parameter",
        ));
        // A bare `new Foo()` where Foo isn't a type parameter → skip.
        assert!(!has("class C<T> { Object m() { return new Foo(); } }", "instantiate the type parameter"));
    }

    // ── 3. generics in `instanceof` ────────────────────────────────────────────

    #[test]
    fn generic_instanceof_flagged() {
        assert!(has("class C { boolean m(Object x) { return x instanceof java.util.List<String>; } }", "instanceof"));
        // A bounded wildcard is still illegal in instanceof.
        assert!(has("class C { boolean m(Object x) { return x instanceof java.util.List<? extends Number>; } }", "instanceof"));
    }

    #[test]
    fn unbounded_wildcard_and_raw_instanceof_not_flagged() {
        assert!(!has("class C { boolean m(Object x) { return x instanceof java.util.List<?>; } }", "instanceof"));
        assert!(!has("class C { boolean m(Object x) { return x instanceof String; } }", "instanceof"));
        assert!(!has("class C { boolean m(Object x) { return x instanceof java.util.List; } }", "instanceof"));
    }

    // ── 4. parameterized `catch` type ──────────────────────────────────────────

    #[test]
    fn generic_catch_flagged() {
        assert!(has(
            "class C { void m() { try {} catch (Foo<X> e) {} } }",
            "parameterized type in a `catch`",
        ));
        // Multi-catch: only the parameterized member is flagged.
        let e = errs("class C { void m() { try {} catch (A<X> | B e) {} } }");
        assert_eq!(e.iter().filter(|m| m.contains("catch")).count(), 1, "{e:?}");
    }

    #[test]
    fn plain_catch_not_flagged() {
        assert!(!has(
            "class C { void m() { try {} catch (java.io.IOException e) {} } }",
            "catch",
        ));
        assert!(!has(
            "class C { void m() { try {} catch (RuntimeException | Error e) {} } }",
            "catch",
        ));
    }

    // ── 5. `this` / `super` in a static context ────────────────────────────────

    #[test]
    fn this_super_in_static_context_flagged() {
        assert!(has("class C { static Object m() { return this; } }", "static context"));
        assert!(has("class C { Object f; static { C.class.getName(); Runnable r = () -> { Object x = this; }; } }", "static context"));
        // static field initializer referring to `this`.
        assert!(has("class C { static Object F = this; }", "static context"));
    }

    #[test]
    fn this_in_instance_method_not_flagged() {
        assert!(!has("class C { Object m() { return this; } }", "static context"));
        assert!(!has("class C { C() { Object x = this; } }", "static context"));
    }

    #[test]
    fn this_in_anonymous_class_inside_static_not_flagged() {
        // The `this` binds to the anonymous Runnable instance, which is legal even though the enclosing
        // method is static.
        let src = "class C { static Runnable m() { return new Runnable() { public void run() { Object x = this; } }; } }";
        assert!(!has(src, "static context"), "{:?}", errs(src));
    }

    #[test]
    fn super_in_static_context_flagged_but_wildcard_super_not() {
        assert!(has("class C { static String m() { return super.toString(); } }", "static context"));
        // `? super X` — the `super` token is a type argument, never flagged as an expression.
        assert!(!has(
            "class C { static void m(java.util.List<? super Integer> l) {} }",
            "static context",
        ));
    }
}
