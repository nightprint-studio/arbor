//! Category: LOCAL variables and parameters in every scope.
//!
//! Each test resolves a use-site of a local/parameter to its declaration in the SAME file, with
//! a `local `name`` label, on the right declaration line. Also covers a local shadowing a field
//! of the same name (the local wins), and the same name being a distinct local in two different
//! methods (scope-exactness).

mod common;
use common::*;

// ---------------------------------------------------------------------------------------------
// Method body
// ---------------------------------------------------------------------------------------------

#[test]
fn local_in_method_body() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m() {\n\
         \x20       int total = 5;\n\
         \x20       return total + 1;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "return total") + "return ".len();
    let d = p.goto("A.java", off).expect("goto local in method body");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `total`");
    assert_eq!(d.line, line_of(&s, "int total ="));
}

// ---------------------------------------------------------------------------------------------
// Method parameter
// ---------------------------------------------------------------------------------------------

#[test]
fn parameter_in_method_body() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m(int amount) {\n\
         \x20       return amount * 2;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "return amount") + "return ".len();
    let d = p.goto("A.java", off).expect("goto parameter");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `amount`");
    assert_eq!(d.line, line_of(&s, "int amount"));
}

// ---------------------------------------------------------------------------------------------
// Constructor body (both a ctor parameter and a ctor-local)
// ---------------------------------------------------------------------------------------------

#[test]
fn parameter_in_constructor_body() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   private int v;\n\
         \x20   public A(int seed) {\n\
         \x20       this.v = seed + 10;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "= seed") + "= ".len();
    let d = p.goto("A.java", off).expect("goto ctor parameter");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `seed`");
    assert_eq!(d.line, line_of(&s, "int seed"));
}

#[test]
fn local_in_constructor_body() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   private int v;\n\
         \x20   public A() {\n\
         \x20       int scratch = 3;\n\
         \x20       this.v = scratch;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "= scratch;") + "= ".len();
    let d = p.goto("A.java", off).expect("goto ctor local");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `scratch`");
    assert_eq!(d.line, line_of(&s, "int scratch ="));
}

// ---------------------------------------------------------------------------------------------
// Nested blocks: if / for / while
// ---------------------------------------------------------------------------------------------

#[test]
fn local_in_if_block() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m(boolean flag) {\n\
         \x20       if (flag) {\n\
         \x20           int inner = 7;\n\
         \x20           return inner + 1;\n\
         \x20       }\n\
         \x20       return 0;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "return inner") + "return ".len();
    let d = p.goto("A.java", off).expect("goto local in if block");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `inner`");
    assert_eq!(d.line, line_of(&s, "int inner ="));
}

#[test]
fn local_in_classic_for_init() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m() {\n\
         \x20       int sum = 0;\n\
         \x20       for (int idx = 0; idx < 3; idx++) {\n\
         \x20           sum += idx;\n\
         \x20       }\n\
         \x20       return sum;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    // The `idx` inside the loop body — resolves to the for-init declaration.
    let off = at(&s, "+= idx;") + "+= ".len();
    let d = p.goto("A.java", off).expect("goto classic for-init local");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `idx`");
    assert_eq!(d.line, line_of(&s, "int idx ="));
}

#[test]
fn local_in_while_block() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m() {\n\
         \x20       int n = 0;\n\
         \x20       while (n < 10) {\n\
         \x20           int step = 2;\n\
         \x20           n = n + step;\n\
         \x20       }\n\
         \x20       return n;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "+ step;") + "+ ".len();
    let d = p.goto("A.java", off).expect("goto local in while block");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `step`");
    assert_eq!(d.line, line_of(&s, "int step ="));
}

// ---------------------------------------------------------------------------------------------
// Enhanced-for variable
// ---------------------------------------------------------------------------------------------

#[test]
fn enhanced_for_variable() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m(int[] xs) {\n\
         \x20       int acc = 0;\n\
         \x20       for (int elem : xs) {\n\
         \x20           acc = acc + elem;\n\
         \x20       }\n\
         \x20       return acc;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "+ elem;") + "+ ".len();
    let d = p.goto("A.java", off).expect("goto enhanced-for variable");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `elem`");
    assert_eq!(d.line, line_of(&s, "int elem :"));
}

// ---------------------------------------------------------------------------------------------
// Switch cases
// ---------------------------------------------------------------------------------------------

#[test]
fn local_in_switch_case() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m(int k) {\n\
         \x20       switch (k) {\n\
         \x20           case 1: {\n\
         \x20               int chosen = 42;\n\
         \x20               return chosen;\n\
         \x20           }\n\
         \x20           default:\n\
         \x20               return 0;\n\
         \x20       }\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "return chosen") + "return ".len();
    let d = p.goto("A.java", off).expect("goto local in switch case");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `chosen`");
    assert_eq!(d.line, line_of(&s, "int chosen ="));
}

#[test]
fn switch_selector_parameter() {
    // The switch selector `k` is itself a parameter use — must resolve to the parameter decl.
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m(int selector) {\n\
         \x20       switch (selector) {\n\
         \x20           default: return 0;\n\
         \x20       }\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "switch (selector") + "switch (".len();
    let d = p
        .goto("A.java", off)
        .expect("goto switch selector parameter");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `selector`");
    assert_eq!(d.line, line_of(&s, "int selector"));
}

// ---------------------------------------------------------------------------------------------
// catch parameter
// ---------------------------------------------------------------------------------------------

#[test]
fn catch_parameter() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m() {\n\
         \x20       try {\n\
         \x20           return 1;\n\
         \x20       } catch (RuntimeException ex) {\n\
         \x20           return ex.hashCode();\n\
         \x20       }\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "return ex.hashCode") + "return ".len();
    let d = p.goto("A.java", off).expect("goto catch parameter");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `ex`");
    assert_eq!(d.line, line_of(&s, "RuntimeException ex"));
}

// ---------------------------------------------------------------------------------------------
// try-with-resources variable
// ---------------------------------------------------------------------------------------------

#[test]
fn try_with_resources_variable() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         import java.io.StringReader;\n\
         public class A {\n\
         \x20   public int m() throws Exception {\n\
         \x20       try (StringReader res = new StringReader(\"x\")) {\n\
         \x20           return res.read();\n\
         \x20       }\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "return res.read") + "return ".len();
    let d = p
        .goto("A.java", off)
        .expect("goto try-with-resources variable");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `res`");
    assert_eq!(d.line, line_of(&s, "StringReader res ="));
}

// ---------------------------------------------------------------------------------------------
// Typed lambda parameters (valid at every JDK level)
// ---------------------------------------------------------------------------------------------

#[test]
fn typed_lambda_parameter() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         import java.util.function.IntUnaryOperator;\n\
         public class A {\n\
         \x20   public IntUnaryOperator m() {\n\
         \x20       return (int arg) -> arg + 1;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "-> arg") + "-> ".len();
    let d = p.goto("A.java", off).expect("goto typed lambda parameter");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `arg`");
    assert_eq!(d.line, line_of(&s, "int arg"));
}

// ---------------------------------------------------------------------------------------------
// A local shadowing a field of the same name — the LOCAL wins
// ---------------------------------------------------------------------------------------------

#[test]
fn local_shadows_field_of_same_name() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   private int count;\n\
         \x20   public int m() {\n\
         \x20       int count = 99;\n\
         \x20       return count + 1;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "return count") + "return ".len();
    let d = p.goto("A.java", off).expect("goto shadowing local");
    assert_eq!(d.file, "A.java");
    // The local declaration wins over the field, and the label says `local`, not `field`.
    assert_eq!(d.label, "local `count`");
    assert_eq!(d.line, line_of(&s, "int count = 99"));
    // Sanity: the field decl is on a different (earlier) line.
    assert_ne!(d.line, line_of(&s, "private int count"));
}

#[test]
fn field_still_reachable_via_this_when_shadowed() {
    // `this.count` inside the shadowing method must resolve to the FIELD, not the local.
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   private int count;\n\
         \x20   public int m() {\n\
         \x20       int count = 99;\n\
         \x20       return this.count + count;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "this.count") + "this.".len();
    let d = p.goto("A.java", off).expect("goto field via this");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "field app.A.count");
    assert_eq!(d.line, line_of(&s, "private int count"));
}

// ---------------------------------------------------------------------------------------------
// Same name, distinct local in two different methods — scope exactness
// ---------------------------------------------------------------------------------------------

#[test]
fn same_name_distinct_locals_in_two_methods() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int first() {\n\
         \x20       int value = 1;\n\
         \x20       return value;\n\
         \x20   }\n\
         \x20   public int second() {\n\
         \x20       int value = 2;\n\
         \x20       return value;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();

    // Use in first() → first()'s declaration line.
    let off_first = at(&s, "return value;") + "return ".len();
    let d1 = p.goto("A.java", off_first).expect("goto value in first()");
    assert_eq!(d1.file, "A.java");
    assert_eq!(d1.label, "local `value`");
    assert_eq!(d1.line, line_of(&s, "int value = 1"));

    // Use in second() → second()'s declaration line (a DIFFERENT line).
    let off_second = at_last(&s, "return value;") + "return ".len();
    let d2 = p
        .goto("A.java", off_second)
        .expect("goto value in second()");
    assert_eq!(d2.file, "A.java");
    assert_eq!(d2.label, "local `value`");
    assert_eq!(d2.line, line_of(&s, "int value = 2"));

    assert_ne!(
        d1.line, d2.line,
        "the two locals are scope-exact, on different lines"
    );
}

// ---------------------------------------------------------------------------------------------
// find-usages on a local is scope-exact and NOT bucketed → count 0
// ---------------------------------------------------------------------------------------------

#[test]
fn find_usages_of_local_is_zero() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m() {\n\
         \x20       int local = 4;\n\
         \x20       return local + local;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    // Caret on the local declaration: locals are not bucketed by find-usages → 0.
    let n = p.usage_count("A.java", at(&s, "int local ="));
    assert_eq!(
        n, 0,
        "a local is scope-exact and not counted by find-usages"
    );
}

// ---------------------------------------------------------------------------------------------
// Caret on the declaration name itself still resolves to the declaration (self-goto)
// ---------------------------------------------------------------------------------------------

#[test]
fn caret_on_local_declaration_name() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         public class A {\n\
         \x20   public int m() {\n\
         \x20       int target = 8;\n\
         \x20       return target;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    // Land directly on the declared name `target` in `int target = 8;`.
    let off = at(&s, "target = 8");
    let d = p
        .goto("A.java", off)
        .expect("goto on the declaration name itself");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `target`");
    assert_eq!(d.line, line_of(&s, "int target ="));
}

// ---------------------------------------------------------------------------------------------
// A local used in a lambda body but declared in the enclosing method (captured) still resolves
// ---------------------------------------------------------------------------------------------

#[test]
fn captured_local_from_lambda_body() {
    let p = Project::new(&[(
        "A.java",
        "package app;\n\
         import java.util.function.IntSupplier;\n\
         public class A {\n\
         \x20   public IntSupplier m() {\n\
         \x20       int captured = 5;\n\
         \x20       return () -> captured + 1;\n\
         \x20   }\n\
         }\n",
    )]);
    let s = p.source("A.java").to_string();
    let off = at(&s, "-> captured") + "-> ".len();
    let d = p
        .goto("A.java", off)
        .expect("goto captured local from lambda body");
    assert_eq!(d.file, "A.java");
    assert_eq!(d.label, "local `captured`");
    assert_eq!(d.line, line_of(&s, "int captured ="));
}
