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

use crate::scopes::{
    is_value_position, resolves_as_local, scope_is_directly_top, single_top_level_type,
};

use crate::nodes::has_generated_members;
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

    // A member-generating annotation means the type's real member list is bigger than anything we
    // can read: under Lombok's `@Slf4j` the bare `log` is a legal field reference declared in no
    // source file, and `@Data`'s accessors are the same story for the call check next door. Flagging
    // those is the fastest way to make someone close the Problems panel. See
    // `crate::nodes::has_generated_members`.
    if has_generated_members(top.node, bytes) {
        return Vec::new();
    }

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
        if !(is_value_position(n) && scope_is_directly_top(n, top.node)) {
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
                superclass: Some(TypeRef::simple("java/lang/Object")),
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
                superclass: Some(TypeRef::simple("com/acme/Base")),
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
                superclass: Some(TypeRef::simple("java/lang/Object")),
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
                superclass: Some(TypeRef::simple("java/lang/Object")),
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
    fn a_branch_label_is_not_a_variable() {
        // `outer` names a LABEL, which lives in its own namespace (JLS §6.5.1). Judging it as a bare
        // value made every labelled loop in a project light up with "cannot resolve symbol" —
        // twelve of them in Guava alone. Whether the label exists is `branches.rs`'s question.
        assert!(diags(
            "outer: for (int i = 0; i < 3; i++) { for (int j = 0; j < 3; j++) { if (j == 1) continue outer; if (i == 2) break outer; } }"
        )
        .is_empty());
    }

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

    /// Lombok's `@Slf4j` injects a `log` field that exists in no source file. Reporting it was a
    /// page of red on a class that compiles — see `crate::nodes::has_generated_members`.
    #[test]
    fn a_member_generating_annotation_skips_the_file() {
        let src = "package com.acme;\n@Slf4j\nclass C extends Base { void m() { Object o = log; } }";
        assert!(diags_with(src, &resolver()).is_empty());
    }

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
                superclass: Some(TypeRef::simple("java/lang/Object")),
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
