//! Which names an enclosing scope binds — the question every "is this a bare reference or a
//! shadowed local" check has to answer first.
//!
//! Three checks ask it, and two of them carried their own copy. The copies drifted badly: one knew
//! about `try`-with-resources resources, `instanceof` pattern bindings and varargs parameters, and
//! the other did not — so a name shadowed by `if (o instanceof String s)` was invisible to the
//! static-context check and got reported as a violation of a scope it was never in. One copy now,
//! and it is the one that knows the most.
//!
//! **Over-collection is the safe direction, and is deliberate.** A name declared in a *sibling*
//! block of an ancestor is gathered too, which can only ever SUPPRESS a finding, never invent one.
//! Every caller is a never-false-positive check, so a missed report costs less than a wrong one.

use tree_sitter::Node;

use crate::nodes::child_field_name;

/// Whether the name `ident` denotes is bound as a local / parameter / for-var / catch-param /
/// try-resource / lambda-param / pattern var in ANY scope enclosing it, up to `top`.
///
/// `ident` may be the identifier itself or a `method_invocation` — the callers ask about both, and
/// reading the name from either here is what lets the two copies be one. We collect
/// every such name declared anywhere in each ancestor `block` / method / for / etc. and check
/// membership. Over-collecting (a name declared in a sibling block of an ancestor) is *conservative*
/// here — it can only SUPPRESS a diagnostic, never create one, and this check must never false-positive.
pub(crate) fn resolves_as_local(ident: Node, top: Node, bytes: &[u8]) -> bool {
    let Ok(name) = referenced_name(ident, bytes) else { return true }; // unreadable → treat as bound

    // The one body node of `top` we must NOT treat as a "locals scope": scanning the whole class body
    // for locals would suppress a genuine positive whenever ANY other method reuses the same local name
    // (`i`, `result`, …) — gutting detection. `top`'s members contribute FIELDS, resolved separately.
    let top_body_id = top.child_by_field_name("body").map(|b| b.id());

    // Walk ancestors from the identifier upward, checking each executable scope for a binding of
    // `name`. Stop at `top`'s body / `top` itself — beyond the enclosing method, only fields apply.
    let mut cur = ident.parent();
    while let Some(p) = cur {
        if p.id() == top.id() || Some(p.id()) == top_body_id {
            break; // reached the type / its body — not a locals scope
        }
        if declares_name_in_scope(p, name, bytes) {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// Whether scope node `scope` introduces `name` as a local/param/etc. anywhere within it (searched
/// broadly — over-inclusion only suppresses diagnostics, never adds them). Handles: method / lambda /
/// constructor parameters, `catch` params, enhanced-`for` and classic-`for` variables, `try`-with-
/// resources resources, local variable declarations, and record/instanceof pattern variables.
pub(crate) fn declares_name_in_scope(scope: Node, name: &str, bytes: &[u8]) -> bool {
    // For the parameter-bearing scopes, check the parameter list directly.
    match scope.kind() {
        "method_declaration" | "constructor_declaration" | "lambda_expression" => {
            if params_declare(scope, name, bytes) {
                return true;
            }
        }
        "catch_clause" => {
            // `catch (E e)` — the `catch_formal_parameter` is a sibling of the catch body block, so
            // the body-only subtree scan below would miss it; check the clause's children directly.
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
            // Classic `for (int i = …; …)` uses an `init` local_variable_declaration; the enhanced
            // `for (T x : xs)` uses a `name` field. Both are captured by the subtree scan below, but
            // the enhanced form's variable is a direct `name` field we check explicitly.
            if let Some(nm) = scope.child_by_field_name("name") {
                if nm.utf8_text(bytes) == Ok(name) {
                    return true;
                }
            }
        }
        _ => {}
    }

    // For a `block` (or any scope), scan its DIRECT and nested statements for declared names WITHOUT
    // crossing into a deeper NEW scope owned by a nested type/lambda — those own their names and we
    // already SKIP identifiers inside them (via `scope_is_directly_top`), so here we simply gather
    // broadly: any local/resource/pattern var textually inside `scope`. Over-collection is safe.
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
            // A record-pattern / type-pattern binding: `if (o instanceof String s)` → `s`.
            "pattern" | "type_pattern" => {
                if let Some(nm) = n.child_by_field_name("name") {
                    if nm.utf8_text(bytes) == Ok(name) {
                        return true;
                    }
                }
                // Some grammars expose the binding as a bare identifier child.
                let mut cc = n.walk();
                for ch in n.named_children(&mut cc) {
                    if ch.kind() == "identifier" && ch.utf8_text(bytes) == Ok(name) {
                        return true;
                    }
                }
            }
            // Params, try-with-resources resources, and an enhanced-for var all bind via a `name`
            // field. An `instanceof_expression` carries the pattern-binding `name` in some grammars
            // (`o instanceof String s` → `s`). All of these are collected broadly (over-collection
            // only SUPPRESSES a diagnostic, never adds one — sound for this never-false-positive check).
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

/// Whether a parameter-bearing scope declares `name` in its `parameters` list.
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
            // A bare-identifier lambda param (`x -> …`) or an inferred_parameters list member.
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

/// The name a reference node denotes: a `method_invocation`'s `name` child, or the node's own text.
fn referenced_name<'a>(node: Node, bytes: &'a [u8]) -> Result<&'a str, std::str::Utf8Error> {
    if node.kind() == "method_invocation" {
        if let Some(nm) = node.child_by_field_name("name") {
            return nm.utf8_text(bytes);
        }
    }
    node.utf8_text(bytes)
}


/// Whether a bare identifier stands in a slot that holds a VALUE — rather than a declaration's name,
/// a member selector, a type argument or a label.
///
/// The **position half** of the question. Both callers then add their own scope gate on top:
/// `undefined_var` requires the reference to sit directly in the file's single top-level type,
/// `static_access` requires it to sit in a `static` context of that type. Splitting it this way is
/// what the two copies were already doing informally — one of them just did it with a smaller list,
/// and so judged an identifier under a `Foo::bar` method reference as a bare value. It is not.
/// Whether `ident` is a bare *value* reference we're entitled to judge, given the file's top-level
/// type `top`. Combines the POSITION guards (it's a primary-expression identifier, not a declaration
/// / suffix / method-name / type / label position) with the SCOPE guards (enclosing type is exactly
/// `top`, no intervening nested class body or lambda). Any doubt → `false` (SKIP).
pub(crate) fn is_value_position(ident: Node) -> bool {
    // POSITION: the parent node kind + the field this identifier occupies determine whether it's a
    // value. Reject every non-value slot explicitly.
    let Some(parent) = ident.parent() else { return false };
    let pkind = parent.kind();

    // The field this identifier fills in its parent, if any — the reliable slot discriminator.
    let field_of_parent = child_field_name(parent, ident);

    // A `variable_declarator` (`int y = count;`) has BOTH a `name` slot (the binding, skip) and a
    // `value` slot (the initializer — a genuine value reference we DO judge). `count` above is the
    // `value`; `y` is the `name`. Skip only the `name` slot.
    if pkind == "variable_declarator" {
        if field_of_parent.as_deref() == Some("name") {
            return false;
        }
        // else: the `value` (RHS) bare identifier → judge it (fall through to scope checks below).
    } else {
        match pkind {
        // A `foo.bar` member access. The `field` (suffix) is a member handled by the fields check.
        // The `object` HEAD (`foo`) is a qualifier that could be a **variable**, but equally a **type**
        // (`Integer.MAX_VALUE`) or a **package segment** (`java.util.List`) — ambiguities we don't
        // model. The PARAMOUNT rule (never a false positive) forces us to SKIP the head too: only a
        // TRULY STANDALONE bare identifier (an argument / operand / RHS, with no `.` before or after)
        // is safe to judge. So we skip BOTH slots of a `field_access`.
        "field_access" => return false,
        // `a.b.C` scoped forms are package/type qualifiers we never judge (head or suffix).
        "scoped_identifier" | "scoped_type_identifier" | "scoped_type_arguments" => return false,
        // A method call. The `name` slot is the method (members check owns it). The `object` HEAD is a
        // qualifier with the same type/package/variable ambiguity as `field_access` → SKIP both. Only a
        // BARE call `foo()` (no `object`) would leave an identifier here, and that's the `name` slot,
        // already skipped. So any identifier directly under a `method_invocation` is skipped.
        "method_invocation" => return false,
        // A method reference `Type::method` / `expr::method` / `Type::new`. The RHS is the referenced
        // method NAME (owned by method-ref resolution, never a bare variable); the LHS is a
        // type-or-value qualifier with the same ambiguity as `field_access`. Skip both slots — else
        // `Long::sum` / `Objects::nonNull` wrongly flag `sum` / `nonNull` as an undefined symbol.
        "method_reference" => return false,
        // Declaration NAME slots — the identifier introduces a binding, not references one.
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
        // Type positions — never a value.
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
        // A `switch`/`case` label constant: `case FOO:` — an enum-constant / constant-name context we
        // don't judge (it resolves against the selector's enum type, not the local scope).
        "switch_label" | "constant" => return false,
        _ => {}
        }
    }

    // SCOPE: the nearest enclosing type must be exactly `top`, with no nested/anonymous/local class
    // body and no lambda between the identifier and `top`. Either kind of intervening scope could
    // declare or capture a name we don't model → we must not judge identifiers inside them.
    true
}

/// A located top-level type: its CST node plus its declared simple name.
pub(crate) struct TopType<'t> {
    pub(crate) node: Node<'t>,
    pub(crate) decl_name: String,
}

/// The file's single top-level `class`/`enum`, or `None` when there are zero, several, or the shape
/// is anything else. We restrict to ONE top-level class/enum so "the enclosing type" is unambiguous —
/// with two top-level classes an identifier's owning type would need per-node attribution we skip.
/// A top-level interface/record/annotation present alongside also bails: an interface body has no
/// instance fields to reference bare, and a record's compact/canonical members are subtle enough to
/// not risk mis-owning an identifier.
pub(crate) fn single_top_level_type<'t>(root: Node<'t>, bytes: &[u8]) -> Option<TopType<'t>> {
    let mut found: Option<TopType> = None;
    let mut c = root.walk();
    for ch in root.named_children(&mut c) {
        if matches!(ch.kind(), "class_declaration" | "enum_declaration") {
            if found.is_some() {
                return None; // more than one top-level class/enum → ambiguous ownership → SKIP
            }
            let name = ch.child_by_field_name("name")?;
            let decl_name = name.utf8_text(bytes).ok()?.to_string();
            found = Some(TopType { node: ch, decl_name });
        } else if matches!(
            ch.kind(),
            "interface_declaration" | "record_declaration" | "annotation_type_declaration"
        ) {
            // A top-level interface/record/annotation present alongside makes ownership murky; bail.
            return None;
        }
    }
    found
}
