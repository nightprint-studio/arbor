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

use crate::support::nodes::child_field_name;

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
    if binds_directly(scope, name, bytes) {
        return true;
    }

    // For a `block` (or any scope), scan its DIRECT and nested statements for declared names WITHOUT
    // crossing into a deeper NEW scope owned by a nested type/lambda — those own their names and we
    // already SKIP identifiers inside them (via the callers' own scope gates), so here we simply gather
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
            // A record-pattern / type-pattern binding: `if (o instanceof String s)` → `s`. A
            // record-pattern COMPONENT (`case Point(int x, var label)`) binds positionally too — the
            // grammar gives its name no field, only a trailing `identifier` child.
            "pattern" | "type_pattern" | "record_pattern_component" => {
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

/// The names a scope node binds itself rather than through a statement inside it: the parameters of
/// a method, constructor or lambda, a `catch` parameter, an enhanced-`for` variable.
fn binds_directly(scope: Node, name: &str, bytes: &[u8]) -> bool {
    match scope.kind() {
        "method_declaration" | "constructor_declaration" | "lambda_expression" => {
            params_declare(scope, name, bytes)
        }
        "catch_clause" => {
            // `catch (E e)` — the `catch_formal_parameter` is a sibling of the catch body block, so
            // a body-only subtree scan would miss it; check the clause's children directly.
            let mut c = scope.walk();
            for ch in scope.named_children(&mut c) {
                if ch.kind() == "catch_formal_parameter"
                    && ch.child_by_field_name("name").is_some_and(|nm| nm.utf8_text(bytes) == Ok(name))
                {
                    return true;
                }
            }
            false
        }
        // Classic `for (int i = …; …)` binds through its `init` declaration, which a subtree scan
        // finds; the enhanced `for (T x : xs)` through a direct `name` field.
        "for_statement" | "enhanced_for_statement" => scope
            .child_by_field_name("name")
            .is_some_and(|nm| nm.utf8_text(bytes) == Ok(name)),
        _ => false,
    }
}

/// [`resolves_as_local`] with Java's block scoping: a name declared inside a block, a `for`, a
/// `catch`, a lambda, a `try`'s resources, a `switch` body or a class body is in scope only inside
/// it, so the scan of an ancestor skips every such nested scope the reference does not sit in.
///
/// For the check that concludes a name binds to NOTHING — there the broad reading hides
/// `int inner` used after its block ends, a loop variable after its loop, a catch parameter after
/// its `catch`. The checks that use a binding to suppress something else keep the broad reading.
///
/// Pattern bindings are still gathered everywhere: their scope follows flow, not blocks
/// (`if (!(o instanceof String s)) return;` binds `s` after the `if`), and over-collecting them only
/// suppresses.
pub(crate) fn resolves_as_local_lexically(ident: Node, top: Node, bytes: &[u8]) -> bool {
    let Ok(name) = referenced_name(ident, bytes) else { return true }; // unreadable → treat as bound
    let top_body_id = top.child_by_field_name("body").map(|b| b.id());
    let mut enclosing: Vec<usize> = Vec::new();
    let mut cur = ident.parent();
    while let Some(p) = cur {
        enclosing.push(p.id());
        cur = p.parent();
    }
    let mut cur = ident.parent();
    while let Some(p) = cur {
        if p.id() == top.id() || Some(p.id()) == top_body_id {
            break;
        }
        if binds_directly(p, name, bytes) || declared_lexically_within(p, name, bytes, &enclosing) {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// Whether `scope`'s subtree declares `name` where a reference inside the `enclosing` nodes can see
/// it: everything outside a nested scope, and pattern bindings anywhere.
fn declared_lexically_within(scope: Node, name: &str, bytes: &[u8], enclosing: &[usize]) -> bool {
    // (node, patterns only): below a nested scope the reference is not in, only a pattern binding
    // can still reach it.
    let mut stack: Vec<(Node, bool)> = Vec::new();
    let mut c = scope.walk();
    for ch in scope.named_children(&mut c) {
        stack.push((ch, false));
    }
    while let Some((n, inherited)) = stack.pop() {
        let patterns_only = inherited || (opens_a_scope(n) && !enclosing.contains(&n.id()));
        if binding_names(n, name, bytes, patterns_only) {
            return true;
        }
        let mut cc = n.walk();
        for ch in n.named_children(&mut cc) {
            stack.push((ch, patterns_only));
        }
    }
    false
}

/// The node kinds whose declarations are invisible outside them.
fn opens_a_scope(n: Node) -> bool {
    matches!(
        n.kind(),
        "block"
            | "for_statement"
            | "enhanced_for_statement"
            | "catch_clause"
            | "lambda_expression"
            | "try_with_resources_statement"
            | "switch_block"
            | "class_body"
    )
}

/// Whether `n` itself declares `name` — as a pattern binding, or, unless `patterns_only`, as a local,
/// a parameter, a resource or a loop variable.
fn binding_names(n: Node, name: &str, bytes: &[u8], patterns_only: bool) -> bool {
    let named = |node: Node| node.child_by_field_name("name").is_some_and(|nm| nm.utf8_text(bytes) == Ok(name));
    match n.kind() {
        "pattern" | "type_pattern" | "record_pattern_component" => {
            if named(n) {
                return true;
            }
            let mut cc = n.walk();
            let bare = n.named_children(&mut cc).any(|ch| ch.kind() == "identifier" && ch.utf8_text(bytes) == Ok(name));
            bare
        }
        "instanceof_expression" => named(n),
        "variable_declarator" | "catch_formal_parameter" | "formal_parameter" | "spread_parameter"
        | "resource" | "enhanced_for_statement" => !patterns_only && named(n),
        _ => false,
    }
}

/// The nearest enclosing method / constructor / lambda of `node` — the scope whose parameters and
/// locals are in scope there, and whose locals a closure can capture.
///
/// `None` when a TYPE declaration comes first: a class body starts a new scope, so a field
/// initializer has no capturable method locals and a method of an anonymous class may legally
/// re-use a name from the method the anonymous class is written in.
///
/// NOT the same question as `checked_throw::enclosing_callable`, which returns the boundary node
/// ITSELF so its caller can see what it was and skip. Here a boundary means "no answer". Two
/// contracts, and they carried one name — merging them would have been a silent behaviour change in
/// whichever check lost.
pub(crate) fn enclosing_executable_scope(node: Node) -> Option<Node> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            "method_declaration" | "constructor_declaration" | "lambda_expression" => return Some(n),
            "class_declaration" | "interface_declaration" | "enum_declaration"
            | "record_declaration" => return None,
            _ => {}
        }
        cur = n.parent();
    }
    None
}

/// Whether a parameter-bearing scope declares `name` in its `parameters` list.
pub(crate) fn params_declare(member: Node, name: &str, bytes: &[u8]) -> bool {
    let Some(params) = member.child_by_field_name("parameters") else { return false };
    // `h -> …`: the single untyped lambda parameter IS the `parameters` field, not a list holding
    // it — iterating its children found nothing, so `h` read as unbound inside its own lambda.
    if params.kind() == "identifier" {
        return params.utf8_text(bytes) == Ok(name);
    }
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
        // The TARGET LABEL of `break outer;` / `continue outer;`. A label lives in its own namespace
        // (JLS §6.5.1) — it is not a variable, and nothing declares it as one, so judging it as a
        // bare value made ordinary labelled-loop code light up with "cannot resolve symbol". The
        // question that IS worth asking about it — does any enclosing statement carry that label —
        // is `branches.rs`'s, and it already answers it as `unknown-label`.
        "break_statement" | "continue_statement" => return false,
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

/// The file's single top-level `class` / `enum` / `interface`, or `None` when there are zero,
/// several, or the shape is anything else. We restrict to ONE top-level type so "the enclosing type"
/// is unambiguous — with two an identifier's owning type would need per-node attribution we skip.
/// A top-level record or annotation present alongside still bails: a record's compact/canonical
/// members are subtle enough to not risk mis-owning an identifier.
///
/// An interface used to bail too, on the grounds that its body has no instance fields to reference
/// bare. True, and beside the point: since Java 8 an interface body has `default` and `static`
/// METHODS, and they call each other bare like any class. Excluding them left every `Failable*` in
/// commons-lang — a `default andThen` whose whole body is `accept(t)` — unreadable to the three
/// checks built on this, and an extraction out of one came out without the `throws E` it needed.
pub(crate) fn single_top_level_type<'t>(root: Node<'t>, bytes: &[u8]) -> Option<TopType<'t>> {
    let mut found: Option<TopType> = None;
    let mut c = root.walk();
    for ch in root.named_children(&mut c) {
        if matches!(
            ch.kind(),
            "class_declaration" | "enum_declaration" | "interface_declaration"
        ) {
            if found.is_some() {
                return None; // more than one top-level type → ambiguous ownership → SKIP
            }
            let name = ch.child_by_field_name("name")?;
            let decl_name = name.utf8_text(bytes).ok()?.to_string();
            found = Some(TopType { node: ch, decl_name });
        } else if matches!(
            ch.kind(),
            "record_declaration" | "annotation_type_declaration"
        ) {
            // A top-level record/annotation present alongside makes ownership murky; bail.
            return None;
        }
    }
    found
}

/// Whether `node`'s nearest enclosing TYPE is exactly `top`, crossing no nested / anonymous / local
/// class body on the way up — the scope in which a bare `foo()` binds to `top`'s methods. Returns
/// `false` (SKIP) on ANY intervening type scope.
///
/// A **lambda** is crossed: its body is not a new scope for method names, because a lambda declares
/// no members and does not rebind `this`. A lambda PARAMETER is typed by the same inference that
/// types it in a receiver call (`list.forEach(x -> svc.take(x))`), so refusing bare calls there bought
/// nothing the receiver-ful checks did not already do without — and cost real answers: `andThen` in
/// commons-lang's `Failable*` interfaces is a one-line lambda that calls `accept(t)`, and
/// `opt.ifPresent(h -> own(h, wrong))` is ordinary legacy code.
///
/// Subtlety: walking UPWARD from a node inside `top`'s own method, we necessarily cross
/// `top`'s OWN body node (`class_body` / `enum_body`) BEFORE reaching the `top` declaration node
/// itself. We must allow that one body but reject every OTHER `class_body`/`enum_body` (which belongs
/// to a nested or anonymous type). So we pin `top`'s body node id up front and only skip on a body
/// whose id differs. An anonymous class `new T(){…}` introduces its own `class_body`; a nested/local
/// `class`/`enum`/`interface` introduces its own declaration node AND body — either trips the guard.
pub(crate) fn scope_is_top_across_lambdas(node: Node, top: Node) -> bool {
    // `top`'s own body node id — the one body we're allowed to cross.
    let top_body_id = top.child_by_field_name("body").map(|b| b.id());

    let mut cur = node.parent();
    while let Some(p) = cur {
        // Reached the top type without crossing a disallowed scope → good.
        if p.id() == top.id() {
            return true;
        }
        match p.kind() {
            // Any nested/local type declaration between us and `top` → its members add/shadow names
            // the caller didn't gather (it gathered `top`'s members + supertypes, nothing else) → SKIP.
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => return false,
            // A class/enum body: allowed ONLY if it's `top`'s own body. Any other body is a nested or
            // anonymous type's body → SKIP.
            "class_body" | "enum_body" | "enum_body_declarations" => {
                if Some(p.id()) != top_body_id {
                    return false;
                }
            }
            _ => {}
        }
        cur = p.parent();
    }
    false
}

/// The declared type text of `name` as visible at `use_node`: a method or lambda parameter, a local
/// declared before the use, or — with `fields` — a field declared before it in an enclosing type.
/// A small, syntactic subset of the inference engine's local resolution.
///
/// Without `fields` the walk stops at the enclosing class body: a name the class declares or
/// inherits is then never answered with a local of some outer scope that happens to share it.
pub(crate) fn declared_type_text(use_node: Node, name: &str, bytes: &[u8], fields: bool) -> Option<String> {
    let use_start = use_node.start_byte();
    let mut scope = use_node.parent();
    while let Some(s) = scope {
        if !fields && s.kind() == "class_body" {
            return None;
        }
        if let Some(params) = s.child_by_field_name("parameters") {
            let mut pw = params.walk();
            for p in params.named_children(&mut pw) {
                if matches!(p.kind(), "formal_parameter" | "spread_parameter")
                    && p.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) == Some(name)
                {
                    return p.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()).map(str::to_string);
                }
            }
        }
        // Locals (and fields, when asked) declared directly in this scope, before the use.
        let mut cw = s.walk();
        for c in s.named_children(&mut cw) {
            if c.start_byte() >= use_start {
                break;
            }
            let declares = match c.kind() {
                "local_variable_declaration" => true,
                "field_declaration" => fields,
                _ => false,
            };
            if declares {
                if let Some(t) = declarator_type(c, name, bytes) {
                    return Some(t);
                }
            }
        }
        scope = s.parent();
    }
    None
}

/// The declared type text of a `local_variable_declaration` / `field_declaration` if it declares
/// `name`.
fn declarator_type(decl: Node, name: &str, bytes: &[u8]) -> Option<String> {
    let ty = decl.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok())?;
    let mut dw = decl.walk();
    for d in decl.named_children(&mut dw) {
        if d.kind() == "variable_declarator"
            && d.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) == Some(name)
        {
            return Some(ty.to_string());
        }
    }
    None
}

