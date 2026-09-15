//! Members of NESTED types — rename and safe delete stay inside the type that declares them.
//!
//! A nested type's file is named after its outer type and may declare a second nested type of the
//! same simple name in another outer (`Outer.Inner`, `Other.Inner`). Every search that located the
//! owner by its simple name alone edited — or deleted — both.

mod common;
use bennu_intel::prelude::{Edit, EditReason};
use common::{at, at_last, Project};

const OUTER: &str = r#"package p;
public class Outer {
    public static class Inner {
        public int limit = 3;
        public void run() { }
    }
    public class Live {
        public void tick() { }
    }
    public enum Mode {
        ON;
        public void apply() { }
    }
    public record Pair(int left) {
        public int twice() { return left * 2; }
    }
}

class Other {
    static class Inner {
        int limit = 4;
        void run() { }
    }
}
"#;

const USE: &str = r#"package p;
public class Use {
    void go(Outer.Inner inner, Outer.Live live, Outer.Mode mode, Outer.Pair pair) {
        inner.run();
        int n = inner.limit;
        live.tick();
        mode.apply();
        pair.twice();
    }
    void other(Other.Inner o) {
        o.run();
        int m = o.limit;
    }
}
"#;

fn project() -> Project {
    Project::new(&[("p/Outer.java", OUTER), ("p/Use.java", USE)])
}

/// Where `needle` puts the name that follows `prefix` in `src`.
fn name_at(src: &str, needle: &str, prefix: &str) -> usize {
    at(src, needle) + prefix.len()
}

fn declaration_starts(edits: &[Edit]) -> Vec<usize> {
    edits
        .iter()
        .filter(|e| e.file == "p/Outer.java" && e.reason == EditReason::Declaration)
        .map(|e| e.start)
        .collect()
}

fn edited(edits: &[Edit], file: &str, start: usize) -> bool {
    edits.iter().any(|e| e.file == file && e.start == start)
}

/// `Other.Inner.run` — the namesake that must never move.
fn other_run() -> usize {
    at_last(OUTER, "void run()") + "void ".len()
}

#[test]
fn a_nested_method_renamed_from_its_call_moves_only_its_own_declaration() {
    let p = project();
    let edits = p.rename_edits("p/Use.java", name_at(USE, "inner.run", "inner."), "execute");
    assert_eq!(
        declaration_starts(&edits),
        vec![name_at(OUTER, "public void run", "public void ")],
        "{edits:?}"
    );
    assert!(edited(&edits, "p/Use.java", name_at(USE, "inner.run", "inner.")));
    assert!(!edited(&edits, "p/Outer.java", other_run()), "Other.Inner.run moved: {edits:?}");
    assert!(!edited(&edits, "p/Use.java", name_at(USE, "o.run", "o.")), "its call moved: {edits:?}");
}

#[test]
fn a_nested_method_renamed_from_its_declaration_moves_only_itself() {
    let p = project();
    let decl = name_at(OUTER, "public void run", "public void ");
    let edits = p.rename_edits("p/Outer.java", decl, "execute");
    assert_eq!(declaration_starts(&edits), vec![decl], "{edits:?}");
    assert!(edited(&edits, "p/Use.java", name_at(USE, "inner.run", "inner.")), "{edits:?}");
    assert!(!edited(&edits, "p/Outer.java", other_run()), "Other.Inner.run moved: {edits:?}");
}

#[test]
fn a_field_of_a_static_nested_class_is_renamed_in_its_own_type_only() {
    let p = project();
    let edits = p.rename_edits("p/Use.java", name_at(USE, "inner.limit", "inner."), "max");
    assert_eq!(
        declaration_starts(&edits),
        vec![name_at(OUTER, "public int limit", "public int ")],
        "{edits:?}"
    );
    let other_limit = at_last(OUTER, "int limit") + "int ".len();
    assert!(!edited(&edits, "p/Outer.java", other_limit), "Other.Inner.limit moved: {edits:?}");
    assert!(!edited(&edits, "p/Use.java", name_at(USE, "o.limit", "o.")), "{edits:?}");
}

/// An inner (non-static) class, an enum and a record nested in a class: each member's declaration
/// is found inside its own nested type when renamed from a call in another file.
#[test]
fn members_of_inner_classes_enums_and_records_are_renamed_at_their_declaration() {
    let p = project();
    for (call, call_prefix, decl, decl_prefix) in [
        ("live.tick", "live.", "public void tick", "public void "),
        ("mode.apply", "mode.", "public void apply", "public void "),
        ("pair.twice", "pair.", "public int twice", "public int "),
    ] {
        let edits = p.rename_edits("p/Use.java", name_at(USE, call, call_prefix), "renamed");
        assert_eq!(
            declaration_starts(&edits),
            vec![name_at(OUTER, decl, decl_prefix)],
            "`{call}`: {edits:?}"
        );
    }
}

#[test]
fn a_nested_type_rename_edits_its_own_declaration_not_a_namesake() {
    let p = project();
    let decl = name_at(OUTER, "public static class Inner", "public static class ");
    let edits = p.rename_edits("p/Outer.java", decl, "Worker");
    assert_eq!(declaration_starts(&edits), vec![decl], "{edits:?}");
    let other = at_last(OUTER, "static class Inner") + "static class ".len();
    assert!(!edited(&edits, "p/Outer.java", other), "Other.Inner renamed: {edits:?}");
}

#[test]
fn safe_delete_from_a_call_finds_the_nested_declaration_and_its_usages() {
    let p = project();
    let plan = p
        .safe_delete("p/Use.java", name_at(USE, "inner.run", "inner."))
        .expect("a safe-delete plan");
    assert_eq!(plan.file, "p/Outer.java", "the declaration lives in the outer type's file");
    let removed = &OUTER[plan.start..plan.end];
    assert!(removed.contains("public void run()"), "removed {removed:?}");
    assert!(plan.start > at(OUTER, "class Inner"), "inside Outer.Inner");
    assert!(plan.end <= at(OUTER, "public class Live"), "and nothing past it: {removed:?}");
    let call = name_at(USE, "inner.run", "inner.");
    assert!(
        plan.usages.iter().any(|u| u.file == "p/Use.java" && u.start == call),
        "the call is a usage: {:?}",
        plan.usages
    );
    let other_call = name_at(USE, "o.run", "o.");
    assert!(
        !plan.usages.iter().any(|u| u.start == other_call),
        "Other.Inner's call is not: {:?}",
        plan.usages
    );
}

#[test]
fn safe_delete_from_the_declaration_stays_in_the_nested_type() {
    let p = project();
    let plan = p
        .safe_delete("p/Outer.java", name_at(OUTER, "public void run", "public void "))
        .expect("a safe-delete plan");
    assert!(plan.end <= at(OUTER, "public class Live"), "{:?}", &OUTER[plan.start..plan.end]);
    assert!(!plan.usages.is_empty(), "the call in Use.java still needs it");
    assert!(!plan.is_safe());
}
