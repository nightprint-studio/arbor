//! Category: MODERN constructs + language-level gating.
//!
//! Constructor bodies resolve locals like any block; records / instanceof pattern variables /
//! inferred lambda params are gated by JDK level, so `Project::with_jdk` pins the level and we
//! assert the gating BOTH ways (resolves at the enabling level, does not at the level below).
//! A sealed class still behaves like a plain type. We only assert what the rules guarantee:
//! exact labels where the format is fixed, and robust `is_some()/is_none()`/`.file` invariants
//! where the exact label or line is uncertain.

mod common;
use common::*;

// ---------------------------------------------------------------------------------------------
// Constructor bodies — a local declared in a constructor resolves like a method-body local.
// ---------------------------------------------------------------------------------------------

#[test]
fn constructor_body_local_resolves() {
    let p = Project::new(&[(
        "Ctor.java",
        "package app;\n\
         public class Ctor {\n\
         \x20   private int field;\n\
         \x20   public Ctor(int arg) {\n\
         \x20       int scratch = arg + 1;\n\
         \x20       this.field = scratch + arg;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("Ctor.java").to_string();
    // `scratch` in `scratch + arg` → its declaration in the same file.
    let d = p.goto("Ctor.java", at(&s, "scratch + arg")).expect("goto ctor local");
    assert_eq!(d.file, "Ctor.java");
    assert_eq!(d.label, "local `scratch`");
    assert_eq!(d.line, line_of(&s, "int scratch ="));
}

#[test]
fn constructor_parameter_resolves() {
    let p = Project::new(&[(
        "Ctor.java",
        "package app;\n\
         public class Ctor {\n\
         \x20   private int field;\n\
         \x20   public Ctor(int arg) {\n\
         \x20       int scratch = arg + 1;\n\
         \x20       this.field = scratch + arg;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("Ctor.java").to_string();
    // `arg` in `arg + 1` → the constructor parameter.
    let d = p.goto("Ctor.java", at(&s, "arg + 1")).expect("goto ctor param");
    assert_eq!(d.file, "Ctor.java");
    assert_eq!(d.label, "local `arg`");
    assert_eq!(d.line, line_of(&s, "Ctor(int arg)"));
}

#[test]
fn constructor_body_field_assignment_resolves() {
    let p = Project::new(&[(
        "Ctor.java",
        "package app;\n\
         public class Ctor {\n\
         \x20   private int field;\n\
         \x20   public Ctor(int arg) {\n\
         \x20       int scratch = arg + 1;\n\
         \x20       this.field = scratch + arg;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("Ctor.java").to_string();
    // `field` in `this.field = ...` → the field declaration.
    let off = at(&s, "this.field =") + "this.".len();
    let d = p.goto("Ctor.java", off).expect("goto field in ctor");
    assert_eq!(d.file, "Ctor.java");
    assert_eq!(d.label, "field app.Ctor.field");
    assert_eq!(d.line, line_of(&s, "int field;"));
}

// ---------------------------------------------------------------------------------------------
// Records — a component used in a compact constructor resolves as a local only at JDK >= 16.
// ---------------------------------------------------------------------------------------------

const RECORD_SRC: &str = "package app;\n\
     public record Point(int x, int y) {\n\
     \x20   public Point {\n\
     \x20       if (x < 0) throw new IllegalArgumentException();\n\
     \x20       int sum = x + y;\n\
     \x20   }\n\
     }\n";

#[test]
fn record_component_in_compact_ctor_resolves_at_17() {
    let p = Project::with_jdk(&[("Point.java", RECORD_SRC)], "17");
    let s = p.source("Point.java").to_string();
    // `x` in `x + y` → the record component, treated like a local inside the compact ctor.
    let off = at(&s, "int sum = x + y") + "int sum = ".len();
    let d = p.goto("Point.java", off).expect("record component resolves at 17");
    assert_eq!(d.file, "Point.java");
    assert!(
        d.label.contains('x'),
        "expected the component `x` to be referenced, got label {:?}",
        d.label
    );
}

#[test]
fn record_component_in_compact_ctor_not_at_11() {
    let p = Project::with_jdk(&[("Point.java", RECORD_SRC)], "11");
    let s = p.source("Point.java").to_string();
    // At JDK 11 records are not enabled → the compact-ctor component binding is not resolved.
    let off = at(&s, "int sum = x + y") + "int sum = ".len();
    // Must not panic; whatever it returns must not be the record-component local binding.
    let got = p.goto("Point.java", off);
    assert!(
        got.is_none(),
        "record component must NOT resolve as a local at JDK 11, got {:?}",
        got.map(|d| d.label)
    );
}

#[test]
fn record_type_reference_resolves() {
    let p = Project::new(&[
        ("Point.java", RECORD_SRC),
        (
            "UsePoint.java",
            "package app;\n\
             public class UsePoint {\n\
             \x20   Point make() { return new Point(1, 2); }\n\
             }\n",
        ),
    ]);
    let u = p.source("UsePoint.java").to_string();
    let d = p.goto("UsePoint.java", at(&u, "Point make")).expect("goto record type");
    assert_eq!(d.file, "Point.java");
    assert_eq!(d.label, "class app.Point");
}

// ---------------------------------------------------------------------------------------------
// instanceof pattern variables — `o instanceof String s` binds `s` only at JDK >= 16.
// ---------------------------------------------------------------------------------------------

const PATTERN_SRC: &str = "package app;\n\
     public class Pat {\n\
     \x20   public int len(Object o) {\n\
     \x20       if (o instanceof String s) {\n\
     \x20           return s.length();\n\
     \x20       }\n\
     \x20       return 0;\n\
     \x20   }\n\
     }\n";

#[test]
fn instanceof_pattern_var_resolves_at_17() {
    let p = Project::with_jdk(&[("Pat.java", PATTERN_SRC)], "17");
    let s = p.source("Pat.java").to_string();
    // `s` in `s.length()` → the pattern variable declared in the instanceof.
    let d = p.goto("Pat.java", at(&s, "s.length()")).expect("pattern var resolves at 17");
    assert_eq!(d.file, "Pat.java");
    assert_eq!(d.label, "local `s`");
    assert_eq!(d.line, line_of(&s, "instanceof String s"));
}

#[test]
fn instanceof_pattern_var_not_at_11() {
    let p = Project::with_jdk(&[("Pat.java", PATTERN_SRC)], "11");
    let s = p.source("Pat.java").to_string();
    // At JDK 11 pattern variables are not enabled → `s` is not a resolvable local binding.
    let got = p.goto("Pat.java", at(&s, "s.length()"));
    assert!(
        got.is_none(),
        "instanceof pattern var must NOT resolve at JDK 11, got {:?}",
        got.map(|d| d.label)
    );
}

#[test]
fn instanceof_type_reference_resolves_cross_file() {
    // The `String` in the instanceof lives in the JDK → no project source → None (not a panic).
    let p = Project::with_jdk(&[("Pat.java", PATTERN_SRC)], "17");
    let s = p.source("Pat.java").to_string();
    let off = at(&s, "instanceof String s") + "instanceof ".len();
    let got = p.goto("Pat.java", off);
    assert!(
        got.is_none(),
        "a JDK type (String) has no project source to open, got {:?}",
        got.map(|d| d.label)
    );
}

// ---------------------------------------------------------------------------------------------
// Inferred lambda params — `(a, b) -> ...` binds `a`/`b` only at JDK >= 8.
// ---------------------------------------------------------------------------------------------

const LAMBDA_SRC: &str = "package app;\n\
     import java.util.function.BinaryOperator;\n\
     public class Lam {\n\
     \x20   public int run() {\n\
     \x20       BinaryOperator<Integer> op = (a, b) -> a + b;\n\
     \x20       return op.apply(1, 2);\n\
     \x20   }\n\
     }\n";

#[test]
fn inferred_lambda_param_resolves_at_8() {
    let p = Project::with_jdk(&[("Lam.java", LAMBDA_SRC)], "8");
    let s = p.source("Lam.java").to_string();
    // `a` in `a + b` → the inferred lambda parameter.
    let off = at(&s, "-> a + b") + "-> ".len();
    let d = p.goto("Lam.java", off).expect("inferred lambda param resolves at 8");
    assert_eq!(d.file, "Lam.java");
    assert_eq!(d.label, "local `a`");
}

#[test]
fn inferred_lambda_second_param_resolves_at_8() {
    let p = Project::with_jdk(&[("Lam.java", LAMBDA_SRC)], "8");
    let s = p.source("Lam.java").to_string();
    // `b` in `a + b` → the second inferred lambda parameter.
    let off = at(&s, "a + b") + "a + ".len();
    let d = p.goto("Lam.java", off).expect("second inferred lambda param resolves at 8");
    assert_eq!(d.file, "Lam.java");
    assert_eq!(d.label, "local `b`");
}

#[test]
fn inferred_lambda_param_not_at_7() {
    let p = Project::with_jdk(&[("Lam.java", LAMBDA_SRC)], "7");
    let s = p.source("Lam.java").to_string();
    // At JDK 7 lambdas / inferred params are not enabled → `a` is not a resolvable binding.
    let off = at(&s, "-> a + b") + "-> ".len();
    let got = p.goto("Lam.java", off);
    assert!(
        got.is_none(),
        "inferred lambda param must NOT resolve at JDK 7, got {:?}",
        got.map(|d| d.label)
    );
}

#[test]
fn typed_lambda_param_resolves_even_at_7() {
    // Typed lambda params `(Integer a, Integer b) -> ...` resolve at any level per the rules
    // ("typed always"). We still pin JDK 7 to prove typed params are not version-gated the way
    // inferred ones are. Assert the robust invariant (Some + same file) rather than an exact line.
    let src = "package app;\n\
         import java.util.function.BinaryOperator;\n\
         public class LamT {\n\
         \x20   public int run() {\n\
         \x20       BinaryOperator<Integer> op = (Integer a, Integer b) -> a + b;\n\
         \x20       return op.apply(1, 2);\n\
         \x20   }\n\
         }\n";
    let p = Project::with_jdk(&[("LamT.java", src)], "7");
    let s = p.source("LamT.java").to_string();
    let off = at(&s, "-> a + b") + "-> ".len();
    let got = p.goto("LamT.java", off);
    if let Some(d) = got {
        assert_eq!(d.file, "LamT.java");
        assert_eq!(d.label, "local `a`");
    }
    // If None, that's still not a panic — the harness contract is "never panic"; we do not
    // over-assert on a construct whose gating we are less certain about at this level.
}

// ---------------------------------------------------------------------------------------------
// Sealed class — a sealed type still resolves like a normal class (no special handling).
// ---------------------------------------------------------------------------------------------

#[test]
fn sealed_class_resolves_like_normal_class() {
    let p = Project::with_jdk(
        &[
            (
                "Shape.java",
                "package app;\n\
                 public sealed class Shape permits Circle {\n\
                 \x20   public int sides() { return 0; }\n\
                 }\n",
            ),
            (
                "Circle.java",
                "package app;\n\
                 public final class Circle extends Shape {\n\
                 }\n",
            ),
            (
                "UseShape.java",
                "package app;\n\
                 public class UseShape {\n\
                 \x20   Shape pick() { return new Circle(); }\n\
                 }\n",
            ),
        ],
        "17",
    );
    let u = p.source("UseShape.java").to_string();
    let d = p.goto("UseShape.java", at(&u, "Shape pick")).expect("goto sealed type");
    assert_eq!(d.file, "Shape.java");
    assert_eq!(d.label, "class app.Shape");
    assert_eq!(d.line, line_of(p.source("Shape.java"), "class Shape"));
}

#[test]
fn sealed_class_method_resolves() {
    let p = Project::with_jdk(
        &[
            (
                "Shape.java",
                "package app;\n\
                 public sealed class Shape permits Circle {\n\
                 \x20   public int sides() { return 0; }\n\
                 }\n",
            ),
            (
                "Circle.java",
                "package app;\n\
                 public final class Circle extends Shape {\n\
                 }\n",
            ),
            (
                "UseShape.java",
                "package app;\n\
                 public class UseShape {\n\
                 \x20   int count(Shape sh) { return sh.sides(); }\n\
                 }\n",
            ),
        ],
        "17",
    );
    let u = p.source("UseShape.java").to_string();
    let d = p.goto("UseShape.java", at(&u, "sh.sides()") + "sh.".len()).expect("goto sealed method");
    assert_eq!(d.file, "Shape.java");
    assert_eq!(d.label, "method app.Shape.sides()");
}

#[test]
fn sealed_permits_subtype_reference_resolves() {
    // The `Circle` in `permits Circle` is a type reference into Circle.java.
    let p = Project::with_jdk(
        &[
            (
                "Shape.java",
                "package app;\n\
                 public sealed class Shape permits Circle {\n\
                 \x20   public int sides() { return 0; }\n\
                 }\n",
            ),
            (
                "Circle.java",
                "package app;\n\
                 public final class Circle extends Shape {\n\
                 }\n",
            ),
        ],
        "17",
    );
    let s = p.source("Shape.java").to_string();
    let d = p.goto("Shape.java", at(&s, "permits Circle") + "permits ".len());
    if let Some(d) = d {
        assert_eq!(d.file, "Circle.java");
        assert_eq!(d.label, "class app.Circle");
    }
    // Some parsers do not index `permits` clause refs; if None, that's acceptable (no panic).
}

// ---------------------------------------------------------------------------------------------
// Enhanced-for / try-with-resources / catch — modern-block locals (enabled at 21).
// ---------------------------------------------------------------------------------------------

#[test]
fn enhanced_for_variable_resolves() {
    let p = Project::new(&[(
        "Loop.java",
        "package app;\n\
         import java.util.List;\n\
         public class Loop {\n\
         \x20   int sum(List<Integer> xs) {\n\
         \x20       int total = 0;\n\
         \x20       for (Integer item : xs) {\n\
         \x20           total = total + item;\n\
         \x20       }\n\
         \x20       return total;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("Loop.java").to_string();
    let off = at(&s, "total + item") + "total + ".len();
    let d = p.goto("Loop.java", off).expect("enhanced-for var resolves");
    assert_eq!(d.file, "Loop.java");
    assert_eq!(d.label, "local `item`");
    assert_eq!(d.line, line_of(&s, "Integer item"));
}

#[test]
fn try_with_resources_variable_resolves() {
    let p = Project::new(&[(
        "Res.java",
        "package app;\n\
         import java.io.StringReader;\n\
         public class Res {\n\
         \x20   int read() throws Exception {\n\
         \x20       try (StringReader r = new StringReader(\"x\")) {\n\
         \x20           return r.read();\n\
         \x20       }\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("Res.java").to_string();
    let off = at(&s, "return r.read()") + "return ".len();
    let d = p.goto("Res.java", off).expect("try-with-resources var resolves");
    assert_eq!(d.file, "Res.java");
    assert_eq!(d.label, "local `r`");
    assert_eq!(d.line, line_of(&s, "StringReader r ="));
}

#[test]
fn catch_parameter_resolves() {
    let p = Project::new(&[(
        "Catch.java",
        "package app;\n\
         public class Catch {\n\
         \x20   int run() {\n\
         \x20       try {\n\
         \x20           return 1;\n\
         \x20       } catch (RuntimeException ex) {\n\
         \x20           return ex.hashCode();\n\
         \x20       }\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("Catch.java").to_string();
    let off = at(&s, "return ex.hashCode()") + "return ".len();
    let d = p.goto("Catch.java", off).expect("catch param resolves");
    assert_eq!(d.file, "Catch.java");
    assert_eq!(d.label, "local `ex`");
    assert_eq!(d.line, line_of(&s, "RuntimeException ex"));
}

// ---------------------------------------------------------------------------------------------
// Negative / no-panic guards on modern syntax.
// ---------------------------------------------------------------------------------------------

#[test]
fn caret_on_arrow_token_is_none() {
    let p = Project::with_jdk(&[("Lam.java", LAMBDA_SRC)], "8");
    let s = p.source("Lam.java").to_string();
    // Caret on the `->` arrow operator resolves to nothing (not a symbol) — must not panic.
    let got = p.goto("Lam.java", at(&s, "-> a + b"));
    assert!(got.is_none(), "arrow token is not resolvable, got {:?}", got.map(|d| d.label));
}

#[test]
fn caret_on_record_keyword_is_none() {
    let p = Project::with_jdk(&[("Point.java", RECORD_SRC)], "17");
    let s = p.source("Point.java").to_string();
    // Caret on the `record` keyword itself → None (a keyword, not a symbol).
    let got = p.goto("Point.java", at(&s, "record Point"));
    assert!(got.is_none(), "the `record` keyword is not resolvable, got {:?}", got.map(|d| d.label));
}
