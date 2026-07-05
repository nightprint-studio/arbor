//! `final`-reassignment diagnostics (pure-AST): a `final` local or field that **already has an
//! initializer**, then gets assigned again.
//!
//! Conservative — only the definitely-illegal case is flagged. An initializer means the variable is
//! already definitely assigned, so *any* later assignment (`x = …`, `x += …`, `x++`) is illegal. A
//! `final` **without** an initializer (assigned once later, possibly across `if`/`else` branches — a
//! legal definite-assignment pattern) is deliberately **not** flagged: proving that safe would need
//! full definite-assignment analysis, so we skip it rather than risk a false positive.
//!
//! Two safe cases:
//!   * **local** — a `final` local with an initializer, reassigned in the same executable scope
//!     (method / constructor / lambda / `static` block). A name shadowed elsewhere in the scope is
//!     skipped (can't attribute the assignment). Assignments inside a nested lambda / local class are
//!     left to their own scope pass (and the lambda-capture check).
//!   * **field** — a `final` field with an initializer, reassigned through `this.field` (unambiguous)
//!     anywhere in the declaring type. A bare `field = …` is *not* flagged (a local could shadow it).

use std::collections::HashMap;

use bennu_java::prelude::{FileSymbols, MemberKind, TypeResolver, Visibility};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::resolve::type_binary;
use crate::walk::for_each_supertype;

/// Parse `source` and flag illegal reassignment of `final` locals and fields.
pub fn final_reassignment_errors(source: &str) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    match parser.parse(source, None) {
        Some(tree) => final_reassignment_errors_in(tree.root_node(), source),
        None => Vec::new(),
    }
}

/// Tree-driven core (shared with the `check_file` single-parse path).
pub fn final_reassignment_errors_in(root: Node, source: &str) -> Vec<Diagnostic> {
    final_reassignment_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core: iterate the shared pre-collected node list instead of re-walking the tree, so
/// the `check_file` aggregator pays for ONE traversal across all pure-AST checks. The inner
/// `check_final_locals` / `check_final_fields` are bounded sub-walks of the matched node.
pub fn final_reassignment_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            // Each executable scope owns its locals: a method / constructor body, a lambda body
            // block, a `static {}` initializer.
            "method_declaration" | "constructor_declaration" | "lambda_expression" => {
                if let Some(body) = n.child_by_field_name("body") {
                    if body.kind() == "block" {
                        check_final_locals(body, bytes, &mut out);
                    }
                }
            }
            "static_initializer" => {
                let mut c = n.walk();
                for ch in n.named_children(&mut c) {
                    if ch.kind() == "block" {
                        check_final_locals(ch, bytes, &mut out);
                    }
                }
            }
            "class_declaration" | "enum_declaration" => {
                check_final_fields(n, bytes, &mut out);
            }
            _ => {}
        }
    }
    out
}

// ── final locals ─────────────────────────────────────────────────────────────

/// Whether a declaration node carries the `final` modifier.
fn has_final(node: Node, bytes: &[u8]) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if m.utf8_text(bytes) == Ok("final") {
                    return true;
                }
            }
        }
    }
    false
}

/// A node that starts a *new* variable scope we must not cross when scanning one scope's locals
/// (a nested type or another executable scope owns its own names).
fn is_scope_boundary(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "lambda_expression"
    )
}

/// Flag illegal reassignments of `final` initialized locals declared directly in `scope` (a body
/// block), not crossing into nested scopes.
fn check_final_locals(scope: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    // (name → declaration count) across this scope — a name declared more than once (shadowing) is
    // skipped so an assignment is never attributed to the wrong declaration.
    let mut decl_count: HashMap<String, usize> = HashMap::new();
    // (name → end offset of the `final … = …;` declaration): the point after which any assignment is
    // illegal.
    let mut final_inited: HashMap<String, usize> = HashMap::new();
    // (name, target node) for every bare-identifier assignment / update in this scope.
    let mut assigns: Vec<(String, Node)> = Vec::new();

    // Walk the scope subtree, but do NOT descend into nested scopes (they own their names).
    let mut stack: Vec<Node> = Vec::new();
    let mut c = scope.walk();
    for ch in scope.named_children(&mut c) {
        stack.push(ch);
    }
    while let Some(n) = stack.pop() {
        match n.kind() {
            "variable_declarator" => {
                if let Some(name) = decl_name(n, bytes) {
                    *decl_count.entry(name).or_insert(0) += 1;
                }
            }
            "formal_parameter" | "spread_parameter" | "catch_formal_parameter" => {
                if let Some(name) = n.child_by_field_name("name").and_then(|x| text(x, bytes)) {
                    *decl_count.entry(name).or_insert(0) += 1;
                }
            }
            "assignment_expression" => {
                if let Some((name, node)) = assign_target_name(n, bytes) {
                    assigns.push((name, node));
                }
            }
            "update_expression" => {
                if let Some((name, node)) = update_target_name(n, bytes) {
                    assigns.push((name, node));
                }
            }
            _ => {}
        }
        if is_scope_boundary(n.kind()) {
            continue; // don't cross into a nested scope
        }
        let mut cc = n.walk();
        for ch in n.named_children(&mut cc) {
            stack.push(ch);
        }
    }

    // Collect final-initialized local declarations (separate pass so nested scopes are excluded the
    // same way).
    collect_final_inited_locals(scope, bytes, &mut final_inited);

    for (name, node) in assigns {
        let Some(&decl_end) = final_inited.get(&name) else { continue };
        if decl_count.get(&name).copied().unwrap_or(0) != 1 {
            continue; // shadowed → can't safely attribute
        }
        if node.start_byte() <= decl_end {
            continue; // the initializer itself, or a forward-reference (different binding)
        }
        out.push(err(format!("Cannot assign a value to final variable `{name}`"), node));
    }
}

/// Record every `final … = …` local's name → declaration-end offset, not crossing nested scopes.
fn collect_final_inited_locals(scope: Node, bytes: &[u8], out: &mut HashMap<String, usize>) {
    let mut stack: Vec<Node> = Vec::new();
    let mut c = scope.walk();
    for ch in scope.named_children(&mut c) {
        stack.push(ch);
    }
    while let Some(n) = stack.pop() {
        if n.kind() == "local_variable_declaration" && has_final(n, bytes) {
            let mut dc = n.walk();
            for d in n.named_children(&mut dc) {
                if d.kind() == "variable_declarator" && d.child_by_field_name("value").is_some() {
                    if let Some(name) = decl_name(d, bytes) {
                        out.insert(name, n.end_byte());
                    }
                }
            }
        }
        if is_scope_boundary(n.kind()) {
            continue;
        }
        let mut cc = n.walk();
        for ch in n.named_children(&mut cc) {
            stack.push(ch);
        }
    }
}

// ── final fields ─────────────────────────────────────────────────────────────

/// Flag `this.field = …` reassignments of a `final` field that already has an initializer, in the
/// type `n`'s own body (not descending into nested types — their `this` is a different object).
fn check_final_fields(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = n.child_by_field_name("body") else { return };

    // Final fields (declared directly in this body) that carry an initializer.
    let mut final_fields: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut bc = body.walk();
    for m in body.named_children(&mut bc) {
        if m.kind() == "field_declaration" && has_final(m, bytes) {
            let mut dc = m.walk();
            for d in m.named_children(&mut dc) {
                if d.kind() == "variable_declarator" && d.child_by_field_name("value").is_some() {
                    if let Some(name) = decl_name(d, bytes) {
                        final_fields.insert(name);
                    }
                }
            }
        }
    }
    if final_fields.is_empty() {
        return;
    }

    // Walk the type body for `this.field` assignment / update targets, not crossing into nested
    // type declarations.
    let mut stack: Vec<Node> = Vec::new();
    let mut c = body.walk();
    for ch in body.named_children(&mut c) {
        stack.push(ch);
    }
    while let Some(node) = stack.pop() {
        let target = match node.kind() {
            "assignment_expression" => node.child_by_field_name("left"),
            "update_expression" => update_operand(node),
            _ => None,
        };
        if let Some(t) = target {
            if let Some(field) = this_field_name(t, bytes) {
                if final_fields.contains(&field) {
                    out.push(err(
                        format!("Cannot assign a value to final field `{field}`"),
                        t,
                    ));
                }
            }
        }
        if matches!(
            node.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            continue; // nested type: its `this` is a different object
        }
        let mut cc = node.walk();
        for ch in node.named_children(&mut cc) {
            stack.push(ch);
        }
    }
}

// ── final-method override (resolver-backed) ──────────────────────────────────

/// Flag a method that overrides a `final` method inherited from a supertype (`final` methods can't be
/// overridden, JLS §8.4.3.3). Conservative: matches by name **and** erased parameter types (so a
/// legal overload with a different signature is never flagged), and only when every parameter type of
/// the overriding method resolves (otherwise the signature can't be confirmed → skipped). Fires
/// against `final` methods of both **library/JDK** supertypes (incl. `java.lang.Object`'s `final`
/// `wait`/`getClass`/…) and **project** supertypes.
pub fn final_override_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if matches!(n.kind(), "class_declaration" | "enum_declaration") {
            check_type_final_overrides(n, bytes, symbols, resolver, &mut out);
        }
    }
    out
}

fn check_type_final_overrides(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    out: &mut Vec<Diagnostic>,
) {
    let Some(body) = n.child_by_field_name("body") else { return };

    // Supertypes to scan: the explicit `extends` (if resolvable) plus `java/lang/Object` — whose
    // `final` methods (`wait`, `getClass`, …) apply to every class even with no explicit `extends`.
    let mut supers: Vec<String> = vec!["java/lang/Object".to_string()];
    if let Some(ext) = superclass_text(n, bytes) {
        if let Some(bin) = type_binary(&ext, symbols, resolver) {
            supers.push(bin);
        }
    }

    // name → the set of erased parameter-type lists of `final`, overridable supertype methods.
    let mut final_methods: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    for sup in &supers {
        for_each_supertype(resolver, sup, &mut |_bn, cm| {
            for m in &cm.methods {
                let overridable = m.kind == MemberKind::Method
                    && m.is_final
                    && !m.is_static
                    && m.visibility != Visibility::Private
                    && m.name != "<init>"
                    && m.name != "<clinit>";
                if overridable {
                    let params = m.params.iter().map(|p| p.binary_name.clone()).collect();
                    final_methods.entry(m.name.clone()).or_default().push(params);
                }
            }
        });
    }
    if final_methods.is_empty() {
        return;
    }

    // Each method declared directly in this type: does it override a collected final method?
    let mut bc = body.walk();
    for m in body.named_children(&mut bc) {
        if m.kind() != "method_declaration" {
            continue;
        }
        if has_static(m, bytes) || has_visibility(m, bytes, "private") {
            continue; // static / private methods don't override
        }
        let Some(name_node) = m.child_by_field_name("name") else { continue };
        let Some(name) = text(name_node, bytes) else { continue };
        let Some(candidates) = final_methods.get(&name) else { continue };
        let Some(params) = method_param_binaries(m, bytes, symbols, resolver) else { continue };
        if candidates.iter().any(|c| *c == params) {
            out.push(err(format!("Cannot override final method `{name}`"), name_node));
        }
    }
}

/// The erased binary names of a method's parameter types. `None` (skip the method) if any parameter
/// type can't be resolved, or the method is varargs (a conservative miss rather than a wrong match).
fn method_param_binaries(
    md: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<Vec<String>> {
    let params_node = md.child_by_field_name("parameters")?;
    let mut out = Vec::new();
    let mut c = params_node.walk();
    for p in params_node.named_children(&mut c) {
        match p.kind() {
            "formal_parameter" => {
                let ty = p.child_by_field_name("type")?;
                let text = ty.utf8_text(bytes).ok()?;
                out.push(type_binary(text, symbols, resolver)?);
            }
            "spread_parameter" => return None, // varargs — skip (erased-array matching is finicky)
            _ => {}
        }
    }
    Some(out)
}

/// The `extends` type text of a class (`superclass` wrapper), if any.
fn superclass_text(n: Node, bytes: &[u8]) -> Option<String> {
    let sc = n.child_by_field_name("superclass")?;
    let mut c = sc.walk();
    for ch in sc.named_children(&mut c) {
        if matches!(ch.kind(), "type_identifier" | "scoped_type_identifier" | "generic_type") {
            return text(ch, bytes);
        }
    }
    None
}

fn has_static(node: Node, bytes: &[u8]) -> bool {
    has_keyword_modifier(node, bytes, "static")
}

fn has_visibility(node: Node, bytes: &[u8], keyword: &str) -> bool {
    has_keyword_modifier(node, bytes, keyword)
}

fn has_keyword_modifier(node: Node, bytes: &[u8], keyword: &str) -> bool {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            if let Ok(t) = ch.utf8_text(bytes) {
                return t.split_whitespace().any(|w| w == keyword);
            }
        }
    }
    false
}

// ── shared node helpers ──────────────────────────────────────────────────────

fn text(node: Node, bytes: &[u8]) -> Option<String> {
    node.utf8_text(bytes).ok().map(str::to_string)
}

fn decl_name(declarator: Node, bytes: &[u8]) -> Option<String> {
    declarator.child_by_field_name("name").and_then(|n| text(n, bytes))
}

/// The bare-identifier target of an assignment (`x = …`), else `None` (a `this.x`/`a[i]`/`o.f` LHS
/// is handled elsewhere or not a simple-variable assignment).
fn assign_target_name<'t>(assign: Node<'t>, bytes: &[u8]) -> Option<(String, Node<'t>)> {
    let left = assign.child_by_field_name("left")?;
    if left.kind() == "identifier" {
        return text(left, bytes).map(|s| (s, left));
    }
    None
}

/// The bare-identifier operand of an update expression (`x++`, `--x`).
fn update_target_name<'t>(update: Node<'t>, bytes: &[u8]) -> Option<(String, Node<'t>)> {
    let op = update_operand(update)?;
    if op.kind() == "identifier" {
        return text(op, bytes).map(|s| (s, op));
    }
    None
}

/// The operand node of an `update_expression` (the identifier / field-access being ++/--'d).
fn update_operand(update: Node) -> Option<Node> {
    let mut c = update.walk();
    for ch in update.named_children(&mut c) {
        // The only named child is the operand; the `++`/`--` is an anonymous token.
        return Some(ch);
    }
    None
}

/// If `node` is a `this.field` access, its field name.
fn this_field_name(node: Node, bytes: &[u8]) -> Option<String> {
    if node.kind() != "field_access" {
        return None;
    }
    let object = node.child_by_field_name("object")?;
    if object.kind() != "this" {
        return None;
    }
    node.child_by_field_name("field").and_then(|f| text(f, bytes))
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

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn errs(src: &str) -> Vec<String> {
        final_reassignment_errors(src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn final_local_reassignment_is_flagged() {
        let src = "class C { void m() { final int x = 1; x = 2; } }";
        let d = errs(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("final variable `x`"), "{d:?}");
    }

    #[test]
    fn final_local_compound_and_increment_are_flagged() {
        assert_eq!(errs("class C { void m() { final int x = 1; x += 2; } }").len(), 1);
        assert_eq!(errs("class C { void m() { final int x = 1; x++; } }").len(), 1);
    }

    #[test]
    fn final_local_without_initializer_is_not_flagged() {
        // Assigned once later (possibly across if/else) — legal definite assignment, never flagged.
        let src = "class C { void m(boolean b) { final int x; if (b) { x = 1; } else { x = 2; } } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn non_final_local_reassignment_is_ok() {
        assert!(errs("class C { void m() { int x = 1; x = 2; } }").is_empty());
    }

    #[test]
    fn shadowed_final_name_is_not_flagged() {
        // Two declarations of `x` in the same method → can't attribute the assignment safely.
        let src = "class C { void m() { final int x = 1; { int x = 5; x = 6; } } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn final_field_this_reassignment_is_flagged() {
        let src = "class C { final int x = 1; void m() { this.x = 2; } }";
        let d = errs(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("final field `x`"), "{d:?}");
    }

    #[test]
    fn final_field_without_initializer_is_not_flagged() {
        // Assigned once in the constructor — legal, never flagged.
        let src = "class C { final int x; C() { this.x = 1; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn non_final_field_this_reassignment_is_ok() {
        assert!(errs("class C { int x = 1; void m() { this.x = 2; } }").is_empty());
    }

    #[test]
    fn nested_type_this_is_not_confused() {
        // The inner class's `this.x` targets the inner field, not the outer final one.
        let src = "class C { final int x = 1; class Inner { int x; void m() { this.x = 2; } } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    // ── final-method override (resolver-backed) ────────────────────────────────

    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap as Map;
    use std::sync::Arc;

    struct MapResolver {
        members: Map<String, ClassMembers>,
        simple: Map<String, String>,
    }
    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _i: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn method(name: &str, params: &[&str], is_final: bool) -> Member {
        let params = params.iter().map(|p| TypeRef::simple(p.to_string())).collect();
        let m = Member::method(name, TypeRef::simple("void"), params);
        if is_final {
            m.final_()
        } else {
            m
        }
    }

    /// A `Base` with a `final run()`, a `final foo(String)`, and a non-final `ok()`.
    fn resolver() -> MapResolver {
        let base = ClassMembers {
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: vec![
                method("run", &[], true),
                method("foo", &["java/lang/String"], true),
                method("ok", &[], false),
            ],
            fields: Vec::new(),
            flags: ClassFlags::default(),
        };
        let mut members = Map::new();
        members.insert("com/acme/Base".to_string(), base);
        let simple = [("Base", "com/acme/Base"), ("String", "java/lang/String")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        MapResolver { members, simple }
    }

    fn overrides(src: &str) -> Vec<String> {
        let symbols = bennu_java::prelude::extract_symbols(src);
        let tree = parse(src);
        let nodes = crate::check::collect_nodes(tree.root_node());
        final_override_errors_in(&nodes, src, &symbols, &resolver())
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn overriding_final_method_is_flagged() {
        let d = overrides("class X extends Base { void run() {} }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("final method `run`"), "{d:?}");
    }

    #[test]
    fn overriding_final_method_with_param_is_flagged() {
        let d = overrides("class X extends Base { void foo(String s) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`foo`"), "{d:?}");
    }

    #[test]
    fn overloading_a_final_method_is_ok() {
        // `run(int)` is a different signature than the final `run()` → an overload, legal.
        assert!(overrides("class X extends Base { void run(int x) {} }").is_empty());
        // `foo(int)` ≠ final `foo(String)` → overload, legal.
        assert!(overrides("class X extends Base { void foo(int x) {} }").is_empty());
    }

    #[test]
    fn overriding_non_final_method_is_ok() {
        assert!(overrides("class X extends Base { void ok() {} }").is_empty());
    }

    #[test]
    fn static_method_of_same_name_is_not_an_override() {
        assert!(overrides("class X extends Base { static void run() {} }").is_empty());
    }

    #[test]
    fn unresolved_param_type_is_skipped() {
        // The param type doesn't resolve → the signature can't be confirmed → not flagged.
        assert!(overrides("class X extends Base { void foo(Mystery m) {} }").is_empty());
    }
}
