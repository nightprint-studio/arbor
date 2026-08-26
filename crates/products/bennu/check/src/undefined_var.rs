//! Undefined-variable diagnostics — a **bare identifier used as a value** that resolves to nothing:
//! javac's "cannot find symbol: variable x". This is the single most false-positive-prone check in
//! the crate — a bare name can bind through a great many scopes (locals, params, fields, inherited
//! fields, enclosing-class fields, enum constants, static imports, static type qualifiers, …). So the
//! PARAMOUNT rule here is: **never a false positive**. Every doubt SKIPs. It is far better to flag
//! nothing than to flag one legal name.
//!
//! The gate is therefore extreme. Two layers:
//!
//! WHOLE-FILE guards (any → produce NOTHING for the file):
//!   * a parse error anywhere near where we'd flag (`has_error` on the tree root) — a broken buffer
//!     mis-shapes the CST and could make a legal name look bare.
//!   * an `import static X.*;` whose owner `X` (or a supertype) is un-indexed — a wildcard from an
//!     unknown type could supply ANY bare name, so we can't soundly flag anything. A SPECIFIC
//!     `import static X.foo;` and a wildcard whose owner IS fully known are modelled precisely (see
//!     RESOLUTION 6) rather than poisoning the file.
//!
//! PER-IDENTIFIER guards (any failing → SKIP that identifier):
//!   * it must be a genuine *value* reference — an `identifier` node in a primary-expression
//!     position, NOT a declaration name, NOT a method-invocation `name`, NOT a `field_access`/scoped
//!     suffix, NOT a type / annotation / label / case-label / import / package context;
//!   * its nearest enclosing type must be the file's TOP-LEVEL class/enum, and its enclosing method a
//!     direct member of it — NO intervening nested/anonymous/local `class_body`, NO enclosing lambda
//!     (either could capture / declare a name in a scope we don't model). Any ambiguity → SKIP.
//!
//! RESOLUTION — only flagged when the name matches NONE of these AND the type hierarchy is fully known:
//!   1. a local / parameter / for-var / catch-param / try-resource / pattern-var in any enclosing
//!      scope (collected textually from every ancestor `block`, the method params, etc.);
//!   2. a field of the enclosing top-level type or any FULLY-KNOWN supertype — if the hierarchy has
//!      any gap, SKIP (an un-indexed base could declare the field);
//!   3. a resolvable TYPE name (a bare `Foo` can head a static access `Foo.BAR`);
//!   4. an enum constant of the enclosing type;
//!   5. a keyword (`this`/`super`/`true`/`false`/`null`) — these aren't `identifier` nodes anyway,
//!      but we guard defensively.
//!   6. a bare name supplied by an `import static …` — a specific member (`import static X.foo;` → `foo`),
//!      or any member of a fully-known wildcard owner (`import static X.*;`).
//!
//! Only when the name matches none of 1–6, the hierarchy is fully known, no unresolved static wildcard
//! is present, and there's no intervening nested class / lambda, do we flag `Cannot resolve symbol `x``.

use std::collections::HashSet;

use bennu_java::prelude::{
    extract_symbols, static_import_targets, FileSymbols, MemberKind, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::resolve::type_binary;
use crate::walk::{for_each_supertype, hierarchy_fully_known};

/// The bare `java.lang` type names available without an import. A standalone bare identifier matching
/// one of these is a type reference (e.g. heading a static access we may not have modelled as such) —
/// never an undefined variable. Mirrors [`crate::types::JAVA_LANG`] intent: a minimal resolver may not
/// seed these, so we hard-exclude them for soundness. Kept small — only the common ones a legacy file
/// touches bare — but its only effect is to SUPPRESS, so over-inclusion is harmless here.
const JAVA_LANG_TYPES: &[&str] = &[
    "String", "Object", "Integer", "Long", "Boolean", "Double", "Float", "Character", "Byte",
    "Short", "Number", "Math", "System", "Thread", "Class", "Void", "StringBuilder", "StringBuffer",
    "Exception", "Throwable", "Error", "RuntimeException", "Enum", "Runnable", "Comparable",
    "Iterable", "CharSequence",
];

/// Parse `source` and flag bare-identifier value references that resolve to nothing.
pub fn undefined_var(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::check::collect_nodes(root);
    undefined_var_errors_in(root, &nodes, source, &symbols, resolver)
}

/// The tree-driven core: mirrors [`crate::types::unresolved_types_in`] / [`crate::members::unknown_members_in`].
/// Iterates the shared pre-collected `nodes` (one DFS) and reuses `root` + `symbols` + the `resolver`.
///
/// Uses of the parameters: `root` — the whole-file `has_error` + static-import guard, and locating
/// the single top-level type; `nodes` — the flat node list to scan for candidate identifiers;
/// `source` — the byte text for names; `symbols` — the file's `imports` (static-import guard) and
/// declared `types` (resolve the enclosing type's binary + its enum constants); `resolver` — resolve
/// the enclosing type's field hierarchy and simple type names.
pub fn undefined_var_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();

    // ── WHOLE-FILE guard: a parse error anywhere → the CST is untrustworthy. ──────────────────────
    // `has_error` on the root reports an ERROR node anywhere in the tree. A broken buffer can nest or
    // re-shape nodes so a legal name looks like a bare value reference (or hide a declaration we'd
    // need to see it). Rather than reason about "near where we'd flag", we bail on any file error —
    // the maximally conservative choice, and this check runs continuously while the user is typing.
    if root.has_error() {
        return Vec::new();
    }

    // ── Locate the file's single TOP-LEVEL class or enum. ────────────────────────────────────────
    // We only analyse identifiers whose enclosing type IS this one (no nested/anonymous/local class in
    // between). If there are zero or several top-level classes/enums, or the one type doesn't resolve
    // to a fully-known hierarchy, we can't gather its fields soundly → produce nothing.
    let Some(top) = single_top_level_type(root, bytes) else {
        return Vec::new();
    };

    // Resolve the top-level type to a binary name and require its ENTIRE hierarchy be known — else a
    // field could live in an un-indexed base and every bare field reference would be a false positive.
    let Some(top_binary) = type_binary(&top.decl_name, symbols, resolver) else {
        return Vec::new();
    };
    if !hierarchy_fully_known(resolver, &top_binary) {
        return Vec::new();
    }

    // Field names across the top type + every (fully-known) supertype. Gathered once for the file.
    let mut field_names: HashSet<String> = HashSet::new();
    for_each_supertype(resolver, &top_binary, &mut |_bn, cm| {
        for m in &cm.fields {
            if m.kind == MemberKind::Field {
                field_names.insert(m.name.clone());
            }
        }
    });

    // Enum constants of the top type (a bare `RED` inside an `enum Color { RED, GREEN }` is legal).
    // These are `enum_constant` nodes under the enum body — the resolver's `fields` list may or may
    // not carry them depending on the index, so we read them straight from the CST to be safe.
    let mut enum_constants: HashSet<String> = HashSet::new();
    collect_enum_constants(top.node, bytes, &mut enum_constants);

    // ── Bare names supplied by `import static …` ─────────────────────────────────────────────────
    // A static import binds an owner's static members into the bare namespace, so such a name is NOT
    // undefined. We model this precisely instead of poisoning the whole file:
    //   * a SPECIFIC `import static X.foo;` declares the bare name `foo` (whether or not X resolves).
    //   * a WILDCARD `import static X.*;` supplies EVERY member of X's hierarchy — but only if that
    //     hierarchy is fully known; if X (or a supertype) is un-indexed it could supply ANY name, so
    //     we bail on the whole file (the old conservative behaviour, now scoped to just this case).
    // Over-inclusion is safe here (it only ever SUPPRESSES a diagnostic), so a wildcard collects every
    // member name (instance ones too) rather than filtering to statics.
    let mut static_import_names: HashSet<String> = HashSet::new();
    for t in static_import_targets(&symbols.imports) {
        match t.member {
            Some(m) => {
                static_import_names.insert(m);
            }
            None => {
                if !hierarchy_fully_known(resolver, &t.owner_binary) {
                    return Vec::new(); // unresolved wildcard owner → can't rule out any name
                }
                for_each_supertype(resolver, &t.owner_binary, &mut |_bn, cm| {
                    for member in cm.methods.iter().chain(cm.fields.iter()) {
                        static_import_names.insert(member.name.clone());
                    }
                });
            }
        }
    }

    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "identifier" {
            continue;
        }
        // Is this identifier a genuine bare *value* reference we're allowed to judge? (position +
        // scope guards). Every rejection here is a deliberate SKIP for soundness.
        if !is_judgeable_value_ident(n, top.node) {
            continue;
        }
        let Ok(name) = n.utf8_text(bytes) else { continue };

        // RESOLUTION 5: keyword-ish tokens. `this`/`super`/`true`/`false`/`null` parse as their own
        // node kinds, not `identifier`, so we won't even reach here for them — but guard defensively
        // in case a grammar quirk ever surfaces one as an identifier.
        if matches!(name, "this" | "super" | "true" | "false" | "null" | "var") {
            continue;
        }
        // A `java.lang` type name used bare → a type reference, never an undefined variable.
        if JAVA_LANG_TYPES.contains(&name) {
            continue;
        }
        // RESOLUTION 1: a local / param / for-var / catch-param / resource / pattern var in ANY
        // enclosing scope. Collected per-identifier by walking its ancestor scopes.
        if resolves_as_local(n, top.node, bytes) {
            continue;
        }
        // RESOLUTION 2: a field of the enclosing type or a known supertype.
        if field_names.contains(name) {
            continue;
        }
        // RESOLUTION 4: an enum constant of the enclosing enum.
        if enum_constants.contains(name) {
            continue;
        }
        // RESOLUTION 3: a resolvable TYPE name — a bare `Foo` legally heads `Foo.BAR` (static field /
        // nested-type access). If the resolver knows the simple name as a type, it's not undefined.
        if resolver.resolve_simple_name(name, &symbols.imports).is_some() {
            continue;
        }
        // A type declared in THIS file is also a valid bare head (`Helper.CONST`). `type_binary`
        // consults same-file `symbols.types` before the resolver, so this covers same-file types too.
        if type_binary(name, symbols, resolver).is_some() {
            continue;
        }
        // …and so is a nested type INHERITED from a supertype (JLS §8.1.5): a subclass writes
        // `Inner.CONST` for `Base.Inner.CONST`, with no import, because the name is in scope by
        // inheritance. Neither the resolver's simple-name index nor `type_binary` can see that —
        // they don't know which type the name was written inside — so it is asked here, where
        // the enclosing type's binary name is already established.
        if crate::resolve::inherited_member_type(&top_binary, name, resolver).is_some() {
            continue;
        }
        // RESOLUTION 6: a bare name brought in by an `import static …` (a specific member, or a member
        // of a fully-known wildcard owner). Precomputed in `static_import_names`.
        if static_import_names.contains(name) {
            continue;
        }

        // Matched NONE of 1–6, hierarchy fully known, no unresolved static wildcard, no intervening
        // nested class / lambda → the name genuinely resolves to nothing here.
        out.push(crate::check_id::CheckId::UnresolvedSymbol.at(n, format!("Cannot resolve symbol `{name}`")));
    }
    out
}

/// A located top-level type: its CST node plus its declared simple name.
struct TopType<'t> {
    node: Node<'t>,
    decl_name: String,
}

/// The file's single top-level `class`/`enum`, or `None` when there are zero, several, or the shape
/// is anything else. We restrict to ONE top-level class/enum so "the enclosing type" is unambiguous —
/// with two top-level classes an identifier's owning type would need per-node attribution we skip.
/// A top-level interface/record/annotation present alongside also bails: an interface body has no
/// instance fields to reference bare, and a record's compact/canonical members are subtle enough to
/// not risk mis-owning an identifier.
fn single_top_level_type<'t>(root: Node<'t>, bytes: &[u8]) -> Option<TopType<'t>> {
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

/// Collect the enum-constant names declared directly in `top` when it's an enum body. A no-op for a
/// class. Read from the CST (`enum_constant` nodes) so we don't depend on whether the resolver's
/// field list includes synthetic enum constants.
fn collect_enum_constants(top: Node, bytes: &[u8], out: &mut HashSet<String>) {
    if top.kind() != "enum_declaration" {
        return;
    }
    let Some(body) = top.child_by_field_name("body") else { return };
    // The body holds an `enum_body_declarations` plus the constant list; scan for `enum_constant`.
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            if ch.kind() == "enum_constant" {
                if let Some(name) = ch.child_by_field_name("name") {
                    if let Ok(t) = name.utf8_text(bytes) {
                        out.insert(t.to_string());
                    }
                }
                // Don't descend into a constant's class body (its own scope) — irrelevant here.
                continue;
            }
            // Only descend the shallow body wrappers, not method bodies etc.
            if matches!(ch.kind(), "enum_body_declarations") {
                stack.push(ch);
            }
        }
    }
}

/// Whether `ident` is a bare *value* reference we're entitled to judge, given the file's top-level
/// type `top`. Combines the POSITION guards (it's a primary-expression identifier, not a declaration
/// / suffix / method-name / type / label position) with the SCOPE guards (enclosing type is exactly
/// `top`, no intervening nested class body or lambda). Any doubt → `false` (SKIP).
fn is_judgeable_value_ident(ident: Node, top: Node) -> bool {
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
    scope_is_directly_top(ident, top)
}

/// The field name that immediate child `child` occupies in `parent` (`name`, `value`, `object`,
/// `field`, …), or `None` if it fills no named field. Uses a cursor to read field names.
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

/// Whether `ident`'s nearest enclosing type is exactly `top`, crossing NO lambda and no
/// nested/anonymous/local class body on the way up. Returns `false` (SKIP) on ANY intervening scope we
/// don't fully model.
///
/// Subtlety: walking UPWARD from an identifier inside `top`'s own method, we necessarily cross
/// `top`'s OWN body node (`class_body` / `enum_body`) BEFORE reaching the `top` declaration node
/// itself. We must allow that one body but reject every OTHER `class_body`/`enum_body` (which belongs
/// to a nested or anonymous type). So we pin `top`'s body node id up front and only skip on a body
/// whose id differs. An anonymous class `new T(){…}` introduces its own `class_body`; a nested/local
/// `class`/`enum`/`interface` introduces its own declaration node AND body — either trips the guard.
fn scope_is_directly_top(ident: Node, top: Node) -> bool {
    // `top`'s own body node id — the one body we're allowed to cross.
    let top_body_id = top.child_by_field_name("body").map(|b| b.id());

    let mut cur = ident.parent();
    while let Some(p) = cur {
        // Reached the top type without crossing a disallowed scope → good.
        if p.id() == top.id() {
            return true;
        }
        match p.kind() {
            // A lambda: its parameters/captures live in a scope we don't fully model → SKIP.
            "lambda_expression" => return false,
            // Any nested/local type declaration between us and `top` → its members add/shadow names we
            // didn't gather (we only gathered `top`'s fields + supertypes) → SKIP.
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

/// Whether `name` (the identifier's text) is declared as a local / parameter / for-var / catch-param /
/// try-resource / lambda-param / pattern var in ANY scope enclosing `ident`, up to `top`. We collect
/// every such name declared anywhere in each ancestor `block` / method / for / etc. and check
/// membership. Over-collecting (a name declared in a sibling block of an ancestor) is *conservative*
/// here — it can only SUPPRESS a diagnostic, never create one, and this check must never false-positive.
fn resolves_as_local(ident: Node, top: Node, bytes: &[u8]) -> bool {
    let Ok(name) = ident.utf8_text(bytes) else { return true }; // unreadable → SKIP (treat as resolved)

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
fn declares_name_in_scope(scope: Node, name: &str, bytes: &[u8]) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// The same `MapResolver` mock the members / fields tests use: a `binary → members` map + a
    /// `simple → binary` table.
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

    /// `com/acme/C extends com/acme/Base`. `C` declares field `count`; `Base` declares inherited field
    /// `base`. `Object` is the ultimate base with no fields. Plus a resolvable `Helper` type (heads a
    /// static access `Helper.CONST`). The hierarchy is FULLY KNOWN so absence can be asserted.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert(
            "java/lang/Object".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: None,
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/Base".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![field("base", "int")],
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/C".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("com/acme/Base".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![field("count", "int")],
                flags: Default::default(),
            },
        );
        members.insert(
            "com/acme/Helper".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: vec![field("CONST", "int")],
                flags: Default::default(),
            },
        );
        // `java.lang.Math` — a static-import owner with a static field `PI` and method `sqrt`, so the
        // wildcard `import static java.lang.Math.*;` can be modelled precisely (Math + Object known).
        members.insert(
            "java/lang/Math".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: vec![Member::method(
                    "sqrt",
                    TypeRef::simple("double"),
                    vec![TypeRef::simple("double")],
                )],
                fields: vec![field("PI", "double")],
                flags: Default::default(),
            },
        );
        let simple = [
            ("C", "com/acme/C"),
            ("Base", "com/acme/Base"),
            ("Helper", "com/acme/Helper"),
            ("Object", "java/lang/Object"),
            ("String", "java/lang/String"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// A resolver whose top-level type `C` has an UNKNOWN supertype (`Base` isn't in the map), so the
    /// hierarchy is not fully known → every bare name must be SKIPPED.
    fn resolver_unknown_super() -> MapResolver {
        let mut r = resolver();
        r.members.remove("com/acme/Base");
        r
    }

    /// Wrap `body` in `class C { void m() { … } }` under package `com.acme` (so `C`'s FQN is
    /// `com/acme/C`, matching the resolver) and collect the messages.
    fn diags_with(header: &str, r: &MapResolver) -> Vec<String> {
        undefined_var(header, r).into_iter().map(|d| d.message).collect()
    }

    fn in_method(body: &str) -> String {
        format!("package com.acme;\nclass C extends Base {{ int count; void m() {{ {body} }} }}")
    }

    fn diags(body: &str) -> Vec<String> {
        diags_with(&in_method(body), &resolver())
    }

    // ── POSITIVES (must flag) ────────────────────────────────────────────────────────────────────

    #[test]
    fn undefined_bare_identifier_is_flagged() {
        // `name` local exists, but `nam` is nothing → flagged.
        let d = diags("String name = \"x\"; System.out.println(nam);");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`nam`"), "{d:?}");
    }

    #[test]
    fn undefined_in_plain_expression_is_flagged() {
        let d = diags("int y = zzz + 1;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`zzz`"), "{d:?}");
    }

    // ── NEGATIVES (must NOT flag) ────────────────────────────────────────────────────────────────

    #[test]
    fn local_variable_is_resolved() {
        assert!(diags("String name = \"x\"; System.out.println(name);").is_empty());
    }

    #[test]
    fn parameter_is_resolved() {
        let src = "package com.acme;\nclass C extends Base { int count; void m(String p) { use(p); } void use(String s) {} }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn own_field_is_resolved() {
        assert!(diags("int y = count;").is_empty());
    }

    #[test]
    fn inherited_field_is_resolved() {
        // `base` is declared on `Base` (C's superclass) — the supertype walk must find it.
        assert!(diags("int y = base;").is_empty());
    }

    #[test]
    fn resolvable_type_name_is_not_flagged() {
        // `Helper` heads a static access `Helper.CONST` — a resolvable type, not an undefined var.
        assert!(diags("int y = Helper.CONST;").is_empty());
    }

    #[test]
    fn for_loop_variable_is_resolved() {
        // The classic-`for` variable `i` is a local of the loop scope — used bare, must not flag.
        assert!(diags("for (int i = 0; i < 3; i++) { System.out.println(i); }").is_empty());
    }

    #[test]
    fn enhanced_for_variable_is_resolved() {
        // The enhanced-`for` variable `s` (a `name` field on the statement) — used bare, must not flag.
        assert!(diags("String[] xs = null; for (String s : xs) { System.out.println(s); }").is_empty());
    }

    #[test]
    fn catch_parameter_is_resolved() {
        assert!(diags("try {} catch (Exception e) { System.out.println(e); }").is_empty());
    }

    #[test]
    fn specific_static_import_is_precise_not_a_poison() {
        // A SPECIFIC `import static X.PI;` declares the bare name `PI` (so it isn't undefined), but it
        // no longer poisons the file — a genuinely undefined name is still flagged.
        let src = "package com.acme;\nimport static java.lang.Math.PI;\n\
                   class C extends Base { int count; void m() { double x = PI; System.out.println(totallyUndefined); } }";
        let d = diags_with(src, &resolver());
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("totallyUndefined"), "{d:?}");
        assert!(!d.iter().any(|m| m.contains("`PI`")), "the imported member PI is resolved: {d:?}");
    }

    #[test]
    fn wildcard_static_import_from_known_owner_is_precise() {
        // `import static Math.*;` supplies Math's members (`PI`) → not undefined; a non-member
        // (`sqrtish`) IS flagged, because Math's hierarchy is fully known.
        let src = "package com.acme;\nimport static java.lang.Math.*;\n\
                   class C extends Base { int count; void m() { double x = PI; System.out.println(sqrtish); } }";
        let d = diags_with(src, &resolver());
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("sqrtish"), "{d:?}");
        assert!(!d.iter().any(|m| m.contains("`PI`")), "PI is a Math member: {d:?}");
    }

    #[test]
    fn wildcard_static_import_from_unknown_owner_still_skips_file() {
        // A wildcard whose owner isn't indexed could supply ANY bare name → we can't soundly flag
        // anything, so the whole file is skipped (the conservative fallback, now scoped to this case).
        let src = "package com.acme;\nimport static com.unknown.Lib.*;\n\
                   class C extends Base { int count; void m() { System.out.println(whateverName); } }";
        assert!(diags_with(src, &resolver()).is_empty(), "{:?}", diags_with(src, &resolver()));
    }

    #[test]
    fn method_name_is_not_flagged() {
        // A bare call `foo()` — the `name` slot is a method, handled by the members check, not here.
        assert!(diags("foo();").is_empty());
    }

    #[test]
    fn method_reference_name_is_not_flagged() {
        // The name after `::` is a referenced method, not a bare variable — never flag it even though
        // it resolves to nothing in local scope. Regression for `Long::sum` / `Objects::nonNull`.
        assert!(diags("Runnable r = System::gc;").is_empty(), "{:?}", diags("Runnable r = System::gc;"));
        let src = "java.util.List<Long> xs = null; xs.stream().reduce(Long::sum);";
        assert!(diags(src).is_empty(), "{:?}", diags(src));
        let obj = "java.util.List<String> ys = null; ys.stream().filter(java.util.Objects::nonNull);";
        assert!(diags(obj).is_empty(), "{:?}", diags(obj));
    }

    #[test]
    fn field_access_suffix_is_not_flagged() {
        // `obj.foo` — `foo` is a member suffix, not a bare value. (`obj` is a resolved local.)
        assert!(diags("String obj = \"x\"; int n = obj.length();").is_empty());
        // And a genuinely unknown suffix must not be flagged BY THIS check (fields check owns it).
        assert!(diags("String obj = \"x\"; Object z = obj.nonexistentSuffix;").is_empty());
    }

    #[test]
    fn identifier_in_nested_class_is_skipped() {
        // A bare name inside a nested class body → SKIP (its scope isn't the top type's).
        let src = "package com.acme;\nclass C extends Base { int count; class Inner { void n() { System.out.println(mystery); } } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn identifier_in_anonymous_class_is_skipped() {
        let src = "package com.acme;\nclass C extends Base { int count; void m() { Runnable r = new Runnable() { public void run() { System.out.println(mystery); } }; } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn identifier_in_lambda_is_skipped() {
        let src = "package com.acme;\nclass C extends Base { int count; void m() { Runnable r = () -> System.out.println(mystery); } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn unknown_supertype_skips_everything() {
        // `Base` isn't indexed → the hierarchy isn't fully known → a field could live there → SKIP.
        assert!(diags_with(&in_method("System.out.println(mystery);"), &resolver_unknown_super()).is_empty());
    }

    #[test]
    fn keywords_are_not_flagged() {
        // `this`/`true`/`null` aren't `identifier` nodes; ensure nothing is produced.
        assert!(diags("Object a = this; boolean b = true; Object c = null;").is_empty());
    }

    #[test]
    fn enum_constant_is_resolved() {
        // A bare enum constant `RED` inside the enum's own method is legal.
        let src = "package com.acme;\nenum Color { RED, GREEN; Color pick() { return RED; } }";
        // The enum type `Color` isn't in the resolver map → hierarchy not fully known → SKIP anyway,
        // which is also a safe (silent) outcome. Add Color to a resolver to exercise the constant path:
        let mut r = resolver();
        r.members.insert(
            "com/acme/Color".to_string(),
            ClassMembers {
                type_params: Vec::new(),
                superclass: Some("java/lang/Object".to_string()),
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
            },
        );
        r.simple.insert("Color".to_string(), "com/acme/Color".to_string());
        assert!(diags_with(src, &r).is_empty(), "{:?}", diags_with(src, &r));
    }

    #[test]
    fn two_top_level_types_skip_the_file() {
        // Ambiguous ownership → produce nothing.
        let src = "package com.acme;\nclass C extends Base { int count; void m() { System.out.println(mystery); } }\nclass D {}";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn parse_error_skips_the_file() {
        // A broken buffer → `has_error` → skip.
        let src = "package com.acme;\nclass C extends Base { int count; void m() { int x = ; System.out.println(mystery); } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

    #[test]
    fn qualifier_head_that_is_a_local_is_resolved() {
        // `sb.append(...)` — `sb` is a resolved local; the head must not be flagged.
        assert!(diags("String sb = \"\"; int n = sb.length();").is_empty());
    }
}
