//! Lossy-primitive-narrowing diagnostics — assigning a WIDER primitive to a NARROWER one **without an
//! explicit cast** is a compile error in Java ("incompatible types: possible lossy conversion from
//! `long` to `int`"). Three sites, mirroring `casts.rs`:
//!
//!   * **declarator** — `int x = aLong;` / a field `int f = aLong;` (target = the declaration's `type`);
//!   * **assignment** — `x = aLong;` where `x` is `int` (target = the inferred type of the `left`);
//!   * **return** — `return aLong;` in a method whose return type is a narrower primitive.
//!
//! PARAMOUNT: never a false positive. We fire ONLY when BOTH the target and the inferred source are
//! primitives we recognise AND the source is strictly wider than the target per the width table below.
//! Everything uncertain (a class, a generic, an unknown type, a constant that MIGHT fit, an explicit
//! cast) is skipped.
//!
//! ## Width / lossy table
//!
//! Widening (narrower → wider), the JLS numeric-promotion order:
//!
//!   `byte` < `short` < `int` < `long` < `float` < `double`
//!
//! plus `char`, which widens to `int`/`long`/`float`/`double` but has NO implicit relation to
//! `byte`/`short` (a `char` is unsigned 16-bit, a `byte`/`short` signed — Java requires a cast either
//! way). We model `char` conservatively with its own rank so that:
//!   * `char` → `int`/`long`/`float`/`double` is widening (OK),
//!   * anything → `char` needs a cast (LOSSY),
//!   * `byte`/`short` ↔ `char` is LOSSY in either direction (a cast is required).
//!
//! A conversion is LOSSY (→ error) exactly when the source rank is STRICTLY GREATER than the target
//! rank on the same axis, e.g. `long`→`int`, `double`→`float`, `int`→`byte`, `int`→`char`,
//! `short`→`char`. Widening (`int`→`long`, `int`→`double`, `float`→`double`, `char`→`int`) is always
//! fine and never flagged.
//!
//! `boolean` participates in no numeric conversion: it never widens/narrows to any other primitive.
//! We give it no rank, so any pairing involving `boolean` returns "not comparable" → we SKIP (the
//! String↔primitive / boxing cases that would make `boolean = "x"` an error are owned by `casts.rs`,
//! not here — this file only speaks about numeric narrowing between two primitives).

use bennu_java::prelude::{infer_node_type_cached, FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::support::constant::{certainly_not_constant, constant_names, fold, Folded};

/// Parse-and-walk entry mirroring `casts::type_compat_errors_in`: iterate the shared `nodes` and flag
/// every lossy narrowing at a declarator, an assignment, or a return.
pub fn narrowing_errors_in(
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
        match n.kind() {
            "local_variable_declaration" | "field_declaration" => {
                check_declaration(n, &root, source, bytes, symbols, resolver, cache, &mut out)
            }
            "assignment_expression" => {
                check_assignment(n, &root, source, bytes, symbols, resolver, cache, &mut out)
            }
            "return_statement" => check_return(n, &root, source, bytes, symbols, resolver, cache, &mut out),
            _ => {}
        }
    }
    out
}

/// `T x = value;` / field `T f = value;` — target = the declaration's `type` text (mirrors
/// `casts::check_declaration`).
#[allow(clippy::too_many_arguments)]
fn check_declaration(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let Some(ty_node) = n.child_by_field_name("type") else { return };
    let Ok(type_text) = ty_node.utf8_text(bytes) else { return };
    // SKIP `var` — the target type is inferred FROM the initializer, so there is no declared narrower
    // type to violate.
    let Some(target) = primitive_rank(type_text) else { return };
    let mut c = n.walk();
    // GOTCHA (project rule): explicit `for`, never `.find`/`.any` on `named_children`.
    for d in n.named_children(&mut c) {
        if d.kind() != "variable_declarator" {
            continue;
        }
        let Some(val) = d.child_by_field_name("value") else { continue };
        narrowing_check(root, source, bytes, symbols, target, type_text, val, resolver, cache, out);
    }
}

/// `x = value;` where `x` is a primitive — target = inferred type of the `left` (mirrors how
/// `casts.rs` gets an assignment's target: infer `type(left)`).
#[allow(clippy::too_many_arguments)]
fn check_assignment(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    // SKIP compound assignments (`x += aLong`, `x >>= …`): Java inserts an IMPLICIT narrowing cast for
    // these, so they are NOT lossy-conversion errors. Only a plain `=` narrows without a cast.
    let Some(op) = n.child_by_field_name("operator") else { return };
    let Ok(op_text) = op.utf8_text(bytes) else { return };
    if op_text != "=" {
        return;
    }
    let Some(left) = n.child_by_field_name("left") else { return };
    let Some(right) = n.child_by_field_name("right") else { return };
    // Target = the static type of the assignee. SKIP unless inference yields a known primitive.
    let Some(left_ty) = infer_node_type_cached(root, source, symbols, &left, resolver, cache) else {
        return;
    };
    let Some(target) = primitive_rank(&left_ty.binary_name) else { return };
    narrowing_check(root, source, bytes, symbols, target, &left_ty.binary_name, right, resolver, cache, out);
}

/// `return value;` — target = the enclosing method's return type (mirrors `casts::check_return`).
#[allow(clippy::too_many_arguments)]
fn check_return(
    n: Node,
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let Some(val) = first_value_child(n) else { return };
    let Some(method) = enclosing_method(n) else { return };
    let Some(ret_text) = method.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()) else {
        return;
    };
    let Some(target) = primitive_rank(ret_text) else { return };
    narrowing_check(root, source, bytes, symbols, target, ret_text, val, resolver, cache, out);
}

/// The shared decision: flag `val` iff it narrows into a primitive of rank `target` lossily and isn't
/// exempt. `target_display` is the *written* target name for the message. Every SKIP is spelled out.
#[allow(clippy::too_many_arguments)]
fn narrowing_check(
    root: &Node,
    source: &str,
    bytes: &[u8],
    symbols: &FileSymbols,
    target: Rank,
    target_display: &str,
    val: Node,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    // SKIP an explicit cast (`(int) aLong`): the `cast_expression` wraps the value and makes the
    // conversion legal. This is the whole point of the check — a cast is the user's opt-out.
    if val.kind() == "cast_expression" {
        return;
    }
    // Infer the source expression's type. SKIP if inference yields nothing (unknown / null literal /
    // untyped arithmetic) — unknown is NOT an error.
    let Some(source_ty) = infer_node_type_cached(root, source, symbols, &val, resolver, cache) else {
        return;
    };
    // SKIP unless the source is a primitive we recognise (a class / generic / unknown → not our case).
    let Some(src) = primitive_rank(&source_ty.binary_name) else { return };

    // Not lossy → never flag: same rank, or a WIDENING (source narrower than target). `char` sits on
    // its own axis vs `byte`/`short`, so `lossy` returns `false` (not-comparable) for those and we skip.
    if !lossy(src, target) {
        return;
    }

    // Constant-fit exception: a compile-time integer literal that FITS in the narrower integral target
    // is LEGAL (`byte b = 100;`, `short s = 300;`, `char c = 65;`). Only flag a NON-constant source, or
    // a literal that DEFINITELY does not fit. If we can't decide, we SKIP (treat as fits) to stay sound.
    if constant_fits_or_uncertain(val, bytes, src, target) {
        return;
    }

    out.push(crate::engine::check_id::CheckId::LossyConversion.at(
        val,
        format!(
            "Incompatible types: possible lossy conversion from `{}` to `{}`",
            source_ty.binary_name, target_display
        ),
    ));
}

/// Numeric rank of a primitive. `char` gets a distinct axis marker so it never silently widens/narrows
/// with `byte`/`short`. `boolean`/`void`/anything else → `None` (no numeric conversion → we skip).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rank {
    /// A regular numeric primitive on the `byte<short<int<long<float<double` line.
    Num(u8),
    /// `char` — widens only UP to int/long/float/double; unrelated to byte/short.
    Char,
}

fn primitive_rank(text: &str) -> Option<Rank> {
    Some(match text.trim() {
        "byte" => Rank::Num(0),
        "short" => Rank::Num(1),
        "int" => Rank::Num(2),
        "long" => Rank::Num(3),
        "float" => Rank::Num(4),
        "double" => Rank::Num(5),
        "char" => Rank::Char,
        // `boolean`, `void`, or a non-primitive name → not part of numeric narrowing.
        _ => return None,
    })
}

/// Is `src → target` a LOSSY narrowing (→ error)? True only when we are CERTAIN it narrows:
///   * both on the numeric line and `src` strictly wider (`long`→`int`, `double`→`float`, `int`→`byte`);
///   * anything numeric → `char` (needs a cast: e.g. `int`→`char`, `short`→`char`);
///   * `char` → `byte`/`short` (narrowing off the char axis: `char`→`byte`).
/// Everything else (widening, `char`→int+, same type, `boolean`) is NOT lossy → `false` → we don't flag.
fn lossy(src: Rank, target: Rank) -> bool {
    match (src, target) {
        // Both numeric: lossy iff source strictly wider. `int`→`long` (2<3) is widening → false.
        (Rank::Num(s), Rank::Num(t)) => s > t,
        // Anything numeric → char always needs a cast. (`char`→char handled below as same-type.)
        (Rank::Num(s_num), Rank::Char) => {
            // byte(0)/short(1) → char: lossy. int(2)+ → char: also lossy (char is 16-bit unsigned).
            let _ = s_num;
            true
        }
        // char → a numeric target: lossy only when the target is BELOW `int` (byte/short); char widens
        // to int/long/float/double.
        (Rank::Char, Rank::Num(t)) => t < 2,
        // char → char: same type, not a conversion.
        (Rank::Char, Rank::Char) => false,
    }
}

/// The constant exception of JLS §5.2: a constant expression of type `byte`, `short`, `char` or
/// `int` narrows to `byte`, `short` or `char` without a cast when its value fits.
///
/// `true` (→ skip) when the value folds and fits, or when the source may be a constant this file
/// cannot evaluate (`OtherType.LIMIT`, an inherited field). `false` (→ report) for a value that
/// folds and does not fit, a source that is certainly not constant (a parameter, a call, a
/// non-`final` variable), or a `long`/`float`/`double` source, which the exception never covers.
fn constant_fits_or_uncertain(val: Node, bytes: &[u8], src: Rank, target: Rank) -> bool {
    if !matches!(src, Rank::Num(0..=2) | Rank::Char) {
        return false;
    }
    let (min, max): (i64, i64) = match target {
        Rank::Num(0) => (i8::MIN.into(), i8::MAX.into()),
        Rank::Num(1) => (i16::MIN.into(), i16::MAX.into()),
        Rank::Char => (0, u16::MAX.into()),
        _ => return false,
    };
    let names = constant_names(val, bytes);
    match fold(val, bytes, &names, 0) {
        Some(Folded::Int(v)) => v >= min && v <= max,
        Some(Folded::Str(_)) => false,
        None => !certainly_not_constant(val, bytes, &names, 0),
    }
}

/// The first non-comment named child of a `return_statement` (the returned value), or `None` for a
/// bare `return;`. (Same shape as `casts::first_value_child`.)
fn first_value_child(ret: Node) -> Option<Node> {
    let mut c = ret.walk();
    for n in ret.named_children(&mut c) {
        if !matches!(n.kind(), "line_comment" | "block_comment") {
            return Some(n);
        }
    }
    None
}

/// The nearest enclosing `method_declaration`, stopping at a `lambda_expression` (a `return` inside a
/// lambda targets the lambda, not the method — nothing to check). Mirrors `casts::enclosing_method`.
fn enclosing_method(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        match p.kind() {
            "lambda_expression" => return None,
            "method_declaration" => return Some(p),
            _ => cur = p.parent(),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{extract_symbols, ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tree_sitter::Parser;

    /// Same fixed resolver shape as `casts.rs`: a `binary → members` map + a `simple → binary` table.
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

    fn getter(name: &str, ret: &str) -> Member {
        Member::method(name, TypeRef::simple(ret.to_string()), Vec::new())
    }

    fn cls(superclass: Option<&str>, methods: Vec<Member>, fields: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(TypeRef::simple),
            interfaces: Vec::new(),
            methods,
            fields,
            flags: ClassFlags::default(),
        }
    }

    /// A `Provider` whose getters return each primitive we need to seed a source expression, plus an
    /// `Unknown`-returning getter and an unrelated class for the "unknown-typed source" negative.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cls(None, vec![], vec![]));
        members.insert(
            "com/acme/Provider".to_string(),
            cls(
                Some("java/lang/Object"),
                vec![
                    getter("aLong", "long"),
                    getter("anInt", "int"),
                    getter("aDouble", "double"),
                    getter("aFloat", "float"),
                    getter("aByte", "byte"),
                    getter("mystery", "com/acme/Unknown"),
                ],
                vec![Member::field("count", TypeRef::simple("int".to_string()))],
            ),
        );
        let simple = [("Object", "java/lang/Object"), ("Provider", "com/acme/Provider")]
            .into_iter()
            .map(|(s, b)| (s.to_string(), b.to_string()))
            .collect();
        MapResolver { members, simple }
    }

    /// Run the check over a method body; `Provider p` is in scope as a field for seeding sources.
    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{ Provider p; void m() {{ {body} }} }}");
        run(&src)
    }

    fn run(src: &str) -> Vec<String> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let symbols = extract_symbols(src);
        let root = tree.root_node();
        let nodes = crate::engine::check::collect_nodes(root);
        let cache = InferCache::new();
        narrowing_errors_in(root, &nodes, src, &symbols, &resolver(), &cache)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    // ── Positives ──────────────────────────────────────────────────────────────

    #[test]
    fn long_local_to_int_is_flagged() {
        // The source is a primitive LOCAL, not a call. Its declared type used to infer to nothing —
        // `parse_type_text` answers `None` for a primitive because it has no members — so the whole
        // check was blind on the shape Java is mostly written in.
        let d = diags("long l = 5L; int x = l;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`long`") && d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn long_parameter_to_int_is_flagged() {
        let d = run("class C { void m(long l) { int x = l; } }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn double_to_float_is_flagged() {
        // A `float` target used to fall out of the constant-fit guard as "uncertain → skip" before
        // anything asked whether there was a constant at all, which swallowed every narrowing INTO
        // a float or a long.
        let d = run("class C { void m(double d) { float f = d; } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`double`") && d[0].contains("`float`"), "{d:?}");
    }

    #[test]
    fn double_to_long_is_flagged() {
        assert_eq!(run("class C { void m(double d) { long l = d; } }").len(), 1);
    }

    #[test]
    fn int_local_to_long_is_ok() {
        assert!(diags("int i = 5; long l = i;").is_empty());
    }

    #[test]
    fn float_literal_to_float_is_ok() {
        assert!(run("class C { void m() { float f = 1.0f; } }").is_empty());
    }

    #[test]
    fn long_to_float_is_ok() {
        // `long` → `float` is a widening primitive conversion (JLS §5.1.2), lossy in precision but
        // legal without a cast — the rank table must not report it.
        assert!(run("class C { void m(long l) { float f = l; } }").is_empty());
    }

    #[test]
    fn long_to_int_declarator_is_flagged() {
        let d = diags("int x = p.aLong();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`long`") && d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn int_to_byte_declarator_is_flagged() {
        let d = diags("byte b = p.anInt();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`int`") && d[0].contains("`byte`"), "{d:?}");
    }

    #[test]
    fn double_to_int_declarator_is_flagged() {
        let d = diags("int i = p.aDouble();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`double`") && d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn out_of_range_byte_literal_is_flagged() {
        // 300 does NOT fit a byte (-128..127) → a real error even as a constant.
        let d = diags("byte b = 300;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`int`") && d[0].contains("`byte`"), "{d:?}");
    }

    #[test]
    fn long_to_int_assignment_is_flagged() {
        // Assignment target typed via the resolver (an `int` field) → long→int is lossy.
        let d = diags("p.count = p.aLong();");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`long`") && d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn narrowing_return_is_flagged() {
        let src = "class C { Provider p; int m() { return p.aLong(); } }";
        let d = run(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`long`") && d[0].contains("`int`"), "{d:?}");
    }

    // ── Negatives (must NEVER flag) ─────────────────────────────────────────────

    #[test]
    fn widening_int_to_long_is_ok() {
        assert!(diags("long x = p.anInt();").is_empty());
    }

    #[test]
    fn explicit_cast_is_ok() {
        assert!(diags("int x = (int) p.aLong();").is_empty());
    }

    #[test]
    fn in_range_byte_constant_is_ok() {
        assert!(diags("byte b = 100;").is_empty());
    }

    #[test]
    fn plain_int_literal_to_int_is_ok() {
        assert!(diags("int x = 5;").is_empty());
    }

    #[test]
    fn widening_float_to_double_is_ok() {
        assert!(diags("double d = p.aFloat();").is_empty());
    }

    #[test]
    fn unknown_typed_source_is_not_flagged() {
        // `p.mystery()` returns an unresolved class → inference yields a non-primitive → SKIP.
        assert!(diags("int x = p.mystery();").is_empty());
    }

    #[test]
    fn short_constant_that_fits_is_ok() {
        assert!(diags("short s = 300;").is_empty());
    }

    #[test]
    fn char_constant_that_fits_is_ok() {
        assert!(diags("char c = 65;").is_empty());
    }

    #[test]
    fn char_widening_to_int_is_ok() {
        let src = "class C { Provider p; int m() { char c = 'a'; return c; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn compound_assignment_is_not_flagged() {
        // `x += p.aLong()` gets an implicit narrowing cast in Java → NOT an error.
        assert!(diags("int x = 0; x += p.aLong();").is_empty());
    }

    #[test]
    fn negative_in_range_byte_constant_is_ok() {
        assert!(diags("byte b = -1;").is_empty());
    }

    #[test]
    fn constant_expressions_that_fit_are_ok() {
        assert!(run("class C { void m() { char next = 'a' + 1; } }").is_empty());
        assert!(run("class C { static final int K = 10; void m() { byte b = K; } }").is_empty());
        assert!(run("class C { void m() { final int local = 20; byte b = local; } }").is_empty());
        assert!(run("class C { void m() { byte b = 'a'; } }").is_empty());
    }

    #[test]
    fn variables_are_not_constants() {
        assert_eq!(run("class C { void m(int p) { byte b = p; } }").len(), 1);
        assert_eq!(run("class C { static int K = 10; void m() { byte b = K; } }").len(), 1);
        assert_eq!(run("class C { static final int K = 1000; void m() { byte b = K; } }").len(), 1);
    }

    #[test]
    fn long_literal_to_int_is_flagged() {
        // `5L` is a long constant, not an int-fit → narrowing to int is a real error.
        let d = diags("int x = 5L;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`long`") && d[0].contains("`int`"), "{d:?}");
    }
}
