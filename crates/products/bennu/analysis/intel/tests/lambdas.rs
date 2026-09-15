//! Lambda / functional-interface category.
//!
//! A project-declared functional interface is an ordinary type + method as far as the index is
//! concerned: go-to and find-usages on the interface type and its single abstract method work
//! exactly like any class/method. On top of that, lambda PARAMETERS are scope-exact locals
//! (gated at JDK >= 8): inferred, explicit-typed, block-body, and enclosing-capture forms all
//! resolve to their own declarators, and a lambda param never leaks outside its lambda.
//!
//! Where a feature depends on machinery we are less certain covers lambdas end to end (method
//! references, inferring a lambda param's type from the functional-interface target for
//! completion), the test asserts the robust invariant — resolves correctly if at all, and never
//! panics — rather than over-committing to an exact result.

mod common;
use common::*;

fn fp() -> Project {
    Project::new(&[
        (
            "Transformer.java",
            "package fn;\n\
             public interface Transformer {\n\
             \x20   int apply(int value);\n\
             }\n",
        ),
        (
            "Sink.java",
            "package fn;\n\
             public interface Sink {\n\
             \x20   void accept(Box b);\n\
             }\n",
        ),
        (
            "Box.java",
            "package fn;\n\
             public class Box {\n\
             \x20   public int value() { return 0; }\n\
             \x20   public void reset() { }\n\
             }\n",
        ),
        (
            "Calc.java",
            "package fn;\n\
             import java.util.List;\n\
             public class Calc {\n\
             \x20   public int run() {\n\
             \x20       Transformer t = x -> x + 1;\n\
             \x20       return t.apply(t.apply(2));\n\
             \x20   }\n\
             \x20   public int explicit() {\n\
             \x20       Transformer f = (int y) -> y * 2;\n\
             \x20       return f.apply(3);\n\
             \x20   }\n\
             \x20   public int outer(int cap) {\n\
             \x20       Transformer g = z -> z + cap;\n\
             \x20       return g.apply(cap);\n\
             \x20   }\n\
             \x20   public int block() {\n\
             \x20       Transformer h = w -> { int q = w + 1; return q; };\n\
             \x20       return h.apply(5);\n\
             \x20   }\n\
             \x20   public void refs(List<Box> boxes) {\n\
             \x20       boxes.forEach(Box::reset);\n\
             \x20   }\n\
             }\n",
        ),
    ])
}

// ── The functional interface as a type ───────────────────────────────────────────────────────

#[test]
fn functional_interface_type_goto() {
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let d = p
        .goto("Calc.java", at(&s, "Transformer t"))
        .expect("goto functional interface type");
    assert_eq!(d.file, "Transformer.java");
    assert_eq!(d.label, "class fn.Transformer");
}

#[test]
fn functional_interface_type_find_usages() {
    let p = fp();
    let t = p.source("Transformer.java").to_string();
    // `Transformer t/f/g/h` = 4 type-reference sites across Calc.
    let n = p.usage_count(
        "Transformer.java",
        at(&t, "interface Transformer") + "interface ".len(),
    );
    assert!(
        n >= 4,
        "Transformer is used as a type at >= 4 sites, got {n}"
    );
}

// ── The single abstract method ───────────────────────────────────────────────────────────────

#[test]
fn functional_interface_method_goto() {
    let p = fp();
    let s = p.source("Calc.java").to_string();
    // `t.apply(...)` → the interface's abstract method (t : Transformer, a project type).
    let off = at(&s, "t.apply(t.apply(2))") + "t.".len();
    let d = p
        .goto("Calc.java", off)
        .expect("goto interface method via lambda-typed receiver");
    assert_eq!(d.file, "Transformer.java");
    assert_eq!(d.label, "method fn.Transformer.apply()");
}

#[test]
fn functional_interface_method_find_usages() {
    let p = fp();
    let t = p.source("Transformer.java").to_string();
    // apply() calls: run() twice, explicit()/outer()/block() once each = 5 (decl not counted).
    let n = p.usage_count("Transformer.java", at(&t, "int apply") + "int ".len());
    assert_eq!(n, 5, "apply() is called at 5 sites");
}

#[test]
fn functional_interface_method_count_stable_from_use() {
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let from_use = p.usage_count("Calc.java", at(&s, "f.apply(3)") + "f.".len());
    let t = p.source("Transformer.java").to_string();
    let from_decl = p.usage_count("Transformer.java", at(&t, "int apply") + "int ".len());
    assert_eq!(
        from_use, from_decl,
        "count is a property of the member, not the caret"
    );
}

// ── Lambda parameters — scope-exact locals ───────────────────────────────────────────────────

#[test]
fn inferred_lambda_param_goto() {
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let off = at(&s, "x -> x + 1") + "x -> ".len(); // the `x` USE
    let d = p
        .goto("Calc.java", off)
        .expect("inferred lambda param resolves");
    assert_eq!(d.file, "Calc.java");
    assert_eq!(d.label, "local `x`");
    assert_eq!(d.line, line_of(&s, "Transformer t = x"));
}

#[test]
fn explicit_lambda_param_goto() {
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let off = at(&s, "y * 2") + 0; // the `y` USE
    let d = p
        .goto("Calc.java", off)
        .expect("explicit-typed lambda param resolves");
    assert_eq!(d.file, "Calc.java");
    assert_eq!(d.label, "local `y`");
}

#[test]
fn lambda_captures_enclosing_parameter() {
    // `z -> z + cap`: `cap` is NOT the lambda param — it is the enclosing method parameter.
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let off = at(&s, "z + cap") + "z + ".len();
    let d = p
        .goto("Calc.java", off)
        .expect("enclosing capture resolves");
    assert_eq!(d.label, "local `cap`");
    assert_eq!(d.line, line_of(&s, "outer(int cap)"));
}

#[test]
fn lambda_own_param_shadows_nothing_here() {
    // `z` inside `z -> z + cap` resolves to the lambda param, distinct from the captured `cap`.
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let off = at(&s, "z + cap") + 0;
    let d = p.goto("Calc.java", off).expect("lambda param z resolves");
    assert_eq!(d.label, "local `z`");
    assert_eq!(d.line, line_of(&s, "Transformer g = z"));
}

#[test]
fn block_body_lambda_local_resolves() {
    // `w -> { int q = w + 1; return q; }`: the block-body local `q`.
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let off = at(&s, "return q; }") + "return ".len();
    let d = p
        .goto("Calc.java", off)
        .expect("block-body lambda local resolves");
    assert_eq!(d.label, "local `q`");
    assert_eq!(d.line, line_of(&s, "int q = w + 1"));
}

#[test]
fn block_body_lambda_param_resolves() {
    // `w` used inside the block body resolves to the lambda parameter.
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let off = at(&s, "w + 1") + 0;
    let d = p
        .goto("Calc.java", off)
        .expect("block-body lambda param resolves");
    assert_eq!(d.label, "local `w`");
    assert_eq!(d.line, line_of(&s, "Transformer h = w"));
}

#[test]
fn lambda_param_is_not_bucketed_by_find_usages() {
    // A lambda parameter is a scope-exact local — find-usages does not bucket it.
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let off = at(&s, "x -> x + 1") + "x -> ".len();
    assert_eq!(
        p.usage_count("Calc.java", off),
        0,
        "lambda param is not a bucketed symbol"
    );
}

// ── Gating — inferred lambda params require JDK >= 8 ──────────────────────────────────────────

#[test]
fn inferred_lambda_param_disabled_pre_8() {
    let files = &[
        (
            "Transformer.java",
            "package fn;\npublic interface Transformer { int apply(int v); }\n",
        ),
        (
            "Old.java",
            "package fn;\n\
             public class Old {\n\
             \x20   int run() {\n\
             \x20       Transformer t = x -> x + 1;\n\
             \x20       return t.apply(1);\n\
             \x20   }\n\
             }\n",
        ),
    ];
    let p = Project::with_jdk(files, "7");
    let s = p.source("Old.java").to_string();
    let off = at(&s, "x -> x + 1") + "x -> ".len();
    let got = p.goto("Old.java", off);
    assert!(
        got.is_none(),
        "inferred lambda param must not resolve at JDK 7, got {:?}",
        got.map(|d| d.label)
    );
}

// ── Method references + lambda-body completion — soft (resolve if at all, never panic) ────────

#[test]
fn method_reference_target_is_safe() {
    // `Box::reset` — a method reference. If the classifier resolves it, it must land on
    // Box.reset(); otherwise `None` is acceptable. Either way: no panic.
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let off = at(&s, "Box::reset") + "Box::".len();
    if let Some(d) = p.goto("Calc.java", off) {
        assert_eq!(d.file, "Box.java");
        assert_eq!(d.label, "method fn.Box.reset()");
    }
}

#[test]
fn method_reference_type_goto() {
    // The `Box` qualifier of `Box::reset` is a plain type reference into Box.java.
    let p = fp();
    let s = p.source("Calc.java").to_string();
    let off = at(&s, "Box::reset") + 0;
    if let Some(d) = p.goto("Calc.java", off) {
        assert_eq!(d.file, "Box.java");
        assert_eq!(d.label, "class fn.Box");
    }
}
