//! A rename must refuse a name that is ALREADY taken where the renamed thing lives.
//!
//! These are the failures no edit list can show: every individual edit in it is correct, and the
//! result either does not compile or — worse — compiles and means something else. A method renamed
//! onto a supertype's signature silently becomes an override; a field renamed onto an inherited one
//! hides it; a local renamed onto a name already in scope captures it.

mod common;
use common::{at, Project};

const BASE: &str = r#"package p;
public class Base {
    public void size() { }
    protected int total;
}
"#;

const IMPL: &str = r#"package p;
public class Impl extends Base {
    public void count() { }
    private int n;
}
"#;

#[test]
fn a_method_rename_onto_an_inherited_signature_is_blocked() {
    let p = Project::new(&[("p/Base.java", BASE), ("p/Impl.java", IMPL)]);
    let plan = p
        .rename("p/Impl.java", at(IMPL, "count"), "size")
        .expect("a plan");
    let reason = plan.blocked.expect("a refusal");
    assert!(reason.contains("overriding"), "{reason}");
}

#[test]
fn a_field_rename_onto_an_inherited_field_is_blocked_as_hiding() {
    let p = Project::new(&[("p/Base.java", BASE), ("p/Impl.java", IMPL)]);
    let plan = p
        .rename("p/Impl.java", at(IMPL, "int n") + 4, "total")
        .expect("a plan");
    let reason = plan.blocked.expect("a refusal");
    assert!(reason.contains("hide"), "{reason}");
}

/// A name taken at a DIFFERENT arity is an overload, which is ordinary Java — refusing there would
/// block a correct rename.
#[test]
fn a_method_rename_onto_another_arity_is_allowed() {
    const SRC: &str = r#"package p;
public class A {
    public void size(int k) { }
    public void count() { }
}
"#;
    let p = Project::new(&[("p/A.java", SRC)]);
    let plan = p
        .rename("p/A.java", at(SRC, "count"), "size")
        .expect("a plan");
    assert_eq!(plan.blocked, None);
}

#[test]
fn a_type_rename_onto_a_sibling_in_the_same_package_is_blocked() {
    const A: &str = "package p;\npublic class A { }\n";
    const B: &str = "package p;\npublic class B { }\n";
    let p = Project::new(&[("p/A.java", A), ("p/B.java", B)]);
    let plan = p
        .rename("p/A.java", at(A, "class A") + 6, "B")
        .expect("a plan");
    assert!(plan.blocked.is_some(), "expected a refusal");
}

#[test]
fn a_local_rename_that_captures_a_name_in_scope_is_blocked() {
    const SRC: &str = r#"package p;
public class A {
    private int total;
    void run() {
        int n = 1;
        System.out.println(total + n);
    }
}
"#;
    let p = Project::new(&[("p/A.java", SRC)]);
    let plan = p
        .rename("p/A.java", at(SRC, "int n = 1") + 4, "total")
        .expect("a plan");
    assert!(plan.blocked.is_some(), "expected a refusal");
}

/// Java keeps variables and methods in separate namespaces: `int size = size();` is ordinary code,
/// so a method of that name must not block the rename.
#[test]
fn a_local_rename_onto_a_method_name_is_allowed() {
    const SRC: &str = r#"package p;
public class A {
    int size() { return 1; }
    void run() {
        int n = size();
    }
}
"#;
    let p = Project::new(&[("p/A.java", SRC)]);
    let plan = p
        .rename("p/A.java", at(SRC, "int n = size") + 4, "size")
        .expect("a plan");
    assert_eq!(plan.blocked, None);
}

/// The override family has to find EVERY subtype. It used to be searched by scanning the project's
/// simple→binary type map, which keeps ONE binary per simple name — so a project with two nested
/// `Builder` classes offered one of them, and the other override was never planned. Guava has
/// dozens, and this was its single largest source of broken builds after a rename.
#[test]
fn an_override_family_finds_same_named_nested_types_in_different_outers() {
    const API: &str = "package p;\npublic interface Api {\n    void build();\n}\n";
    const ONE: &str = r#"package p;
public class One {
    public static class Builder implements Api {
        public void build() { }
    }
}
"#;
    const TWO: &str = r#"package p;
public class Two {
    public static class Builder implements Api {
        public void build() { }
    }
}
"#;
    let p = Project::new(&[
        ("p/Api.java", API),
        ("p/One.java", ONE),
        ("p/Two.java", TWO),
    ]);
    let plan = p
        .rename("p/Api.java", at(API, "build"), "assemble")
        .expect("a plan");
    let touched: Vec<&str> = plan.files.iter().map(|f| f.file.as_str()).collect();
    assert!(touched.contains(&"p/One.java"), "{touched:?}");
    assert!(
        touched.contains(&"p/Two.java"),
        "both overrides must move: {touched:?}"
    );
}

/// A superclass method that satisfies an interface **on behalf of a subclass**. `Base.step()` is
/// not an override of anything `Base` itself implements — the connection runs DOWN to `Impl` and
/// then UP to `Contract`, and a family built by walking up from the owner and then down from there
/// never crosses it.
///
/// Commons Collections is built this way: `AbstractEmptyIterator.hasPrevious()` implements
/// `OrderedIterator.hasPrevious()` for `EmptyOrderedIterator`. Renaming one side alone leaves a
/// class that no longer implements the interface it declares.
#[test]
fn an_override_family_crosses_down_then_up() {
    const CONTRACT: &str = "package p;\npublic interface Contract {\n    boolean step();\n}\n";
    const BASE: &str = r#"package p;
public class Base {
    public boolean step() { return true; }
}
"#;
    const IMPL: &str = "package p;\npublic class Impl extends Base implements Contract { }\n";

    let p = Project::new(&[
        ("p/Contract.java", CONTRACT),
        ("p/Base.java", BASE),
        ("p/Impl.java", IMPL),
    ]);
    let plan = p
        .rename("p/Base.java", at(BASE, "boolean step") + 8, "advance")
        .expect("a plan");
    let touched: Vec<&str> = plan.files.iter().map(|f| f.file.as_str()).collect();
    assert!(
        touched.contains(&"p/Contract.java"),
        "the interface it satisfies must move with it: {touched:?}"
    );
}
