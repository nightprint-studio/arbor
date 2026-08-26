//! The rename cases reported from a real project: overloads, and members that exist at more than
//! one level of a type hierarchy.
//!
//! These are all "the declaration half is wrong" again — the use sites come from the index and are
//! right; what a rename misses is source it must also edit.

mod common;
use common::{at, Project};

/// Two overloads share one name. They are ONE key in the reference index (overloads collapse), so
/// renaming has to move BOTH declarations, not the first one it finds.
#[test]
fn both_overloads_of_a_method_are_renamed() {
    let p = Project::new(&[(
        "p/Marshal.java",
        "package p;\npublic class Marshal {\n    public <T> T sneaky_unmarshal(Source source, T... values) { return null; }\n    public <T> T sneaky_unmarshal(Source source, Class<T> de) { return null; }\n}\n",
    ), ("p/Source.java", "package p;\npublic class Source {}\n")]);
    let src = p.source("p/Marshal.java");
    let edits = p.rename_edits("p/Marshal.java", at(src, "sneaky_unmarshal"), "sneakyUnmarshal");
    let decls = edits.iter().filter(|e| e.reason.label() == "declaration").count();
    assert_eq!(decls, 2, "only {decls} of the 2 overload declarations were renamed");
}

/// An abstract method and its implementation in a subclass are the same method to a caller.
#[test]
fn an_override_in_a_subclass_is_renamed_with_the_abstract_method() {
    let p = Project::new(&[
        (
            "p/Base.java",
            "package p;\npublic abstract class Base {\n    public abstract void do_work();\n}\n",
        ),
        (
            "p/Impl.java",
            "package p;\npublic class Impl extends Base {\n    @Override\n    public void do_work() { }\n}\n",
        ),
    ]);
    let src = p.source("p/Base.java");
    let edits = p.rename_edits("p/Base.java", at(src, "do_work"), "doWork");
    assert!(
        edits.iter().any(|e| e.file == "p/Impl.java"),
        "the subclass override was not renamed; files: {:?}",
        edits.iter().map(|e| e.file.as_str()).collect::<Vec<_>>()
    );
}

/// Same shape through an interface.
#[test]
fn an_interface_method_renames_its_implementors() {
    let p = Project::new(&[
        (
            "p/Api.java",
            "package p;\npublic interface Api {\n    void do_work();\n}\n",
        ),
        (
            "p/Impl.java",
            "package p;\npublic class Impl implements Api {\n    @Override\n    public void do_work() { }\n}\n",
        ),
    ]);
    let src = p.source("p/Api.java");
    let edits = p.rename_edits("p/Api.java", at(src, "do_work"), "doWork");
    assert!(
        edits.iter().any(|e| e.file == "p/Impl.java"),
        "the implementor was not renamed; files: {:?}",
        edits.iter().map(|e| e.file.as_str()).collect::<Vec<_>>()
    );
}

/// A call whose receiver is typed as the SUBCLASS. The subclass declares its own override, so the
/// call may be keyed to the subclass rather than the base — and then a rename from the base misses
/// an ordinary-looking call site.
#[test]
fn a_call_through_a_subclass_typed_receiver_is_renamed() {
    let p = Project::new(&[
        (
            "p/Base.java",
            "package p;\npublic abstract class Base {\n    public abstract void do_work();\n}\n",
        ),
        (
            "p/Impl.java",
            "package p;\npublic class Impl extends Base {\n    @Override\n    public void do_work() { }\n}\n",
        ),
        (
            "p/Caller.java",
            "package p;\npublic class Caller {\n    void go(Impl impl) {\n        impl.do_work();\n    }\n}\n",
        ),
    ]);
    let src = p.source("p/Base.java");
    let edits = p.rename_edits("p/Base.java", at(src, "do_work"), "doWork");
    assert!(
        edits.iter().any(|e| e.file == "p/Caller.java"),
        "the call through the subclass-typed receiver was not renamed; files: {:?}",
        edits.iter().map(|e| e.file.as_str()).collect::<Vec<_>>()
    );
}

// ── the file a type rename has to take with it ────────────────────────────────────────────────

/// Java ties a public top-level type to its filename, so renaming the type without the file leaves
/// code that does not compile.
#[test]
fn renaming_a_top_level_type_moves_its_file() {
    let p = Project::new(&[("p/Order.java", "package p;\npublic class Order { }\n")]);
    let src = p.source("p/Order.java");
    let plan = p.rename("p/Order.java", at(src, "Order"), "Invoice").expect("a plan");
    let mv = plan.file_rename.expect("the file rename was not proposed");
    assert_eq!(mv.from, "p/Order.java");
    assert_eq!(mv.to, "p/Invoice.java");
}

/// A nested type lives in a file named after its OUTER type, which must stay put.
#[test]
fn renaming_a_nested_type_leaves_the_file_alone() {
    let p = Project::new(&[(
        "p/Order.java",
        "package p;\npublic class Order {\n    public static class Line { }\n}\n",
    )]);
    let src = p.source("p/Order.java");
    let plan = p.rename("p/Order.java", at(src, "Line"), "Row").expect("a plan");
    assert!(plan.file_rename.is_none(), "a nested type must not move its outer type's file");
}

/// A second, non-public type sharing a file is not what the file is named after either.
#[test]
fn a_type_whose_file_is_named_after_something_else_does_not_move_it() {
    let p = Project::new(&[(
        "p/Order.java",
        "package p;\npublic class Order { }\nclass Helper { }\n",
    )]);
    let src = p.source("p/Order.java");
    let plan = p.rename("p/Order.java", at(src, "Helper"), "Support").expect("a plan");
    assert!(plan.file_rename.is_none());
}

/// Renaming a METHOD moves nothing.
#[test]
fn renaming_a_member_never_moves_a_file() {
    let p = Project::new(&[(
        "p/Order.java",
        "package p;\npublic class Order {\n    void do_work() { }\n}\n",
    )]);
    let src = p.source("p/Order.java");
    let plan = p.rename("p/Order.java", at(src, "do_work"), "doWork").expect("a plan");
    assert!(plan.file_rename.is_none());
}

// ── a member the project cannot rename on its own ─────────────────────────────────────────────

/// A method that implements a LIBRARY interface is not free to be renamed: the jar cannot be
/// edited to follow, so renaming only this side leaves a class that no longer implements what it
/// declares. The plan is still produced — the edits are what make the reason legible — but it
/// carries a refusal the caller must honour.
#[test]
fn overriding_a_library_method_blocks_the_rename() {
    // `Api` stands in for the library: `StreamJdk` resolves it, and the project index does not
    // declare it, which is exactly what makes it "not ours" to the engine.
    let p = Project::with_stream_jdk(&[(
        "p/Impl.java",
        "package p;\nimport java.util.function.Function;\npublic class Impl implements Function<String, String> {\n    @Override\n    public String apply(String s) { return s; }\n}\n",
    )]);
    let src = p.source("p/Impl.java");
    let plan = p.rename("p/Impl.java", at(src, "apply"), "run").expect("a plan is still produced");
    let reason = plan.blocked.expect("renaming a library override must be refused");
    assert!(
        reason.contains("java.util.function.Function"),
        "the refusal should name the library type: {reason}"
    );
}

/// The same shape entirely inside the project stays renameable — the refusal must not fire on an
/// ordinary override.
#[test]
fn overriding_a_project_method_is_not_blocked() {
    let p = Project::with_stream_jdk(&[
        ("p/Api.java", "package p;\npublic interface Api {\n    void run();\n}\n"),
        (
            "p/Impl.java",
            "package p;\npublic class Impl implements Api {\n    @Override\n    public void run() { }\n}\n",
        ),
    ]);
    let src = p.source("p/Impl.java");
    let plan = p.rename("p/Impl.java", at(src, "run"), "execute").expect("a plan");
    assert!(plan.blocked.is_none(), "a project-only override is renameable: {:?}", plan.blocked);
}
