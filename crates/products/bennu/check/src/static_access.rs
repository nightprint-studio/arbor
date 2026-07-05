//! Static-context diagnostics — a **bare (unqualified) reference to a non-static member** made from
//! inside a `static` method or a `static` initializer: javac's "non-static X cannot be referenced
//! from a static context". A `static` method has no `this`, so a bare `instance_counter` /
//! `instance_helper()` (which implicitly means `this.instance_counter` / `this.instance_helper()`)
//! has no receiver to bind against.
//!
//! Like [`crate::undefined_var`], this is highly false-positive-prone (a bare name can bind through
//! locals, params, fields, inherited fields, static imports, type qualifiers, …), so the PARAMOUNT
//! rule holds: **never a false positive**. Every doubt SKIPs. We flag only the narrow, certain case:
//! a bare identifier/call whose ONLY matching member in a FULLY-KNOWN hierarchy is a definitely
//! non-static field (for a bare value) or method (for a bare call) of the enclosing top-level type.
//!
//! WHOLE-FILE guards (any → produce NOTHING for the file), same as `undefined_var`:
//!   * ANY `import static …` — could supply an arbitrary bare name we don't model;
//!   * a parse error anywhere (`has_error`) — a broken CST could mis-shape a reference;
//!   * more than one top-level class/enum, or a top-level interface/record/annotation alongside —
//!     ambiguous ownership of an identifier's enclosing type;
//!   * the top-level type's hierarchy not fully known — a member could live in an un-indexed base and
//!     its static-ness would be unknown.
//!
//! PER-REFERENCE guards (any failing → SKIP that reference):
//!   * BARE only: a `method_invocation` with NO `object` (`foo()`), or an `identifier` used as a
//!     value with no receiver. A qualified `x.foo()`, `this.x`, `Type.STATIC` → SKIP;
//!   * the reference's enclosing type must be exactly the file's top-level class/enum, with NO
//!     intervening lambda / anonymous / nested / local class (its `this` context differs) → SKIP;
//!   * the enclosing executable context must be a `static` method or a `static_initializer` (the
//!     nearest method walking up must be `static`; an instance method / instance initializer /
//!     constructor → SKIP);
//!   * the name must NOT be a local / parameter / for-var / catch-param / resource / pattern var in
//!     scope (a local shadows the member) → SKIP;
//!   * the name must resolve to an INSTANCE member of the enclosing type or a fully-known supertype,
//!     and NO static member of that name may exist (ambiguity) → only then flag.

use std::collections::HashSet;

use bennu_java::prelude::{FileSymbols, MemberKind, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::resolve::type_binary;
use crate::walk::{for_each_supertype, hierarchy_fully_known};

/// The bare `java.lang` type names available without an import — a standalone bare identifier
/// matching one of these is a type reference (heading `Integer.parseInt(...)` etc.), never an
/// instance-member reference. Mirrors [`crate::undefined_var`]'s exclusion list; its only effect is
/// to SUPPRESS, so over-inclusion is harmless.
const JAVA_LANG_TYPES: &[&str] = &[
    "String", "Object", "Integer", "Long", "Boolean", "Double", "Float", "Character", "Byte",
    "Short", "Number", "Math", "System", "Thread", "Class", "Void", "StringBuilder", "StringBuffer",
    "Exception", "Throwable", "Error", "RuntimeException", "Enum", "Runnable", "Comparable",
    "Iterable", "CharSequence",
];

/// Parse `source` and flag bare non-static-member references made from a static context.
pub fn static_access_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::check::collect_nodes(root);
    static_access_errors_in(root, &nodes, source, &symbols, resolver)
}

/// The tree-driven core (mirrors [`crate::members::unknown_members_in`] /
/// [`crate::undefined_var::undefined_var_errors_in`]): iterates the shared pre-collected `nodes`
/// (one DFS) and reuses `root` + `symbols` + `resolver`.
pub fn static_access_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();

    // ── WHOLE-FILE guard 1: any static import poisons the file. ──────────────────────────────────
    // A static import can supply an arbitrary bare name (`foo`) that is NOT an instance member; we
    // don't model those, so we cannot soundly assert "this bare name is a non-static member".
    if symbols.imports.iter().any(|i| i.static_) {
        return Vec::new();
    }

    // ── WHOLE-FILE guard 2: a parse error anywhere → the CST is untrustworthy. ────────────────────
    if root.has_error() {
        return Vec::new();
    }

    // ── Locate the file's single TOP-LEVEL class or enum. ────────────────────────────────────────
    let Some(top) = single_top_level_type(root, bytes) else {
        return Vec::new();
    };

    // Resolve it to a binary name and require its ENTIRE hierarchy be known — else a member of this
    // name could live in an un-indexed base and we couldn't know whether it's static.
    let Some(top_binary) = type_binary(&top.decl_name, symbols, resolver) else {
        return Vec::new();
    };
    if !hierarchy_fully_known(resolver, &top_binary) {
        return Vec::new();
    }

    // Gather, across the top type + every (fully-known) supertype, the set of member names that have
    // an INSTANCE (non-static) declaration and, separately, the set that have a STATIC declaration —
    // split by kind (field vs method). A name that appears in the static set is AMBIGUOUS (a static
    // member of that name exists → SKIP), so we keep both to enforce "instance-only".
    let mut sig = MemberSignatures::default();
    for_each_supertype(resolver, &top_binary, &mut |_bn, cm| {
        for m in &cm.fields {
            if m.kind == MemberKind::Field {
                if m.is_static {
                    sig.static_fields.insert(m.name.clone());
                } else {
                    sig.instance_fields.insert(m.name.clone());
                }
            }
        }
        for m in &cm.methods {
            if m.kind == MemberKind::Method {
                if m.is_static {
                    sig.static_methods.insert(m.name.clone());
                } else {
                    sig.instance_methods.insert(m.name.clone());
                }
            }
        }
    });

    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            // A bare call `foo()` — no `object` field. Resolve against METHOD signatures.
            "method_invocation" => {
                if n.child_by_field_name("object").is_some() {
                    continue; // qualified `x.foo()` → SKIP
                }
                let Some(name_node) = n.child_by_field_name("name") else { continue };
                check_reference(name_node, n, top.node, bytes, &sig, MemberDomain::Method, &mut out);
            }
            // A bare value identifier. Resolve against FIELD signatures.
            "identifier" => {
                if !is_judgeable_value_ident(n, top.node) {
                    continue;
                }
                check_reference(n, n, top.node, bytes, &sig, MemberDomain::Field, &mut out);
            }
            _ => {}
        }
    }
    out
}

/// Which member namespace a reference resolves against.
#[derive(Clone, Copy)]
enum MemberDomain {
    /// A bare call `foo()` → resolves against methods.
    Method,
    /// A bare value identifier `foo` → resolves against fields.
    Field,
}

/// Instance / static member names split by kind, gathered once for the file across the whole known
/// hierarchy. A name in a `static_*` set makes any reference to it AMBIGUOUS → not flagged.
#[derive(Default)]
struct MemberSignatures {
    instance_fields: HashSet<String>,
    static_fields: HashSet<String>,
    instance_methods: HashSet<String>,
    static_methods: HashSet<String>,
}

/// Judge one bare reference (`name_node` is the identifier carrying the name; `ref_node` is the whole
/// reference expression — the `method_invocation` for a call, or the identifier itself for a value)
/// and push a diagnostic when it is a definitely non-static member referenced from a static context.
///
/// SKIP (return without pushing) on ANY of: name a keyword / `java.lang` type; name a local in scope;
/// reference NOT in a static context; the member ambiguous or not an instance member of the type.
fn check_reference(
    name_node: Node,
    ref_node: Node,
    top: Node,
    bytes: &[u8],
    sig: &MemberSignatures,
    domain: MemberDomain,
    out: &mut Vec<Diagnostic>,
) {
    if name_node.has_error() {
        return;
    }
    let Ok(name) = name_node.utf8_text(bytes) else { return };

    // Keyword-ish tokens never denote an instance member here (defensive — `this`/`super` etc. parse
    // as their own node kinds, not `identifier`).
    if matches!(name, "this" | "super" | "true" | "false" | "null" | "var") {
        return;
    }
    // A bare `java.lang` type name is a type reference (`Math.max(...)`), not an instance member.
    if JAVA_LANG_TYPES.contains(&name) {
        return;
    }

    // A local / parameter / for-var / catch-param / resource / pattern var shadows the member — the
    // bare name binds to the LOCAL, not to the instance member → not this error.
    if resolves_as_local(ref_node, top, bytes) {
        return;
    }

    // The reference must sit in a STATIC context, with no intervening lambda / anonymous / nested /
    // local class between it and the enclosing static callable (whose `this` context differs).
    if !in_static_context_of_top(ref_node, top) {
        return;
    }

    // Resolve against the relevant namespace. Flag ONLY when the name is a KNOWN instance member and
    // NO static member of the same name exists (ambiguity → SKIP).
    let (is_instance, is_static) = match domain {
        MemberDomain::Method => {
            (sig.instance_methods.contains(name), sig.static_methods.contains(name))
        }
        MemberDomain::Field => {
            (sig.instance_fields.contains(name), sig.static_fields.contains(name))
        }
    };
    if !is_instance || is_static {
        return; // not an instance member, or a static member of the name also exists → SKIP
    }

    out.push(Diagnostic {
        message: format!("Non-static member `{name}` cannot be referenced from a static context"),
        severity: "error".to_string(),
        start: name_node.start_byte(),
        end: name_node.end_byte(),
    });
}

/// A located top-level type: its CST node plus its declared simple name. (Same shape as
/// [`crate::undefined_var`]'s.)
struct TopType<'t> {
    node: Node<'t>,
    decl_name: String,
}

/// The file's single top-level `class`/`enum`, or `None` when there are zero, several, or a top-level
/// interface/record/annotation is present (ambiguous ownership → SKIP the file). Mirrors
/// [`crate::undefined_var`]'s guard exactly.
fn single_top_level_type<'t>(root: Node<'t>, bytes: &[u8]) -> Option<TopType<'t>> {
    let mut found: Option<TopType> = None;
    let mut c = root.walk();
    for ch in root.named_children(&mut c) {
        if matches!(ch.kind(), "class_declaration" | "enum_declaration") {
            if found.is_some() {
                return None;
            }
            let name = ch.child_by_field_name("name")?;
            let decl_name = name.utf8_text(bytes).ok()?.to_string();
            found = Some(TopType { node: ch, decl_name });
        } else if matches!(
            ch.kind(),
            "interface_declaration" | "record_declaration" | "annotation_type_declaration"
        ) {
            return None;
        }
    }
    found
}

/// Whether `ident` is a bare *value* reference we're entitled to judge (a primary-expression
/// identifier, not a declaration / suffix / method-name / type / label). The SCOPE part (enclosing
/// type + static context) is enforced separately by [`in_static_context_of_top`]; here we only reject
/// non-value POSITIONS — mirroring [`crate::undefined_var::is_judgeable_value_ident`]'s position half.
fn is_judgeable_value_ident(ident: Node, _top: Node) -> bool {
    let Some(parent) = ident.parent() else { return false };
    let pkind = parent.kind();
    let field_of_parent = child_field_name(parent, ident);

    if pkind == "variable_declarator" {
        // Skip the binding `name` slot; judge the `value` (initializer RHS) slot.
        if field_of_parent.as_deref() == Some("name") {
            return false;
        }
    } else {
        match pkind {
            // A `foo.bar` member access: the `object` head is a qualifier (variable/type/package
            // ambiguity) and the `field` suffix is a member — neither is a BARE value. SKIP both.
            "field_access" => return false,
            "scoped_identifier" | "scoped_type_identifier" | "scoped_type_arguments" => return false,
            // Any identifier directly under a call is the method `name` or a qualifier head — a bare
            // CALL is handled via the `method_invocation` branch, not here. SKIP.
            "method_invocation" => return false,
            // Declaration NAME slots — introduce a binding, not a reference.
            "formal_parameter"
            | "spread_parameter"
            | "catch_formal_parameter"
            | "type_parameter"
            | "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "enum_constant"
            | "labeled_statement" => return false,
            // Type positions.
            "type_identifier"
            | "generic_type"
            | "array_type"
            | "cast_expression"
            | "object_creation_expression"
            | "type_arguments"
            | "annotation"
            | "marker_annotation"
            | "annotation_argument_list"
            | "element_value_pair" => return false,
            // Import / package qualifiers.
            "import_declaration" | "package_declaration" => return false,
            // A `case FOO:` constant label — resolves against the selector's enum, not local scope.
            "switch_label" | "constant" => return false,
            _ => {}
        }
    }
    true
}

/// The field name that immediate child `child` occupies in `parent`, or `None`. (Copied from
/// [`crate::undefined_var`] — reads field names via a cursor, never `.find(...)`.)
fn child_field_name(parent: Node, child: Node) -> Option<String> {
    let mut c = parent.walk();
    if c.goto_first_child() {
        loop {
            if c.node().id() == child.id() {
                return c.field_name().map(str::to_string);
            }
            if !c.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

/// Whether `ref_node` sits in a STATIC context whose `this`-less scope is exactly the top type's:
/// walking upward from the reference to `top`, the NEAREST enclosing executable member must be a
/// `static` method or a `static_initializer`, crossing NO lambda and no nested/anonymous/local type
/// body (any of which changes the `this` context / owning type) → otherwise SKIP.
///
/// Returns `false` (SKIP) when:
///   * a lambda / nested / anonymous / local class body intervenes before reaching `top`;
///   * the nearest enclosing method is an INSTANCE method (not `static`);
///   * the enclosing initializer is an INSTANCE initializer (a bare `block` directly in the class
///     body, not a `static_initializer`);
///   * the reference is inside a `constructor_declaration` (has `this`);
///   * `top` is reached without having passed through any static method / static initializer (e.g. a
///     field initializer expression — an instance-init context — or an unrecognised shape).
fn in_static_context_of_top(ref_node: Node, top: Node) -> bool {
    // `top`'s own body node id — the one class/enum body we're allowed to cross on the way up.
    let top_body_id = top.child_by_field_name("body").map(|b| b.id());

    let mut cur = ref_node.parent();
    while let Some(p) = cur {
        // Reached the top type itself without hitting a static method / static initializer → the
        // reference is in an instance-init / field-init / unknown context → SKIP.
        if p.id() == top.id() {
            return false;
        }
        match p.kind() {
            // A lambda has its own `this`-less-but-capturing scope we don't model → SKIP.
            "lambda_expression" => return false,
            // A nested/local type declaration between us and `top` → different owning type → SKIP.
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => return false,
            // Any class/enum body other than `top`'s own is a nested/anonymous type body → SKIP.
            "class_body" | "enum_body" | "enum_body_declarations" => {
                if Some(p.id()) != top_body_id {
                    return false;
                }
            }
            // The nearest enclosing METHOD decides: static → this is a static context; instance →
            // SKIP. Either way, the method is the boundary of the executable scope, so we stop here.
            "method_declaration" => {
                return is_static_method(p);
            }
            // A constructor always has `this` → never a static context → SKIP.
            "constructor_declaration" => return false,
            // A `static { … }` initializer block IS a static context.
            "static_initializer" => return true,
            _ => {}
        }
        cur = p.parent();
    }
    false
}

/// Whether a `method_declaration` node carries the `static` keyword modifier. Reads the anonymous
/// `static` token out of the `modifiers` child via a cursor (never `.any(...)` on the iterator).
fn is_static_method(method: Node) -> bool {
    let mut c = method.walk();
    for ch in method.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() && m.kind() == "static" {
                    return true;
                }
            }
            return false;
        }
    }
    false
}

/// Whether `name` (the reference's text) is declared as a local / parameter / for-var / catch-param /
/// resource / pattern var in ANY scope enclosing `ref_node`, up to `top`. Collected textually from
/// every ancestor executable scope. Over-collection (a name declared in a sibling block of an
/// ancestor) is SAFE here — it can only SUPPRESS a diagnostic, never create one. Mirrors
/// [`crate::undefined_var::resolves_as_local`].
fn resolves_as_local(ref_node: Node, top: Node, bytes: &[u8]) -> bool {
    let Ok(name) = ref_node_name(ref_node, bytes) else { return true }; // unreadable → SKIP as resolved

    let top_body_id = top.child_by_field_name("body").map(|b| b.id());

    let mut cur = ref_node.parent();
    while let Some(p) = cur {
        if p.id() == top.id() || Some(p.id()) == top_body_id {
            break; // reached the type / its body — beyond the enclosing member only fields apply
        }
        if declares_name_in_scope(p, name, bytes) {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// The referenced name: the identifier's text for a value reference, or the `name` child's text for a
/// `method_invocation`.
fn ref_node_name<'a>(ref_node: Node, bytes: &'a [u8]) -> Result<&'a str, std::str::Utf8Error> {
    if ref_node.kind() == "method_invocation" {
        if let Some(nm) = ref_node.child_by_field_name("name") {
            return nm.utf8_text(bytes);
        }
    }
    ref_node.utf8_text(bytes)
}

/// Whether scope node `scope` introduces `name` as a local/param/etc. anywhere within it (searched
/// broadly — over-inclusion only suppresses diagnostics). Handles method / lambda / constructor
/// parameters, `catch` params, enhanced- and classic-`for` variables, try-with-resources resources,
/// local variable declarations, and record/instanceof pattern variables. Mirrors
/// [`crate::undefined_var::declares_name_in_scope`].
fn declares_name_in_scope(scope: Node, name: &str, bytes: &[u8]) -> bool {
    match scope.kind() {
        "method_declaration" | "constructor_declaration" | "lambda_expression" => {
            if params_declare(scope, name, bytes) {
                return true;
            }
        }
        "catch_clause" => {
            let mut c = scope.walk();
            for ch in scope.named_children(&mut c) {
                if ch.kind() == "catch_formal_parameter" {
                    if let Some(nm) = ch.child_by_field_name("name") {
                        if nm.utf8_text(bytes) == Ok(name) {
                            return true;
                        }
                    }
                }
            }
        }
        "for_statement" | "enhanced_for_statement" => {
            if let Some(nm) = scope.child_by_field_name("name") {
                if nm.utf8_text(bytes) == Ok(name) {
                    return true;
                }
            }
        }
        _ => {}
    }

    let mut stack: Vec<Node> = Vec::new();
    let mut c = scope.walk();
    for ch in scope.named_children(&mut c) {
        stack.push(ch);
    }
    while let Some(n) = stack.pop() {
        match n.kind() {
            "variable_declarator" => {
                if let Some(nm) = n.child_by_field_name("name") {
                    if nm.utf8_text(bytes) == Ok(name) {
                        return true;
                    }
                }
            }
            "pattern" | "type_pattern" => {
                if let Some(nm) = n.child_by_field_name("name") {
                    if nm.utf8_text(bytes) == Ok(name) {
                        return true;
                    }
                }
                let mut cc = n.walk();
                for ch in n.named_children(&mut cc) {
                    if ch.kind() == "identifier" && ch.utf8_text(bytes) == Ok(name) {
                        return true;
                    }
                }
            }
            "catch_formal_parameter"
            | "formal_parameter"
            | "spread_parameter"
            | "resource"
            | "enhanced_for_statement"
            | "instanceof_expression" => {
                if let Some(nm) = n.child_by_field_name("name") {
                    if nm.utf8_text(bytes) == Ok(name) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        let mut cc = n.walk();
        for ch in n.named_children(&mut cc) {
            stack.push(ch);
        }
    }
    false
}

/// Whether a parameter-bearing scope declares `name` in its `parameters` list. Mirrors
/// [`crate::undefined_var::params_declare`].
fn params_declare(member: Node, name: &str, bytes: &[u8]) -> bool {
    let Some(params) = member.child_by_field_name("parameters") else { return false };
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        match p.kind() {
            "formal_parameter" | "spread_parameter" => {
                if let Some(nm) = p.child_by_field_name("name") {
                    if nm.utf8_text(bytes) == Ok(name) {
                        return true;
                    }
                }
            }
            "identifier" => {
                if p.utf8_text(bytes) == Ok(name) {
                    return true;
                }
            }
            "inferred_parameters" => {
                let mut ic = p.walk();
                for id in p.named_children(&mut ic) {
                    if id.kind() == "identifier" && id.utf8_text(bytes) == Ok(name) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// The same `MapResolver` mock the members / fields / undefined_var tests use.
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

    fn field(name: &str, ty: &str) -> Member {
        Member::field(name, TypeRef::simple(ty.to_string())).sig(format!("{ty} {name}"))
    }
    fn method(name: &str, ret: &str) -> Member {
        Member::method(name, TypeRef::simple(ret.to_string()), Vec::new()).sig(format!("{ret} {name}()"))
    }

    /// `com/acme/C extends java/lang/Object`, with:
    ///   * instance field  `instance_counter`, instance method `instance_helper` (the positives);
    ///   * static  field  `STATIC_COUNTER`,  static  method `static_helper`   (the negatives).
    /// The hierarchy is FULLY KNOWN (`Object` has no fields) so absence can be asserted.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "java/lang/Object".to_string(),
            ClassMembers {
                superclass: None,
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/C".to_string(),
            ClassMembers {
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![
                    method("instance_helper", "void"),      // non-static
                    method("static_helper", "void").stat(), // static
                ],
                fields: vec![
                    field("instance_counter", "int"),      // non-static
                    field("STATIC_COUNTER", "int").stat(), // static
                ],
                flags: Default::default(),
            },
        );
        let simple = [("C", "com/acme/C"), ("Object", "java/lang/Object"), ("String", "java/lang/String")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        MapResolver { members, simple }
    }

    /// A resolver whose top-level type `C` has an UNKNOWN supertype (`Base` isn't in the map) → the
    /// hierarchy isn't fully known → everything must be SKIPPED.
    fn resolver_unknown_super() -> MapResolver {
        let mut r = resolver();
        // Re-point C at a missing base.
        if let Some(cm) = r.members.get_mut("com/acme/C") {
            cm.superclass = Some("com/acme/Base".to_string());
        }
        r
    }

    fn diags_with(src: &str, r: &MapResolver) -> Vec<String> {
        static_access_errors(src, r).into_iter().map(|d| d.message).collect()
    }

    /// Wrap a class body under package `com.acme` (so `C`'s FQN is `com/acme/C`, matching the resolver).
    fn wrap(body: &str) -> String {
        format!("package com.acme;\nclass C {{ {body} }}")
    }

    // ── POSITIVES (must flag) ────────────────────────────────────────────────────────────────────

    #[test]
    fn instance_field_and_method_from_static_method_are_flagged() {
        let src = wrap(
            "int instance_counter; void instance_helper(){} \
             static void m(){ instance_counter++; instance_helper(); }",
        );
        let d = diags_with(&src, &resolver());
        assert_eq!(d.len(), 2, "{d:?}");
        assert!(d.iter().any(|m| m.contains("`instance_counter`")), "{d:?}");
        assert!(d.iter().any(|m| m.contains("`instance_helper`")), "{d:?}");
        assert!(d.iter().all(|m| m.contains("static context")), "{d:?}");
    }

    #[test]
    fn instance_field_from_static_initializer_is_flagged() {
        let src = wrap("int instance_counter; static int X; static { X = instance_counter; }");
        let d = diags_with(&src, &resolver());
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`instance_counter`"), "{d:?}");
    }

    // ── NEGATIVES (must NOT flag) ────────────────────────────────────────────────────────────────

    #[test]
    fn static_member_from_static_method_is_not_flagged() {
        // Both a static field and a static method referenced bare from a static method → legal.
        let src = wrap("static int STATIC_COUNTER; static void static_helper(){} \
                        static void m(){ STATIC_COUNTER++; static_helper(); }");
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn local_shadowing_the_member_is_not_flagged() {
        // A local `instance_counter` in the static method shadows the instance field → not this error.
        let src = wrap("int instance_counter; static void m(){ int instance_counter = 0; instance_counter++; }");
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn parameter_shadowing_the_member_is_not_flagged() {
        let src = wrap("int instance_counter; static void m(int instance_counter){ instance_counter++; }");
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn instance_member_from_instance_method_is_not_flagged() {
        // The SAME references, but from a NON-static method → perfectly legal (there's a `this`).
        let src = wrap(
            "int instance_counter; void instance_helper(){} \
             void m(){ instance_counter++; instance_helper(); }",
        );
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn instance_member_from_instance_initializer_is_not_flagged() {
        // A bare `{ … }` (instance initializer) has `this` → legal.
        let src = wrap("int instance_counter; { instance_counter++; }");
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn instance_member_from_constructor_is_not_flagged() {
        let src = wrap("int instance_counter; C(){ instance_counter++; }");
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn qualified_this_reference_is_not_flagged() {
        // `this.instance_counter` is a `field_access` (qualified) → not a bare reference → SKIP.
        let src = wrap("int instance_counter; void n(){ this.instance_counter++; }");
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn hierarchy_not_fully_known_is_not_flagged() {
        // `C extends Base` and `Base` isn't indexed → a member could live there with unknown
        // static-ness → SKIP everything.
        let src = wrap("static void m(){ instance_counter++; instance_helper(); }");
        assert!(
            diags_with(&src, &resolver_unknown_super()).is_empty(),
            "{:?}",
            diags_with(&src, &resolver_unknown_super()),
        );
    }

    #[test]
    fn unknown_bare_name_from_static_method_is_not_flagged() {
        // `mystery` is neither an instance nor a static member → not OUR error (undefined_var owns it).
        let src = wrap("static void m(){ mystery++; }");
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn reference_inside_lambda_in_static_method_is_not_flagged() {
        // A lambda inside a static method has its own scope we don't fully model → SKIP.
        let src = wrap(
            "int instance_counter; \
             static void m(){ Runnable r = () -> { instance_counter++; }; }",
        );
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn reference_inside_nested_class_is_not_flagged() {
        let src = wrap(
            "int instance_counter; \
             static void m(){ class Local { void n(){ instance_counter++; } } }",
        );
        assert!(diags_with(&src, &resolver()).is_empty(), "{:?}", diags_with(&src, &resolver()));
    }

    #[test]
    fn static_import_poisons_whole_file() {
        let src = "package com.acme;\nimport static java.lang.Math.PI;\nclass C { int instance_counter; static void m(){ instance_counter++; } }";
        assert!(diags_with(src, &resolver()).is_empty(), "{:?}", diags_with(src, &resolver()));
    }

    #[test]
    fn parse_error_skips_the_file() {
        let src = "package com.acme;\nclass C { int instance_counter; static void m(){ int x = ; instance_counter++; } }";
        assert!(diags_with(src, &resolver()).is_empty(), "{:?}", diags_with(src, &resolver()));
    }

    #[test]
    fn two_top_level_types_skip_the_file() {
        let src = "package com.acme;\nclass C { int instance_counter; static void m(){ instance_counter++; } }\nclass D {}";
        assert!(diags_with(src, &resolver()).is_empty(), "{:?}", diags_with(src, &resolver()));
    }
}
