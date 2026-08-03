//! Enum **switch-expression** exhaustiveness — a resolver-backed check.
//!
//! A `switch` used as an EXPRESSION (its result is required as a value) whose selector is an enum
//! type must be exhaustive: it needs a `default`, or a `case` for every constant of the enum. When it
//! has neither, the compiler rejects it. This is the ORTHOGONAL sibling of the yield check in
//! [`crate::switches`] (which verifies each ARM produces a value); here we verify the SET of arms
//! covers the enum.
//!
//! ## Never a false positive (the paramount rule)
//! Every guard below exists to make sure an emitted diagnostic is *certainly* a compile error. A
//! diagnostic fires ONLY when all of these hold — otherwise we stay silent:
//!   * the `switch` is unmistakably an **expression** (value position), NOT a statement — a statement
//!     `switch` missing enum cases is perfectly legal Java, so we never touch one;
//!   * the selector's type **fully resolves** (via the shared inference) to a type the resolver knows;
//!   * that type's `flags.is_enum` is **true** — a non-enum (int / String / class) is skipped;
//!   * the enum's constants are **completely enumerable** — we identify constants as the static fields
//!     whose declared type is the enum itself (how enum constants are represented in the member model).
//!     If that yields an empty set we bail (an enum has ≥1 constant, so an empty set means our view of
//!     the members is incomplete — a partial list would fabricate a "missing case");
//!   * there is **no `default`** clause (a `default` makes any switch exhaustive);
//!   * at least one constant is **named by no case label**.
//!
//! Under-reporting (e.g. a selector we can't infer, an enum whose members we can't see) is fine;
//! a wrong diagnostic is not.

use bennu_java::prelude::{
    infer_node_type_cached, ClassMembers, FileSymbols, InferCache, TypeResolver,
};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag enum switch **expressions** that neither cover every constant nor carry a `default`.
///
/// Resolver-backed signature (mirrors [`crate::casts::type_compat_errors_in`]): the caller's already
/// parsed `root` + shared `nodes` slice + `symbols` + inference `cache`, so this reuses the one parse,
/// one traversal and one memoized inference the aggregator sets up.
pub fn enum_switch_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        // A statement `switch` and an expression `switch` share the `switch_expression` grammar kind;
        // `is_value_context` (below, mirroring `switches.rs`) is what tells them apart. Skip a broken
        // subtree — a parse error there makes the arm/label reads unreliable.
        if n.kind() == "switch_expression" && !n.has_error() {
            check_switch(n, &root, source, bytes, symbols, resolver, cache, &mut out);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn check_switch(
    switch: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    // GUARD 1 — expression, not statement. Only a `switch` whose value is required is exhaustiveness-
    // checked; a statement `switch` missing enum cases is legal Java. Same predicate as
    // `switches::is_value_context` (kept identical, mirrored here because that fn is module-private and
    // this file must not modify `switches.rs`).
    if !crate::switches::is_value_context(switch) {
        return;
    }
    let Some(cond) = switch.child_by_field_name("condition") else { return };
    let Some(body) = switch.child_by_field_name("body") else { return };

    // GUARD 2 — the selector type must fully resolve. `infer_node_type_cached` returns `None` when it
    // can't type the selector expression → we can't know it's an enum → skip.
    let Some(sel_ty) = infer_node_type_cached(root, source, symbols, &cond, resolver, cache) else {
        return;
    };
    if sel_ty.binary_name.is_empty() {
        return;
    }
    // GUARD 3 — the resolved type must be a KNOWN enum. `members_of` = None (unknown type) or a type
    // whose `flags.is_enum` is false → skip.
    let Some(members) = resolver.members_of(&sel_ty.binary_name) else { return };
    if !members.flags.is_enum {
        return;
    }

    // GUARD 4 — the constants must be COMPLETELY enumerable. An empty set means our view of the enum's
    // members is incomplete (every enum has ≥1 constant), and a partial list would invent a false
    // "missing case", so we bail.
    let constants = enum_constants(&members, &sel_ty.binary_name);
    if constants.is_empty() {
        return;
    }

    // GUARD 5 — a `default` makes the switch exhaustive regardless of which cases are present.
    let mut covered: Vec<&str> = Vec::new();
    let mut has_default = false;
    let mut c = body.walk();
    for arm in body.named_children(&mut c) {
        // `case X ->` (arrow rule) and `case X:` (colon group) both carry their labels as
        // `switch_label` children; collect the constant names each names, and note any `default`.
        match arm.kind() {
            "switch_rule" | "switch_block_statement_group" => {
                collect_labels(arm, bytes, &mut covered, &mut has_default);
            }
            _ => {}
        }
    }
    if has_default {
        return;
    }

    // GUARD 6 — flag only the constants no case label names. `covered` holds bare identifiers taken
    // from the labels (enum switch labels are unqualified constant names), so a direct name compare is
    // exactly the JLS rule. If nothing is missing, the switch is exhaustive → silent.
    let missing: Vec<&str> = constants
        .iter()
        .filter(|con| !covered.contains(&con.as_str()))
        .map(String::as_str)
        .collect();
    if missing.is_empty() {
        return;
    }

    out.push(crate::check_id::CheckId::NonExhaustiveEnumSwitch.at(
        switch,
        format!(
            "Switch expression does not cover all enum constants (missing: {}) \
             — add the missing cases or a `default`",
            missing.join(", ")
        ),
    ));
}

/// The enum's constant names: the static fields whose declared type is the enum itself. That is how
/// an enum constant is represented in the member model (`enum E { A, B }` → static `E A`, static
/// `E B`), so this both *finds* the constants and *excludes* ordinary static fields of a different
/// type. Order follows declaration order in `fields`, which is the order we want in the message.
fn enum_constants(members: &ClassMembers, enum_binary: &str) -> Vec<String> {
    members
        .fields
        .iter()
        .filter(|f| f.is_static && f.return_type.binary_name == enum_binary)
        .map(|f| f.name.clone())
        .collect()
}

/// Walk an arm's `switch_label`s, appending each named enum constant to `covered` and flipping
/// `has_default` when a `default` label is present.
///
/// A case label names a constant in one of two shapes, and BOTH are the same constant:
///   * bare — `case A`, an `identifier` child of the label (the classic form);
///   * **qualified** — `case Status.A`, which the grammar gives us as a `field_access`. Java 21
///     accepts it in a pattern switch, and it is what you write when the constant reads better
///     with its type. Reading only the bare form is why a switch that covered every constant was
///     reported as covering none of them: every label was invisible to this check, and the
///     "missing" list was the whole enum.
///
/// A `default` label carries no named expression child (only the anonymous `default` keyword), so
/// it is detected by scanning the label's anonymous children. Any other label shape is simply not
/// added to `covered`, which can only make us *under*-report — never over-report.
fn collect_labels<'a>(
    arm: Node,
    bytes: &'a [u8],
    covered: &mut Vec<&'a str>,
    has_default: &mut bool,
) {
    let mut ac = arm.walk();
    for ch in arm.named_children(&mut ac) {
        if ch.kind() != "switch_label" {
            continue;
        }
        if label_is_default(ch, bytes) {
            *has_default = true;
        }
        let mut lc = ch.walk();
        for lch in ch.named_children(&mut lc) {
            if let Some(name) = label_constant(lch, bytes) {
                covered.push(name);
            }
        }
    }
}

/// The constant a single label expression names: itself when bare, its last segment when
/// qualified (`Status.A` → `A`). `None` for anything else — a guard, a pattern, a literal.
///
/// The qualifier is deliberately NOT matched against the selector's type. A label qualified with
/// the wrong type does not compile at all, so treating it as covering the constant it names costs
/// us nothing; refusing it, on the other hand, would report the constant as missing — the exact
/// false positive this check promises never to produce.
fn label_constant<'a>(label: Node, bytes: &'a [u8]) -> Option<&'a str> {
    match label.kind() {
        "identifier" => label.utf8_text(bytes).ok(),
        "field_access" => {
            let field = label.child_by_field_name("field")?;
            (field.kind() == "identifier").then(|| field.utf8_text(bytes).ok()).flatten()
        }
        _ => None,
    }
}

/// Whether a `switch_label` is the `default` clause. The `default` keyword is an anonymous (unnamed)
/// child of the label, so we scan the label's children (including anonymous ones) for its text.
fn label_is_default(label: Node, bytes: &[u8]) -> bool {
    let mut c = label.walk();
    for ch in label.children(&mut c) {
        if !ch.is_named() && ch.utf8_text(bytes) == Ok("default") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{
        extract_symbols, ClassFlags, ClassMembers, Import, Member, TypeRef,
    };
    use std::collections::HashMap;
    use std::sync::Arc;
    use tree_sitter::Parser;

    /// The same fixed resolver shape the other resolver-backed checks' tests use: a `binary → members`
    /// map + a `simple → binary` name table.
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

    /// One enum constant: a static field of the enum's own type (how a constant is modelled).
    fn constant(name: &str, enum_binary: &str) -> Member {
        Member::field(name, TypeRef::simple(enum_binary.to_string())).stat()
    }

    fn enum_cls(binary: &str, constants: &[&str]) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: Some("java/lang/Enum".to_string()),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: constants.iter().map(|c| constant(c, binary)).collect(),
            flags: ClassFlags { is_enum: true, is_final: true, ..ClassFlags::default() },
        }
    }

    fn plain_cls() -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    /// `enum Color { RED, GREEN, BLUE }`; a plain class `Widget`; `java/lang/String` (non-enum).
    /// A `Holder` with a `color()` getter → `Color`, so a selector can be typed by inference.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), plain_cls());
        members.insert(
            "com/acme/Color".to_string(),
            enum_cls("com/acme/Color", &["RED", "GREEN", "BLUE"]),
        );
        members.insert("com/acme/Widget".to_string(), plain_cls());
        members.insert("java/lang/String".to_string(), plain_cls());
        let simple = [
            ("Color", "com/acme/Color"),
            ("Widget", "com/acme/Widget"),
            ("String", "java/lang/String"),
            ("Object", "java/lang/Object"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// Run the check over a method body and return the messages. `c` is a `Color` field, so
    /// `switch (c)` has an inferable enum selector.
    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{ Color c; int m() {{ {body} }} }}");
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(&src, None).unwrap();
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = extract_symbols(&src);
        enum_switch_errors_in(root, &nodes, &src, &symbols, &resolver(), &InferCache::new())
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn expression_missing_constant_no_default_is_flagged() {
        // `switch (c)` used as an initializer, covers RED/GREEN but not BLUE, no default → error.
        let d = diags("int x = switch (c) { case RED -> 1; case GREEN -> 2; }; return x;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("missing: BLUE"), "{d:?}");
        assert!(d[0].contains("does not cover all enum constants"), "{d:?}");
    }

    #[test]
    fn expression_missing_two_constants_lists_both() {
        let d = diags("int x = switch (c) { case RED -> 1; }; return x;");
        assert_eq!(d.len(), 1, "{d:?}");
        // Declaration order preserved (GREEN before BLUE).
        assert!(d[0].contains("missing: GREEN, BLUE"), "{d:?}");
    }

    #[test]
    fn expression_with_default_is_ok() {
        // A `default` makes it exhaustive even though BLUE has no explicit case.
        assert!(diags(
            "int x = switch (c) { case RED -> 1; case GREEN -> 2; default -> 0; }; return x;"
        )
        .is_empty());
    }

    #[test]
    fn expression_covering_all_constants_is_ok() {
        assert!(diags(
            "int x = switch (c) { case RED -> 1; case GREEN -> 2; case BLUE -> 3; }; return x;"
        )
        .is_empty());
    }

    /// The reported bug: qualifying the constants (`case Color.RED`) made every label invisible,
    /// so a switch that covers the enum completely was reported as covering none of it.
    #[test]
    fn qualified_case_labels_count_as_covered() {
        assert!(
            diags(
                "int x = switch (c) { case Color.RED -> 1; case Color.GREEN -> 2; case Color.BLUE -> 3; }; return x;"
            )
            .is_empty(),
            "a qualified label names the same constant a bare one does",
        );
    }

    /// …and a qualified switch that really is missing one still says so, naming only that one.
    #[test]
    fn qualified_labels_still_report_a_genuinely_missing_constant() {
        let d = diags("int x = switch (c) { case Color.RED -> 1; case Color.GREEN -> 2; }; return x;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("missing: BLUE"), "{d:?}");
    }

    #[test]
    fn colon_style_expression_is_checked_too() {
        // Colon-group arms (`case X:` + `yield`) are the other expression form; still covered.
        let d = diags(
            "int x = switch (c) { case RED: yield 1; case GREEN: yield 2; }; return x;",
        );
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("missing: BLUE"), "{d:?}");
    }

    #[test]
    fn statement_switch_is_not_checked() {
        // A statement `switch` (no value position) missing a constant is legal Java → never flagged.
        assert!(diags("switch (c) { case RED -> {} case GREEN -> {} } return 0;").is_empty());
    }

    #[test]
    fn non_enum_selector_is_not_checked() {
        // An `int` / `String` selector isn't an enum → skipped (this would also not resolve to an
        // enum type). Use a String local so inference yields String (non-enum).
        assert!(diags(
            "String s = \"x\"; int x = switch (s) { case \"a\" -> 1; }; return x;"
        )
        .is_empty());
    }

    #[test]
    fn unresolvable_selector_is_not_checked() {
        // A selector whose type inference can't resolve → silent (no enum knowledge).
        let src =
            "class C { int m() { int x = switch (mystery()) { case RED -> 1; }; return x; } }";
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = extract_symbols(src);
        let out = enum_switch_errors_in(
            root,
            &nodes,
            src,
            &symbols,
            &resolver(),
            &InferCache::new(),
        );
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn enum_with_no_visible_constants_is_not_checked() {
        // If the resolver's view of the enum has no constants (incomplete members), we can't build a
        // complete constant set → bail rather than invent a "missing case".
        let mut r = resolver();
        r.members.insert(
            "com/acme/Empty".to_string(),
            enum_cls("com/acme/Empty", &[]), // enum flag set, but zero constants visible
        );
        r.simple.insert("Empty".to_string(), "com/acme/Empty".to_string());
        let src = "class C { Empty e; int m() { int x = switch (e) { case RED -> 1; }; return x; } }";
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = extract_symbols(src);
        let out = enum_switch_errors_in(root, &nodes, src, &symbols, &r, &InferCache::new());
        assert!(out.is_empty(), "{out:?}");
    }
}
