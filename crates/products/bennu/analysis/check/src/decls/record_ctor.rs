//! Record constructors and accessors (pure-AST, JLS §8.10.3–§8.10.4).
//!
//! * the explicit **canonical** constructor — the one taking the component types in order — must
//!   name its parameters like the components, declare no `throws`, be at least as accessible as the
//!   record, and assign every component;
//! * the **compact** constructor has the same header rules and may not `return`;
//! * every **non-canonical** constructor must start with `this(…)`;
//! * an explicit **accessor** must be `public`, non-`static`, without `throws`, of the component's type.
//!
//! No flow analysis: ANY textual assignment to a component anywhere in the canonical body (even in one
//! branch) counts as assigned. Types are compared as written, so a canonical constructor spelled with
//! other names for the same types (`java.lang.String` for `String`) is not recognised — and silence
//! is what that costs.

use std::collections::HashSet;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// All incomplete-canonical-record-constructor errors over the shared pre-collected node list (one
/// traversal across all pure-AST checks).
pub fn record_ctor_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "record_declaration" {
            check_record(n, bytes, &mut out);
        }
    }
    out
}

/// A record header component, or a constructor parameter: its name and its type as written, with
/// whitespace removed (`Map<K, V>` and `Map<K,V>` are one spelling).
struct Param {
    name: String,
    ty: String,
}

/// Check every constructor and accessor `record` declares.
fn check_record(record: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(components) = params_of(record, bytes) else { return };
    let Some(body) = record.child_by_field_name("body") else { return };
    let record_access = access_of(record, bytes);
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        match member.kind() {
            "constructor_declaration" => check_constructor(member, &components, record_access, bytes, out),
            "compact_constructor_declaration" => check_compact(member, record_access, bytes, out),
            "method_declaration" => check_accessor(member, &components, bytes, out),
            _ => {}
        }
    }
}

/// A full constructor. Canonical means *the component types, in order* (JLS §8.10.4) — so a
/// constructor with those types and other parameter NAMES is a malformed canonical one, not an
/// overload. A constructor with another arity is non-canonical and must delegate to another.
///
/// Types are compared as written. Equal arity with different spellings (`String` against
/// `java.lang.String`) may or may not be the canonical signature, and there the answer is silence.
fn check_constructor(ctor: Node, components: &[Param], record_access: Option<u8>, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(params) = params_of(ctor, bytes) else { return };
    let anchor = ctor.child_by_field_name("name").unwrap_or(ctor);
    if params.len() != components.len() {
        if !delegates_via_this(ctor) {
            out.push(err(
                "A non-canonical record constructor must start with a call to another constructor, `this(…)`".to_string(),
                anchor,
            ));
            report_unassigned(ctor, components, &HashSet::new(), out);
        }
        return;
    }
    if params.iter().zip(components).any(|(p, c)| p.ty != c.ty) {
        return;
    }
    if let Some((p, c)) = params.iter().zip(components).find(|(p, c)| p.name != c.name) {
        out.push(err(
            format!("Canonical constructor parameter `{}` must be named like the record component `{}`", p.name, c.name),
            anchor,
        ));
        return;
    }
    check_canonical_header(ctor, anchor, record_access, bytes, out);
    if delegates_via_this(ctor) {
        return;
    }
    let Some(ctor_body) = ctor.child_by_field_name("body") else { return };
    let assigned = collect_assigned_names(ctor_body, bytes);
    report_unassigned(ctor, components, &assigned, out);
}

/// The compact canonical constructor: it assigns every field itself when its body ends, which is
/// exactly why its body may not `return` before that.
fn check_compact(ctor: Node, record_access: Option<u8>, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let anchor = ctor.child_by_field_name("name").unwrap_or(ctor);
    check_canonical_header(ctor, anchor, record_access, bytes, out);
    let Some(body) = ctor.child_by_field_name("body") else { return };
    let mut returns = Vec::new();
    collect_returns(body, &mut returns);
    if let Some(first) = returns.first() {
        out.push(err("A compact constructor cannot contain a `return` statement".to_string(), anchor));
        out.push(crate::engine::check_id::CheckId::DefiniteAssignment.at(
            *first,
            "Record fields are not initialized when the compact constructor returns early",
        ));
    }
}

/// What both canonical forms owe the record: no `throws` clause, and at least the record's access.
fn check_canonical_header(ctor: Node, anchor: Node, record_access: Option<u8>, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    if crate::support::nodes::child_of_kind(ctor, "throws").is_some() {
        out.push(err("A canonical record constructor cannot declare a `throws` clause".to_string(), anchor));
    }
    if let (Some(record), Some(own)) = (record_access, access_of(ctor, bytes)) {
        if own < record {
            out.push(err(
                "A canonical record constructor must be at least as accessible as its record".to_string(),
                anchor,
            ));
        }
    }
}

/// An explicit accessor — a method named like a component, with no parameters — must be `public`,
/// non-`static`, without `throws`, and return exactly the component's type. The type is only judged
/// where spelling cannot differ for the same type: when either side is a primitive.
fn check_accessor(method: Node, components: &[Param], bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(name_node) = method.child_by_field_name("name") else { return };
    let Some(name) = name_node.utf8_text(bytes).ok() else { return };
    let Some(component) = components.iter().find(|c| c.name == name) else { return };
    if !params_of(method, bytes).is_some_and(|p| p.is_empty()) {
        return;
    }
    let mods = crate::support::nodes::modifier_keywords(method, bytes);
    let mut problems = Vec::new();
    if !mods.contains(&"public") {
        problems.push("must be `public`".to_string());
    }
    if mods.contains(&"static") {
        problems.push("cannot be `static`".to_string());
    }
    if crate::support::nodes::child_of_kind(method, "throws").is_some() {
        problems.push("cannot declare `throws`".to_string());
    }
    if let Some(ret) = method.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()) {
        let ret = squeeze(ret);
        let primitive_involved =
            crate::support::nodes::is_primitive(&ret) || crate::support::nodes::is_primitive(&component.ty);
        if ret != component.ty && primitive_involved {
            problems.push(format!("must return `{}`", component.ty));
        }
    }
    if !problems.is_empty() {
        out.push(err(format!("Accessor `{name}()` of a record {}", problems.join(", ")), name_node));
    }
}

/// One `var.might.not.have.been.initialized` on the constructor's closing brace — where control
/// leaves it with the fields unset — naming every component `assigned` does not contain.
fn report_unassigned(ctor: Node, components: &[Param], assigned: &HashSet<String>, out: &mut Vec<Diagnostic>) {
    let missing: Vec<String> =
        components.iter().filter(|c| !assigned.contains(&c.name)).map(|c| format!("`{}`", c.name)).collect();
    if missing.is_empty() {
        return;
    }
    let Some(body) = ctor.child_by_field_name("body") else { return };
    let end = body.end_byte();
    let noun = if missing.len() == 1 { "component" } else { "components" };
    out.push(crate::engine::check_id::CheckId::DefiniteAssignment.span(
        end.saturating_sub(1),
        end,
        format!("Record {noun} {} not initialized by this constructor", missing.join(", ")),
    ));
}

/// The parameters of a record header or a constructor / method, in order; `None` when one of them
/// cannot be read.
fn params_of(decl: Node, bytes: &[u8]) -> Option<Vec<Param>> {
    let params = decl.child_by_field_name("parameters")?;
    let mut out = Vec::new();
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        match p.kind() {
            "formal_parameter" => {
                let name = p.child_by_field_name("name")?.utf8_text(bytes).ok()?.to_string();
                let ty = squeeze(p.child_by_field_name("type")?.utf8_text(bytes).ok()?);
                out.push(Param { name, ty });
            }
            "spread_parameter" => {
                let declarator = crate::support::nodes::child_of_kind(p, "variable_declarator")?;
                let name = declarator.child_by_field_name("name")?.utf8_text(bytes).ok()?.to_string();
                let mut pc = p.walk();
                let ty_node = p.named_children(&mut pc).find(|ch| !matches!(ch.kind(), "modifiers" | "variable_declarator"))?;
                let ty = format!("{}...", squeeze(ty_node.utf8_text(bytes).ok()?));
                out.push(Param { name, ty });
            }
            "line_comment" | "block_comment" => {}
            _ => return None,
        }
    }
    Some(out)
}

fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `public` 3, `protected` 2, package 1, `private` 0. `None` for a record nested in an interface,
/// which is implicitly public whatever is written.
fn access_of(decl: Node, bytes: &[u8]) -> Option<u8> {
    let in_interface = decl.parent().is_some_and(|p| p.kind() == "interface_body");
    if in_interface && decl.kind() == "record_declaration" {
        return None;
    }
    let mods = crate::support::nodes::modifier_keywords(decl, bytes);
    Some(if mods.contains(&"public") {
        3
    } else if mods.contains(&"protected") {
        2
    } else if mods.contains(&"private") {
        0
    } else {
        1
    })
}

/// The `return` statements of a body, not those of a lambda or a nested type inside it.
fn collect_returns<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    let mut c = node.walk();
    for ch in node.named_children(&mut c) {
        match ch.kind() {
            "return_statement" => out.push(ch),
            "lambda_expression" | "class_body" | "class_declaration" | "interface_declaration"
            | "enum_declaration" | "record_declaration" => {}
            _ => collect_returns(ch, out),
        }
    }
}

/// Whether the constructor's FIRST statement is a `this(...)` delegation (an
/// `explicit_constructor_invocation` on `this`). A `super(...)` chain is not delegation to another
/// ctor of the SAME record and doesn't initialize components, so only `this(...)` suppresses.
fn delegates_via_this(ctor: Node) -> bool {
    let Some(body) = ctor.child_by_field_name("body") else { return false };
    let mut c = body.walk();
    for stmt in body.named_children(&mut c) {
        if matches!(stmt.kind(), "line_comment" | "block_comment") {
            continue;
        }
        if stmt.kind() != "explicit_constructor_invocation" {
            return false; // first real statement isn't a chain call
        }
        // The invocation's constructor keyword is `this` for delegation, `super` for a super-call.
        // Records have no superclass to chain to, but a defensive check on the `constructor` field text
        // keeps this precise: only `this` is delegation.
        if let Some(kw) = stmt.child_by_field_name("constructor") {
            return kw.kind() == "this";
        }
        // No `constructor` field to read → be conservative and treat the leading chain call as
        // delegation (suppresses rather than risks a false positive).
        return true;
    }
    false
}

/// The set of identifier names assigned anywhere under `body` — bare `x = …`, `this.x = …`, `X.x = …`,
/// and `x++` / `this.x++` update targets. Mirrors [`crate::decls::init_checks::collect_assigned_names`]:
/// over-collecting only ever suppresses a report (safe); under-collecting would risk a false positive.
fn collect_assigned_names(body: Node, bytes: &[u8]) -> HashSet<String> {
    let mut names = HashSet::new();
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
            if let Some(name) = assigned_target_name(t, bytes) {
                names.insert(name);
            }
        }
        let mut cc = node.walk();
        for ch in node.named_children(&mut cc) {
            stack.push(ch);
        }
    }
    names
}

/// The variable/field name an assignment / update target refers to: `x` → `x`; `this.x` / `a.b.x` →
/// `x` (the trailing field). `None` for an array-index or other complex LHS (ignoring those only ever
/// avoids over-suppression, never a false positive).
fn assigned_target_name(target: Node, bytes: &[u8]) -> Option<String> {
    match target.kind() {
        "identifier" => target.utf8_text(bytes).ok().map(str::to_string),
        "field_access" => target
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(bytes).ok())
            .map(str::to_string),
        _ => None,
    }
}

/// The operand node of an `update_expression` (`x++`, `--x`, `this.x++`) — its single named child.
fn update_operand(update: Node) -> Option<Node> {
    let mut c = update.walk();
    for ch in update.named_children(&mut c) {
        return Some(ch);
    }
    None
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic {
        message,
        severity: crate::engine::check_id::CheckId::RecordConstructor.severity().to_string(),
        code: crate::engine::check_id::CheckId::RecordConstructor.code().to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
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
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        record_ctor_errors_nodes(&nodes, src).into_iter().map(|d| d.message).collect()
    }

    // ── positive ─────────────────────────────────────────────────────────────

    #[test]
    fn canonical_ctor_missing_a_component_is_flagged() {
        let src = "record UnitRecord(String name, int level) { \
                   public UnitRecord(String name, int level) { this.name = name; } }";
        let d = errs(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Record component `level` not initialized"), "{d:?}");
    }

    #[test]
    fn multiple_missing_components_share_one_report() {
        let src = "record R(int a, int b, int c) { \
                   R(int a, int b, int c) { this.a = a; } }";
        let d = errs(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`b`, `c`"), "{d:?}");
        assert!(!d[0].contains("`a`"), "{d:?}");
    }

    #[test]
    fn canonical_header_rules() {
        assert!(errs("public record R(int x) { R(int x) { this.x = x; } }").iter().any(|m| m.contains("accessible")));
        assert!(errs("record R(int x) { R(int x) throws Exception { this.x = x; } }").iter().any(|m| m.contains("throws")));
        assert!(errs("record R(int x) { R { return; } }").iter().any(|m| m.contains("`return`")));
        // A record nested in an interface is implicitly public; its access is not ours to judge.
        assert!(errs("interface I { record R(int x) { R(int x) { this.x = x; } } }").is_empty());
    }

    #[test]
    fn accessor_rules() {
        assert!(errs("record R(int x) { public long x() { return x; } }").iter().any(|m| m.contains("must return `int`")));
        assert!(errs("record R(int x) { int x() { return x; } }").iter().any(|m| m.contains("must be `public`")));
        assert!(errs("record R(int x) { public int x() { return x; } int x(int scale) { return x * scale; } }").is_empty());
        // `String` against `java.lang.String` is the same type spelled twice: not judged.
        assert!(errs("record R(String s) { public java.lang.String s() { return s; } }").is_empty());
    }

    #[test]
    fn bare_name_assignment_counts_as_initialized() {
        // Only `level` is missing; `name = name;` (bare, no `this.`) still counts as assigned.
        let src = "record R(String name, int level) { \
                   R(String name, int level) { name = name; } }";
        let d = errs(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`level`"), "{d:?}");
    }

    // ── negatives ────────────────────────────────────────────────────────────

    #[test]
    fn canonical_ctor_assigning_all_is_ok() {
        let src = "record UnitRecord(String name, int level) { \
                   public UnitRecord(String name, int level) { this.name = name; this.level = level; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn component_assigned_in_one_branch_is_not_flagged() {
        // ANY textual assignment (even one branch) suppresses — no flow analysis.
        let src = "record R(int x) { R(int x) { if (x > 0) { this.x = x; } } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn compact_canonical_ctor_is_never_flagged() {
        // A compact ctor auto-assigns every component at the end → can't be incomplete.
        let src = "record R(int x) { R { if (x < 0) throw new IllegalArgumentException(); } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn record_with_no_explicit_ctor_is_not_flagged() {
        // Compiler generates a complete canonical ctor → nothing to check.
        assert!(errs("record R(int x, int y) {}").is_empty());
    }

    #[test]
    fn non_canonical_ctor_delegating_via_this_is_not_flagged() {
        let src = "record R(int x, int y) { R(int x) { this(x, 0); } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn non_canonical_ctor_must_delegate() {
        let d = errs("record R(int x, int y) { R(int x) { } }");
        assert!(d.iter().any(|m| m.contains("`this(…)`")), "{d:?}");
    }

    #[test]
    fn same_types_different_names_is_a_malformed_canonical_ctor() {
        let d = errs("record R(int x, int y) { R(int a, int b) { this.x = a; this.y = b; } }");
        assert!(d.iter().any(|m| m.contains("must be named like")), "{d:?}");
    }

    #[test]
    fn same_arity_other_spelling_is_left_alone() {
        let src = "record R(String s) { R(java.lang.String s) { this.s = s; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }

    #[test]
    fn non_record_is_ignored() {
        // A plain class ctor that doesn't assign a field is another check's concern, not ours.
        assert!(errs("class C { int x; C(int x) {} }").is_empty());
    }

    #[test]
    fn empty_component_record_is_ignored() {
        assert!(errs("record R() { R() {} }").is_empty());
    }
}
