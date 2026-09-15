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
use tree_sitter::Node;
#[cfg(test)]
use tree_sitter::Parser;

use crate::support::method_sig::{member_param_binaries, method_param_binaries};
use crate::support::nodes::{has_keyword, text};
use crate::support::walk::for_each_supertype;

/// Parse `source` and flag illegal reassignment of `final` locals and fields.
pub fn final_reassignment_errors(source: &str) -> Vec<Diagnostic> {
    match bennu_java::prelude::parse_java(source) {
        Some(tree) => final_reassignment_errors_in(tree.root_node(), source),
        None => Vec::new(),
    }
}

/// Tree-driven core (shared with the `check_file` single-parse path).
pub fn final_reassignment_errors_in(root: Node, source: &str) -> Vec<Diagnostic> {
    final_reassignment_errors_nodes(&crate::engine::check::collect_nodes(root), source)
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
                    check_final_params(n, body, bytes, &mut out);
                }
            }
            "catch_clause" => check_catch_parameter(n, bytes, &mut out),
            "enhanced_for_statement" => {
                if has_final(n, bytes) {
                    if let (Some(name), Some(body)) =
                        (n.child_by_field_name("name").and_then(|x| text(x, bytes)), n.child_by_field_name("body"))
                    {
                        for target in assignments_to(body, &name, bytes) {
                            out.push(err(format!("Cannot assign a value to final variable `{name}`"), target));
                        }
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
            "class_declaration" | "enum_declaration" | "record_declaration" => {
                check_final_fields(n, bytes, &mut out);
            }
            _ => {}
        }
    }
    out
}

// ── final parameters ─────────────────────────────────────────────────────────

/// A `final` parameter of a method, constructor or lambda, assigned in its body.
fn check_final_params(decl: Node, body: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(params) = decl.child_by_field_name("parameters") else { return };
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        if p.kind() != "formal_parameter" || !has_final(p, bytes) {
            continue;
        }
        let Some(name) = p.child_by_field_name("name").and_then(|x| text(x, bytes)) else { continue };
        for target in assignments_to(body, &name, bytes) {
            out.push(err(format!("Cannot assign a value to final parameter `{name}`"), target));
        }
    }
}

/// A `catch` parameter that is final — written `final`, or implicitly because the clause catches
/// more than one type (JLS §14.20) — assigned in the catch block.
fn check_catch_parameter(clause: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(param) = crate::support::nodes::child_of_kind(clause, "catch_formal_parameter") else { return };
    let Some(body) = clause.child_by_field_name("body") else { return };
    let Some(name) = param.child_by_field_name("name").and_then(|x| text(x, bytes)) else { return };
    let multi = crate::support::nodes::child_of_kind(param, "catch_type").is_some_and(|t| t.named_child_count() > 1);
    let message = if multi {
        format!("Multi-catch parameter `{name}` is implicitly final and cannot be assigned")
    } else if has_final(param, bytes) {
        format!("Cannot assign a value to final parameter `{name}`")
    } else {
        return;
    };
    for target in assignments_to(body, &name, bytes) {
        out.push(err(message.clone(), target));
    }
}

/// The bare-name assignment / update targets naming `name` inside `scope`, not crossing a nested
/// type, lambda or anonymous class. Empty when anything inside `scope` declares `name` again — the
/// assignment might then be to that one.
fn assignments_to<'t>(scope: Node<'t>, name: &str, bytes: &[u8]) -> Vec<Node<'t>> {
    if declared_names(scope, bytes).iter().any(|d| d == name) {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        let target = match n.kind() {
            "assignment_expression" => assign_target_name(n, bytes),
            "update_expression" => update_target_name(n, bytes),
            _ => None,
        };
        if let Some((target_name, node)) = target {
            if target_name == name {
                found.push(node);
            }
        }
        if is_scope_boundary(n.kind()) || n.kind() == "class_body" {
            continue;
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    found
}

/// Every name something inside `scope` declares — locals, parameters of nested lambdas and
/// methods, catch and for-each variables, pattern bindings. Over-inclusive on purpose: it only ever
/// makes a caller stay silent.
fn declared_names(scope: Node, bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "variable_declarator" | "formal_parameter" | "catch_formal_parameter" | "enhanced_for_statement"
            | "resource" => names.extend(n.child_by_field_name("name").and_then(|x| text(x, bytes))),
            "type_pattern" | "record_pattern_component" | "inferred_parameters" => {
                let mut c = n.walk();
                names.extend(n.named_children(&mut c).filter(|x| x.kind() == "identifier").filter_map(|x| text(x, bytes)));
            }
            "lambda_expression" => {
                if let Some(p) = n.child_by_field_name("parameters").filter(|p| p.kind() == "identifier") {
                    names.extend(text(p, bytes));
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        stack.extend(n.named_children(&mut c));
    }
    names
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

/// A `final` field of the type being checked, as far as assigning it is concerned.
struct FinalField {
    is_static: bool,
    initialized: bool,
    /// A record component's implicit `private final` field.
    component: bool,
}

/// The member of a type body an assignment sits in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Site {
    Method,
    Constructor,
    CompactConstructor,
    InstanceInitializer,
    StaticInitializer,
}

/// The members of a type body — an enum's sit one wrapper deeper, after its constants.
fn type_members(body: Node) -> Vec<Node> {
    let mut members = Vec::new();
    let mut c = body.walk();
    for m in body.named_children(&mut c) {
        if m.kind() == "enum_body_declarations" {
            let mut dc = m.walk();
            members.extend(m.named_children(&mut dc));
        } else {
            members.push(m);
        }
    }
    members
}

/// The `final` fields `n` declares itself, by name — including a record's components.
fn final_fields_of(n: Node, body: Node, bytes: &[u8]) -> HashMap<String, FinalField> {
    let mut fields = HashMap::new();
    if n.kind() == "record_declaration" {
        if let Some(params) = n.child_by_field_name("parameters") {
            let mut c = params.walk();
            for p in params.named_children(&mut c).filter(|p| p.kind() == "formal_parameter") {
                if let Some(name) = p.child_by_field_name("name").and_then(|x| text(x, bytes)) {
                    fields.insert(name, FinalField { is_static: false, initialized: false, component: true });
                }
            }
        }
    }
    for m in type_members(body) {
        if m.kind() != "field_declaration" || !has_final(m, bytes) {
            continue;
        }
        let is_static = has_static(m, bytes);
        let mut dc = m.walk();
        for d in m.named_children(&mut dc).filter(|d| d.kind() == "variable_declarator") {
            if let Some(name) = decl_name(d, bytes) {
                let initialized = d.child_by_field_name("value").is_some();
                fields.insert(name, FinalField { is_static, initialized, component: false });
            }
        }
    }
    fields
}

/// Whether assigning `field` at `site` is illegal whatever the flow (JLS §16): an initialized final
/// anywhere, a blank one in a method, a static blank one in an instance constructor or initializer,
/// a record component anywhere but its canonical constructor (a compact constructor assigns the
/// parameter when it writes the bare name, the field only through `this.`).
///
/// A blank final assigned in its own constructor is a definite-assignment question, left alone here.
fn assignment_is_illegal(field: &FinalField, site: Site, via_this: bool) -> bool {
    if field.initialized {
        return true;
    }
    if field.component {
        return match site {
            Site::Method => true,
            Site::CompactConstructor => via_this,
            _ => false,
        };
    }
    match site {
        Site::Method => true,
        Site::Constructor | Site::InstanceInitializer => field.is_static,
        Site::CompactConstructor | Site::StaticInitializer => false,
    }
}

/// Flag assignments to the `final` fields the type `n` declares, in its own members — through
/// `this.field` or a bare name nothing in the member shadows. Nested and anonymous types are not
/// entered: their `this` and their names are their own.
fn check_final_fields(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = n.child_by_field_name("body") else { return };
    let fields = final_fields_of(n, body, bytes);
    if fields.is_empty() {
        return;
    }
    for member in type_members(body) {
        let site = match member.kind() {
            "method_declaration" => Site::Method,
            "constructor_declaration" => Site::Constructor,
            "compact_constructor_declaration" => Site::CompactConstructor,
            "block" => Site::InstanceInitializer,
            "static_initializer" => Site::StaticInitializer,
            _ => continue,
        };
        let shadowed = declared_names(member, bytes);
        let mut stack = vec![member];
        while let Some(node) = stack.pop() {
            let target = match node.kind() {
                "assignment_expression" => node.child_by_field_name("left"),
                "update_expression" => update_operand(node),
                _ => None,
            };
            if let Some(t) = target {
                let named = match this_field_name(t, bytes) {
                    Some(field) => Some((field, true)),
                    None if t.kind() == "identifier" => text(t, bytes).filter(|x| !shadowed.contains(x)).map(|x| (x, false)),
                    None => None,
                };
                if let Some((name, via_this)) = named {
                    if fields.get(&name).is_some_and(|f| assignment_is_illegal(f, site, via_this)) {
                        out.push(err(format!("Cannot assign a value to final field `{name}`"), t));
                    }
                }
            }
            let nested_type = matches!(
                node.kind(),
                "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration"
                    | "annotation_type_declaration" | "class_body"
            );
            // A blank final assigned inside a lambda of its constructor is still illegal, but that is
            // javac's flow analysis talking; the constructor rules above do not know about the lambda.
            let lambda_in_ctor = node.kind() == "lambda_expression" && site != Site::Method;
            if nested_type || lambda_in_ctor {
                continue;
            }
            let mut cc = node.walk();
            stack.extend(node.named_children(&mut cc));
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
    supers.extend(crate::support::supertypes::superclass(n, bytes).and_then(|sup| {
        crate::support::supertypes::binary(&sup.text, n, bytes, symbols, resolver)
    }));

    // name → the erased parameter-type lists of the inherited methods an override would collide
    // with: `final` ones, `static` ones (an instance method cannot override those) and instance ones
    // (a static method cannot hide those). Package-private ones are left out — whether they are
    // inherited at all depends on the package, which this does not decide.
    let mut final_methods: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    let mut static_methods: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    let mut instance_methods: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    for sup in &supers {
        for_each_supertype(resolver, sup, &mut |_bn, cm| {
            for m in &cm.methods {
                let inherited = m.kind == MemberKind::Method
                    && !matches!(m.visibility, Visibility::Private | Visibility::Package)
                    && m.name != "<init>"
                    && m.name != "<clinit>";
                if !inherited {
                    continue;
                }
                let params = member_param_binaries(m);
                let into = match (m.is_static, m.is_final) {
                    (true, _) => &mut static_methods,
                    (false, true) => &mut final_methods,
                    (false, false) => &mut instance_methods,
                };
                into.entry(m.name.clone()).or_default().push(params);
            }
        });
    }
    if final_methods.is_empty() && static_methods.is_empty() && instance_methods.is_empty() {
        return;
    }

    // Each method declared directly in this type: does it collide with a collected one?
    let mut bc = body.walk();
    for m in body.named_children(&mut bc) {
        if m.kind() != "method_declaration" || has_visibility(m, bytes, "private") {
            continue; // a private method overrides and hides nothing
        }
        let Some(name_node) = m.child_by_field_name("name") else { continue };
        let Some(name) = text(name_node, bytes) else { continue };
        let collides = |map: &HashMap<String, Vec<Vec<String>>>, params: &[String]| {
            map.get(&name).is_some_and(|cs| cs.iter().any(|c| c == params))
        };
        let Some(params) = method_param_binaries(m, bytes, symbols, resolver) else { continue };
        let message = if has_static(m, bytes) {
            // `static` beside an inherited instance method is not hiding, it is an error (JLS §8.4.8.2).
            (collides(&instance_methods, &params) || collides(&final_methods, &params))
                .then(|| format!("Static method `{name}` cannot hide the instance method it would override"))
        } else if collides(&final_methods, &params) {
            Some(format!("Cannot override final method `{name}`"))
        } else {
            collides(&static_methods, &params)
                .then(|| format!("`{name}` is static in the supertype, so an instance method cannot override it"))
        };
        if let Some(message) = message {
            out.push(crate::engine::check_id::CheckId::FinalMethodOverride.at(name_node, message));
        }
    }
}

/// The erased binary names of a method's parameter types. `None` (skip the method) if any parameter
/// type can't be resolved, or the method is varargs (a conservative miss rather than a wrong match).
fn has_static(node: Node, bytes: &[u8]) -> bool {
    has_keyword(node, bytes, "static")
}

fn has_visibility(node: Node, bytes: &[u8], keyword: &str) -> bool {
    has_keyword(node, bytes, keyword)
}

// ── shared node helpers ──────────────────────────────────────────────────────

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
        severity: crate::engine::check_id::CheckId::FinalAssignment.severity().to_string(),
        code: crate::engine::check_id::CheckId::FinalAssignment.code().to_string(),
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
    fn final_parameters_and_catch_parameters_are_flagged() {
        assert!(errs("class C { void m(final int v) { v = 2; } }").iter().any(|m| m.contains("final parameter `v`")));
        assert!(errs("class C { void m() { try { } catch (final RuntimeException e) { e = null; } } }").len() == 1);
        let multi = errs("class C { void m() { try { } catch (IllegalStateException | IllegalArgumentException e) { e = null; } } }");
        assert!(multi.iter().any(|m| m.contains("Multi-catch")), "{multi:?}");
        // Neither final nor multi: assignable.
        assert!(errs("class C { void m(int v) { v = 2; try { } catch (RuntimeException e) { e = null; } } }").is_empty());
    }

    #[test]
    fn final_for_each_variable_is_flagged() {
        assert_eq!(errs("class C { void m(int[] a) { for (final int v : a) { v = 0; } } }").len(), 1);
        assert!(errs("class C { void m(int[] a) { for (int v : a) { v = 0; } } }").is_empty());
    }

    #[test]
    fn bare_final_field_assignments_are_flagged() {
        assert_eq!(errs("class C { final int x = 1; void m() { x = 2; } }").len(), 1);
        assert_eq!(errs("class C { static final int K = 1; void m() { K = 2; } }").len(), 1);
        // A blank final assigned in a method is illegal; in its constructor it is not.
        assert_eq!(errs("class C { final int x; C() { x = 1; } void m() { x = 2; } }").len(), 1);
        // A static blank final belongs to the static initializer.
        assert!(errs("class C { static final int K; static { K = 1; } }").is_empty());
    }

    #[test]
    fn a_shadowed_field_name_is_not_flagged() {
        assert!(errs("class C { final int x = 1; void m(int x) { x = 2; } }").is_empty());
        assert!(errs("class C { final int x = 1; void m() { Runnable r = new Runnable() { int x; public void run() { x = 2; } }; } }").is_empty());
    }

    #[test]
    fn record_component_fields_are_final() {
        assert_eq!(errs("record R(int x) { void reset() { x = 0; } }").len(), 1);
        assert_eq!(errs("record R(int x) { R { this.x = 1; } }").len(), 1);
        // The compact constructor's bare name is its parameter; the canonical one assigns the field.
        assert!(errs("record R(int x) { R { x = Math.abs(x); } }").is_empty());
        assert!(errs("record R(int x) { R(int x) { this.x = x; } }").is_empty());
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
            type_params: Vec::new(),
            superclass: Some(TypeRef::simple("java/lang/Object")),
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
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
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

    /// A `static` method with an inherited instance method's signature does not hide it: javac
    /// rejects it (`override.static`), whatever the instance method's modifiers.
    #[test]
    fn a_static_method_over_an_inherited_instance_method_is_flagged() {
        let d = overrides("class X extends Base { static void run() {} }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Static method `run`"), "{d:?}");
        assert!(overrides("class X extends Base { static void run(int x) {} }").is_empty());
    }

    #[test]
    fn unresolved_param_type_is_skipped() {
        // The param type doesn't resolve → the signature can't be confirmed → not flagged.
        assert!(overrides("class X extends Base { void foo(Mystery m) {} }").is_empty());
    }
}
