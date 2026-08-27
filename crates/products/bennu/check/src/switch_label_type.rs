//! `case` label **type** compatibility — a resolver-backed check.
//!
//! A `case` label has to be assignable to the selector. The one that bites hardest in practice is an
//! enum selector with numeric labels:
//!
//! ```text
//! switch (format) {   // format is an enum
//!     case 1: …       // javac: an enum switch case label must be the unqualified name of an
//!     case 2: …       //        enumeration constant
//! }
//! ```
//!
//! It compiles nowhere and yet reads as ordinary code, so it survives review; it usually arrives
//! from a refactor that turned an `int` constant into an enum and left the switches behind.
//!
//! This is the third of the switch checks and it is orthogonal to the other two:
//! [`crate::switch_dup`] compares labels to *each other*, [`crate::enum_switch`] compares the *set*
//! of labels to the enum's constants, and this one compares *each label* to the **selector's type**.
//!
//! ## Never a false positive (the paramount rule)
//!
//! A diagnostic fires only where the label is wrong for *every* reading of the program:
//!
//!   * the selector's type must **fully resolve** through the shared inference to a type the
//!     resolver knows — an un-inferable selector is skipped entirely;
//!   * **pattern labels are skipped whole** ([`crate::switch_label::label_is_pattern`]). A `when`
//!     guard is a *sibling* of its pattern in the grammar, so a bare identifier guard sits exactly
//!     where a case constant sits; reading one as a constant would flag legal Java 21;
//!   * `default` and `case null` are skipped — `null` is a legal label for any reference selector;
//!   * only the shapes we can decide are judged. A literal is decidable from the AST alone; a bare
//!     name against an enum's constant set is decidable *when the set is complete*. Everything else
//!     — a constant expression (`case A + 1`), a qualified name we can't attribute, an unresolved
//!     selector — is left alone;
//!   * the "not a constant of this enum" arm additionally requires a **non-empty** constant set, the
//!     same guard [`crate::enum_switch`] uses: an enum has ≥1 constant, so an empty set means our
//!     view of its members is incomplete, and judging names against a partial list would invent an
//!     error.
//!
//! Under-reporting is the intended failure mode.

use bennu_java::prelude::{infer_node_type_cached, FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::nodes::{simple_name};

use crate::switch_label::{label_is_default, label_is_pattern, labels_of};

/// The boxed selector types whose labels must be integral — a `String` label on one of these is an
/// error. (The *primitive* `int`/`short`/… selectors are not listed: Bennu's inference doesn't model
/// primitives, so they never reach this check.)
const INTEGRAL_BOXES: [&str; 4] =
    ["java/lang/Integer", "java/lang/Short", "java/lang/Byte", "java/lang/Character"];

/// What a label literal *is*, as far as switch compatibility cares.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lit {
    Int,
    Float,
    Char,
    Bool,
    Str,
}

impl Lit {
    /// How the message names it.
    const fn describe(self) -> &'static str {
        match self {
            Lit::Int => "an integer literal",
            Lit::Float => "a floating-point literal",
            Lit::Char => "a character literal",
            Lit::Bool => "a boolean literal",
            Lit::Str => "a string literal",
        }
    }
}

/// Flag `case` labels whose type can't match the `switch` selector's.
///
/// Resolver-backed signature (mirrors [`crate::enum_switch::enum_switch_errors_in`]): the caller's
/// already-parsed `root` + shared `nodes` slice + `symbols` + inference `cache`, so this reuses the
/// one parse, one traversal and one memoized inference the aggregator sets up.
pub fn switch_label_type_errors_in(
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
        // Statement and expression `switch` share the grammar kind, and BOTH are checked: unlike
        // exhaustiveness (legal to omit in a statement switch), a mistyped label is an error in
        // either position. A broken subtree is skipped — the label reads would be unreliable.
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
    let Some(cond) = switch.child_by_field_name("condition") else { return };
    let Some(body) = switch.child_by_field_name("body") else { return };

    // The selector type must fully resolve, or we have nothing to compare labels against.
    let Some(sel) = infer_node_type_cached(root, source, symbols, &cond, resolver, cache) else {
        return;
    };
    if sel.binary_name.is_empty() {
        return;
    }
    let Some(members) = resolver.members_of(&sel.binary_name) else { return };
    let simple = simple_name(&sel.binary_name);

    // For an enum selector the constant set doubles as the legal label vocabulary — but only when it
    // is complete (see the module doc). Empty ⇒ judge literals only, never names.
    let constants: Vec<String> = if members.flags.is_enum {
        crate::enum_switch::enum_constants(&members, &sel.binary_name)
    } else {
        Vec::new()
    };

    for label in labels_of(body) {
        if label_is_default(label, bytes) || label_is_pattern(label) {
            continue;
        }
        let mut lc = label.walk();
        for cst in label.named_children(&mut lc) {
            if members.flags.is_enum {
                check_enum_label(cst, bytes, simple, &constants, out);
            } else {
                check_scalar_label(cst, bytes, &sel.binary_name, simple, out);
            }
        }
    }
}

/// One label of an **enum** switch. Two ways it can be wrong, both hard errors: a literal (never
/// legal — the JLS wants the unqualified name of a constant), or a name that is not one of this
/// enum's constants.
fn check_enum_label(
    cst: Node,
    bytes: &[u8],
    enum_simple: &str,
    constants: &[String],
    out: &mut Vec<Diagnostic>,
) {
    let Ok(text) = cst.utf8_text(bytes) else { return };

    if let Some(lit) = literal_kind(cst) {
        out.push(crate::check_id::CheckId::IncompatibleCaseLabel.at(
            cst,
            format!(
                "`case {}` is {} — an enum `switch` label must be the unqualified name of a \
                 constant of `{}`",
                crate::text::short(text.trim()),
                lit.describe(),
                enum_simple
            ),
        ));
        return;
    }

    // A name, judged against the enum's vocabulary — only when we can see all of it.
    if constants.is_empty() {
        return;
    }
    let Some(name) = label_name(cst, bytes) else { return };
    if !constants.iter().any(|c| c == name) {
        out.push(crate::check_id::CheckId::UnknownEnumCaseLabel.at(
            cst,
            format!("`{name}` is not a constant of enum `{enum_simple}`"),
        ));
    }
}

/// One label of a **non-enum** switch: only the clear-cut literal mismatches against the two selector
/// families we can resolve — `String`, and the integral boxes.
fn check_scalar_label(cst: Node, bytes: &[u8], binary: &str, simple: &str, out: &mut Vec<Diagnostic>) {
    let Some(lit) = literal_kind(cst) else { return };
    let wrong = if binary == "java/lang/String" {
        lit != Lit::Str
    } else if INTEGRAL_BOXES.contains(&binary) {
        // `case 'a'` on an `Integer` is a legal widening; only a string is certainly wrong.
        lit == Lit::Str
    } else {
        false
    };
    if !wrong {
        return;
    }
    let Ok(text) = cst.utf8_text(bytes) else { return };
    out.push(crate::check_id::CheckId::IncompatibleCaseLabel.at(
        cst,
        format!(
            "`case {}` is {}, which can't match a `{}` selector",
            crate::text::short(text.trim()),
            lit.describe(),
            simple
        ),
    ));
}

/// The literal family of a label expression, or `None` when it isn't a literal.
///
/// `null` is deliberately NOT a literal here: `case null` is legal for any reference selector, so it
/// must reach neither arm. A unary sign is unwrapped (`case -1` is an integer literal), because that
/// is the shape a negative constant actually parses into.
fn literal_kind(node: Node) -> Option<Lit> {
    let inner = if node.kind() == "unary_expression" {
        node.child_by_field_name("operand")?
    } else {
        node
    };
    match inner.kind() {
        "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal"
        | "binary_integer_literal" => Some(Lit::Int),
        "decimal_floating_point_literal" | "hex_floating_point_literal" => Some(Lit::Float),
        "character_literal" => Some(Lit::Char),
        "true" | "false" => Some(Lit::Bool),
        "string_literal" | "text_block" => Some(Lit::Str),
        _ => None,
    }
}

/// The constant a name-shaped label denotes: itself when bare (`case A`), its last segment when
/// qualified (`case Status.A` → `A`). `None` for any other shape — an expression, a call, a cast —
/// which is how those stay unjudged.
fn label_name<'a>(node: Node, bytes: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(bytes).ok(),
        "field_access" => {
            let field = node.child_by_field_name("field")?;
            (field.kind() == "identifier").then(|| field.utf8_text(bytes).ok()).flatten()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{extract_symbols, ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tree_sitter::Parser;

    /// The same fixed resolver shape the other resolver-backed checks' tests use.
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

    /// `enum Fmt { CSV, TSV }`, plus `String` and `Integer` as plain classes.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), plain_cls());
        members.insert("java/lang/String".to_string(), plain_cls());
        members.insert("java/lang/Integer".to_string(), plain_cls());
        members.insert("com/acme/Fmt".to_string(), enum_cls("com/acme/Fmt", &["CSV", "TSV"]));
        let simple = [
            ("Fmt", "com/acme/Fmt"),
            ("String", "java/lang/String"),
            ("Integer", "java/lang/Integer"),
            ("Object", "java/lang/Object"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// Run the check over a method body. `f` is an `Fmt` field, `s` a `String` and `i` an `Integer`,
    /// so every selector below is inferable.
    fn diags(body: &str) -> Vec<String> {
        let src =
            format!("class C {{ Fmt f; String s; Integer i; boolean flag; void m() {{ {body} }} }}");
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(&src, None).unwrap();
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = extract_symbols(&src);
        switch_label_type_errors_in(root, &nodes, &src, &symbols, &resolver(), &InferCache::new())
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    /// The reported case: an enum selector with integer labels. Compiled nowhere, reported nowhere.
    #[test]
    fn integer_labels_on_an_enum_selector_are_errors() {
        let d = diags("switch (f) { case 1: break; case 2: break; }");
        assert_eq!(d.len(), 2, "{d:?}");
        assert!(d[0].contains("an integer literal"), "{d:?}");
        assert!(d[0].contains("`Fmt`"), "{d:?}");
    }

    #[test]
    fn a_string_label_on_an_enum_selector_is_an_error() {
        let d = diags("switch (f) { case \"CSV\": break; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("a string literal"), "{d:?}");
    }

    #[test]
    fn real_constants_are_silent() {
        let d = diags("switch (f) { case CSV: break; case Fmt.TSV: break; }");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn a_name_that_is_not_a_constant_is_flagged() {
        let d = diags("switch (f) { case JSON: break; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`JSON` is not a constant"), "{d:?}");
    }

    /// `case null` is legal Java 21 for any reference selector — including an enum.
    #[test]
    fn a_null_label_is_silent() {
        let d = diags("switch (f) { case null: break; default: break; }");
        assert!(d.is_empty(), "{d:?}");
    }

    /// A `when` guard is a *sibling* of its pattern, so a bare-identifier guard sits exactly where a
    /// case constant sits. Reading it as one would flag legal code.
    #[test]
    fn a_guarded_pattern_label_is_left_alone() {
        // Valid Java 21: the guarded pattern is never total, the unguarded one is — so no `default`.
        let d = diags("switch (f) { case Fmt g when flag -> {} case Fmt g2 -> {} }");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn an_integer_label_on_a_string_selector_is_an_error() {
        let d = diags("switch (s) { case 1: break; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`String` selector"), "{d:?}");
    }

    #[test]
    fn string_labels_on_a_string_selector_are_silent() {
        let d = diags("switch (s) { case \"a\": break; case \"b\": break; }");
        assert!(d.is_empty(), "{d:?}");
    }

    /// A character label widens to an `Integer` selector — legal, and must stay silent.
    #[test]
    fn a_char_label_on_an_integral_box_is_silent() {
        let d = diags("switch (i) { case 'a': break; case 1: break; }");
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn a_string_label_on_an_integral_box_is_an_error() {
        let d = diags("switch (i) { case \"a\": break; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`Integer` selector"), "{d:?}");
    }

    /// An unresolvable selector is skipped entirely rather than guessed at.
    #[test]
    fn an_unknown_selector_type_is_silent() {
        let src = "class C { Mystery x; void m() { switch (x) { case 1: break; } } }";
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let root = tree.root_node();
        let nodes = crate::check::collect_nodes(root);
        let symbols = extract_symbols(src);
        let d = switch_label_type_errors_in(
            root,
            &nodes,
            src,
            &symbols,
            &resolver(),
            &InferCache::new(),
        );
        assert!(d.is_empty(), "{d:?}");
    }

    /// A nested switch's labels belong to the nested switch's own selector, not the outer one.
    #[test]
    fn a_nested_switch_is_judged_against_its_own_selector() {
        let d = diags("switch (f) { case CSV: switch (s) { case \"a\": break; } break; }");
        assert!(d.is_empty(), "{d:?}");
    }
}
