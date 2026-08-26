//! Classes declared inside a method body — **local classes**, legal since Java 1.1 (local
//! interfaces, enums and records arrived in Java 16, JEP 395).
//!
//! Nothing else in a method body is a declaration the symbol walk cares about, so it read
//! signatures and never descended into bodies: a local class was invisible end to end — no index
//! entry, no go-to, no find-usages, and every use of it flagged as an unknown type.

mod common;
use common::{at, at_last, Project};

const SRC: &str = r#"package p;
public class Outer {
    int run() {
        class Helper {
            int count = 2;
            int twice() { return count * 2; }
        }
        Helper h = new Helper();
        return h.twice();
    }
}
"#;

fn project() -> Project {
    Project::new(&[("p/Outer.java", SRC)])
}

#[test]
fn go_to_the_local_class_from_its_use() {
    let p = project();
    let src = p.source("p/Outer.java");
    let label = p.goto_label("p/Outer.java", at(src, "Helper h = new Helper()"));
    assert_eq!(label.as_deref(), Some("class p.Outer.Helper"));
}

#[test]
fn go_to_a_method_of_the_local_class() {
    let p = project();
    let src = p.source("p/Outer.java");
    let off = at(src, "h.twice()") + "h.".len();
    assert_eq!(
        p.goto_label("p/Outer.java", off).as_deref(),
        Some("method p.Outer.Helper.twice()"),
    );
}

#[test]
fn find_usages_of_the_local_class_sees_both_mentions() {
    let p = project();
    let src = p.source("p/Outer.java");
    // From the declaration: `Helper h` and `new Helper()`.
    assert_eq!(
        p.usage_count("p/Outer.java", at(src, "class Helper") + "class ".len()),
        2
    );
}

#[test]
fn renaming_the_local_class_moves_every_mention() {
    let p = project();
    let src = p.source("p/Outer.java");
    let edits = p.rename_edits(
        "p/Outer.java",
        at(src, "class Helper") + "class ".len(),
        "Aide",
    );
    assert!(
        edits.iter().any(|e| e.reason.label() == "declaration"),
        "declaration edit"
    );
    assert_eq!(edits.len(), 3, "declaration + the two uses: {edits:?}");
    assert!(edits.iter().all(|e| e.new_text == "Aide"));
}

#[test]
fn renaming_a_member_of_a_local_class_reaches_its_use() {
    let p = project();
    let src = p.source("p/Outer.java");
    let edits = p.rename_edits(
        "p/Outer.java",
        at(src, "int twice()") + "int ".len(),
        "doubled",
    );
    let call = at_last(src, "h.twice()") + "h.".len();
    assert!(
        edits.iter().any(|e| e.start == call),
        "the call was not renamed: {edits:?}"
    );
}

/// A local class does not move its file: the file is named after the top-level type.
#[test]
fn renaming_a_local_class_leaves_the_file_alone() {
    let p = project();
    let src = p.source("p/Outer.java");
    let plan = p
        .rename(
            "p/Outer.java",
            at(src, "class Helper") + "class ".len(),
            "Aide",
        )
        .expect("a plan");
    assert!(plan.file_rename.is_none());
}

// ── anonymous classes ─────────────────────────────────────────────────────────────────────────

const ANON: &str = r#"package p;
public class Host {
    int size = 1;
    Runnable field = new Runnable() {
        int tag = 7;
        public void run() { int t = tag; }
    };
    void install() {
        Runnable r = new Runnable() {
            public void run() { }
        };
    }
    void run() { }
}
"#;

fn anon() -> Project {
    Project::new(&[("p/Host.java", ANON)])
}

/// The point of giving an anonymous body an identity: what it declares belongs to IT. Without one,
/// the anonymous `run()` counted as a use of `Host.run()` — a method nobody had called.
#[test]
fn an_anonymous_method_is_not_a_use_of_the_outer_classes_method() {
    let p = anon();
    let src = p.source("p/Host.java");
    let outer_run = at_last(src, "void run() { }") + "void ".len();
    assert_eq!(
        p.goto_label("p/Host.java", outer_run).as_deref(),
        Some("method p.Host.run()"),
        "the outer method should still be its own",
    );
    assert_eq!(
        p.usage_count("p/Host.java", outer_run),
        0,
        "nobody calls Host.run()"
    );
}

/// A field read inside an anonymous body resolves against the anonymous class, not the host.
#[test]
fn a_field_of_the_anonymous_class_resolves_to_it() {
    let p = anon();
    let src = p.source("p/Host.java");
    let read = at(src, "int t = tag;") + "int t = ".len();
    assert_eq!(
        p.goto_label("p/Host.java", read).as_deref(),
        Some("field p.Host.1.tag")
    );
}

/// Numbered by source order within the enclosing type, the way javac names them.
#[test]
fn anonymous_classes_are_numbered_in_source_order() {
    let p = anon();
    let src = p.source("p/Host.java");
    let read = at(src, "int t = tag;") + "int t = ".len();
    // The field initializer's anonymous class comes first in the file, so it is `1`.
    let label = p.goto_label("p/Host.java", read).expect("resolved");
    assert!(
        label.ends_with("Host.1.tag"),
        "expected the first anonymous class: {label}"
    );
}
