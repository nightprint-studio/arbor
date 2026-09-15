//! Rename category — the rename PLAN (preview) + apply (flatten).
//!
//! Rename reuses the same caret classifier as go-to / find-usages, then rewrites the declaration
//! + every recorded use site: a local within its scope (current file only), a member across its
//! bucketed accesses, a type across its references + `new` expressions (+ import rewrites). Every
//! edit carries the exact `old` text at its span (the FE's stale-buffer guard), which these tests
//! assert holds against the buffer. Renaming one symbol must never touch an unrelated same-named
//! one (a field vs a local of the same name).

mod common;
use common::*;

fn proj() -> Project {
    Project::new(&[
        (
            "Counter.java",
            "package app;\n\
             public class Counter {\n\
             \x20   private int total;\n\
             \x20   public int add(int delta) {\n\
             \x20       int local = delta + 1;\n\
             \x20       this.total = this.total + local;\n\
             \x20       return this.total;\n\
             \x20   }\n\
             \x20   public int get() { return this.total; }\n\
             }\n",
        ),
        (
            "UseCounter.java",
            "package app;\n\
             public class UseCounter {\n\
             \x20   public int use(Counter c) {\n\
             \x20       return c.add(2) + c.get();\n\
             \x20   }\n\
             \x20   public Counter make() { return new Counter(); }\n\
             \x20   public int shadow() {\n\
             \x20       int total = 9;\n\
             \x20       return total;\n\
             \x20   }\n\
             }\n",
        ),
    ])
}

/// Every edit's declared `old` text must equal the current buffer slice at its span — the
/// stale-buffer guard invariant the FE relies on. Asserts it for a whole plan.
fn assert_old_matches_buffer(p: &Project, plan: &bennu_intel::prelude::RenamePlan) {
    for fe in &plan.files {
        let src = p.source(&fe.file);
        for e in &fe.edits {
            assert_eq!(
                &src[e.start..e.end],
                e.old,
                "edit.old must match the buffer at [{}, {}) in {}",
                e.start,
                e.end,
                fe.file
            );
        }
    }
}

// ── Local ────────────────────────────────────────────────────────────────────────────────────

#[test]
fn rename_local_touches_only_its_scope() {
    let p = proj();
    let s = p.source("Counter.java").to_string();
    let off = at(&s, "int local =") + "int ".len();
    let plan = p.rename("Counter.java", off, "acc").expect("rename local");
    assert_eq!(plan.old_name, "local");
    assert_eq!(plan.new_name, "acc");
    assert_eq!(plan.target_label, "local `local`");
    // Decl + the single use `+ local` = 2 edits, all in Counter.java.
    assert_eq!(plan.files.len(), 1, "a local rename stays in one file");
    assert_eq!(plan.files[0].file, "Counter.java");
    assert_eq!(plan.total_edits(), 2, "declarator + one use site");
    assert!(plan.files[0].edits.iter().all(|e| e.new_text == "acc"));
    assert_old_matches_buffer(&p, &plan);
}

#[test]
fn rename_local_does_not_touch_field_of_same_name() {
    // `shadow()`'s local `total` must not drag in the field `Counter.total` (different symbol).
    let p = proj();
    let s = p.source("UseCounter.java").to_string();
    let off = at(&s, "int total = 9") + "int ".len();
    let plan = p
        .rename("UseCounter.java", off, "n")
        .expect("rename shadow local");
    assert_eq!(plan.target_label, "local `total`");
    assert_eq!(plan.files.len(), 1);
    assert_eq!(
        plan.files[0].file, "UseCounter.java",
        "stays in the declaring file"
    );
    assert_eq!(plan.total_edits(), 2, "decl + `return total` only");
    assert_old_matches_buffer(&p, &plan);
}

// ── Field ────────────────────────────────────────────────────────────────────────────────────

#[test]
fn rename_field_rewrites_qualified_accesses() {
    let p = proj();
    let s = p.source("Counter.java").to_string();
    let off = at(&s, "int total;") + "int ".len();
    let plan = p
        .rename("Counter.java", off, "count")
        .expect("rename field");
    assert_eq!(plan.old_name, "total");
    assert_eq!(plan.target_label, "field app.Counter.total");
    // Decl + the `this.total` accesses (>= 3): 2 in add(), 1 in get().
    assert!(
        plan.total_edits() >= 4,
        "field decl + several this.total accesses, got {}",
        plan.total_edits()
    );
    for fe in &plan.files {
        assert!(fe.edits.iter().all(|e| e.new_text == "count"));
    }
    assert_old_matches_buffer(&p, &plan);
}

#[test]
fn rename_field_does_not_touch_unrelated_local_total() {
    // Renaming the FIELD `Counter.total` must not edit `UseCounter.shadow()`'s local `total`.
    let p = proj();
    let s = p.source("Counter.java").to_string();
    let off = at(&s, "int total;") + "int ".len();
    let plan = p
        .rename("Counter.java", off, "count")
        .expect("rename field");
    let us = p.source("UseCounter.java");
    let local_decl = at(us, "int total = 9") + "int ".len();
    for fe in &plan.files {
        if fe.file == "UseCounter.java" {
            for e in &fe.edits {
                assert!(
                    !(e.start <= local_decl && local_decl < e.end),
                    "field rename must not touch the unrelated local `total`"
                );
            }
        }
    }
}

// ── Method ───────────────────────────────────────────────────────────────────────────────────

#[test]
fn rename_method_rewrites_call_sites_cross_file() {
    let p = proj();
    let s = p.source("Counter.java").to_string();
    let off = at(&s, "int add(int delta)") + "int ".len();
    let plan = p
        .rename("Counter.java", off, "increment")
        .expect("rename method");
    assert_eq!(plan.old_name, "add");
    assert_eq!(plan.target_label, "method app.Counter.add()");
    // Decl (Counter.java) + call site `c.add(2)` (UseCounter.java).
    let files: Vec<&str> = plan.files.iter().map(|f| f.file.as_str()).collect();
    assert!(
        files.contains(&"Counter.java"),
        "declaring file edited, got {files:?}"
    );
    assert!(
        files.contains(&"UseCounter.java"),
        "caller file edited, got {files:?}"
    );
    for fe in &plan.files {
        assert!(fe.edits.iter().all(|e| e.new_text == "increment"));
    }
    assert_old_matches_buffer(&p, &plan);
}

// ── Type ─────────────────────────────────────────────────────────────────────────────────────

#[test]
fn rename_type_rewrites_references_and_new() {
    let p = proj();
    let s = p.source("Counter.java").to_string();
    let off = at(&s, "class Counter") + "class ".len();
    let plan = p.rename("Counter.java", off, "Tally").expect("rename type");
    assert_eq!(plan.old_name, "Counter");
    assert_eq!(plan.target_label, "type app.Counter");
    // Decl + `Counter c` param + `Counter make()` return + `new Counter()` in UseCounter.
    let use_edits: usize = plan
        .files
        .iter()
        .filter(|f| f.file == "UseCounter.java")
        .map(|f| f.edits.len())
        .sum();
    assert!(
        use_edits >= 3,
        "type used as param + return + new(), got {use_edits}"
    );
    for fe in &plan.files {
        assert!(fe.edits.iter().all(|e| e.new_text == "Tally"));
    }
    assert_old_matches_buffer(&p, &plan);
}

// ── Apply + negatives ────────────────────────────────────────────────────────────────────────

#[test]
fn rename_apply_flattens_every_file() {
    let p = proj();
    let s = p.source("Counter.java").to_string();
    let off = at(&s, "int add(int delta)") + "int ".len();
    let plan = p
        .rename("Counter.java", off, "increment")
        .expect("rename method");
    let flat = p.rename_edits("Counter.java", off, "increment");
    assert_eq!(
        flat.len(),
        plan.total_edits(),
        "apply flattens exactly the plan's edits"
    );
    assert!(!flat.is_empty());
}

#[test]
fn rename_on_keyword_is_none() {
    let p = proj();
    let s = p.source("Counter.java").to_string();
    assert!(
        p.rename("Counter.java", at(&s, "return this.total;"), "x")
            .is_none(),
        "keyword not renameable"
    );
}

#[test]
fn rename_on_literal_is_none() {
    let p = proj();
    let s = p.source("Counter.java").to_string();
    assert!(
        p.rename("Counter.java", at(&s, "delta + 1") + "delta + ".len(), "x")
            .is_none(),
        "literal not renameable"
    );
}

#[test]
fn rename_from_use_site_matches_from_decl() {
    // Renaming a method from a call site yields the same plan as from its declaration.
    let p = proj();
    let cs = p.source("UseCounter.java").to_string();
    let from_use = p.rename(
        "UseCounter.java",
        at(&cs, "c.add(2)") + "c.".len(),
        "increment",
    );
    let ds = p.source("Counter.java").to_string();
    let from_decl = p.rename(
        "Counter.java",
        at(&ds, "int add(int delta)") + "int ".len(),
        "increment",
    );
    let (u, d) = (from_use.expect("from use"), from_decl.expect("from decl"));
    assert_eq!(
        u.total_edits(),
        d.total_edits(),
        "same symbol → same edit count"
    );
    assert_eq!(u.target_label, d.target_label);
}

// ── record components reached through a lambda ──────────────────────────────────

/// The shape real code has: a nested `record`, and its accessor called on a **lambda parameter**
/// whose type is only known by following a generic chain — `Box<Failure>` → `stream()` →
/// `Seq<Failure>` → `map(Fn<T,R>)` → `T`.
///
/// Modelled with project types on purpose. The harness has no JDK, so `List.stream()` cannot be
/// resolved here — but the *inference* being tested is generic substitution through method return
/// types and functional-interface parameters, which project types exercise identically. Testing it
/// against an explicitly typed variable instead (`Failure f`) is what made an earlier fix look
/// correct while the real code stayed broken.
fn record_lambda_project() -> Project {
    Project::new(&[
        (
            "Fn.java",
            "package app;\npublic interface Fn<T, R> {\n    R apply(T t);\n}\n",
        ),
        (
            "Seq.java",
            "package app;\n\
             public class Seq<T> {\n\
             \x20   public <R> Seq<R> map(Fn<T, R> f) { return null; }\n\
             }\n",
        ),
        (
            "Box.java",
            "package app;\n\
             public class Box<T> {\n\
             \x20   public Seq<T> stream() { return null; }\n\
             }\n",
        ),
        (
            "Outer.java",
            "package app;\n\
             public class Outer {\n\
             \x20   private record Failure(String source_path) {}\n\
             \x20   Seq<String> show(Box<Failure> failures) {\n\
             \x20       return failures.stream().map(failure -> failure.source_path());\n\
             \x20   }\n\
             }\n",
        ),
    ])
}

#[test]
fn the_index_records_an_accessor_called_on_a_lambda_parameter() {
    // Asserted separately from the rename so a failure says WHICH half is broken: if the index
    // never keyed this call, no rename could ever have found it and the problem is type inference,
    // not the rename plan.
    let p = record_lambda_project();
    let s = p.source("Outer.java").to_string();
    let off = at(&s, "failure.source_path()") + "failure.".len();
    let usages = p.find_usages("Outer.java", off);
    assert!(
        usages.is_some_and(|u| !u.usages.is_empty()),
        "the accessor call on a lambda parameter must be indexed as a use"
    );
}

#[test]
fn renaming_a_record_component_reaches_an_accessor_on_a_lambda_parameter() {
    let p = record_lambda_project();
    let s = p.source("Outer.java").to_string();
    let off = at(&s, "String source_path)") + "String ".len();
    let edits = p.rename_edits("Outer.java", off, "sourcePath");
    assert!(!edits.is_empty(), "the component must be renameable at all");

    let accessor = at(&s, "failure.source_path()") + "failure.".len();
    assert!(
        edits
            .iter()
            .any(|e| e.file == "Outer.java" && e.start == accessor),
        "the accessor call must be renamed, got {edits:?}"
    );
}

#[test]
fn go_to_a_record_accessor_lands_in_the_record_not_in_a_generated_stub() {
    // A record's accessor is synthesized, and its symbol used to carry no span — so anything
    // navigating to it had nowhere in the project to go and fell back to the generated-source
    // view: "go to source_path()" opened a stub of the record instead of the record itself.
    // The component's name in the header IS where it is written.
    let p = record_lambda_project();
    let s = p.source("Outer.java").to_string();
    let off = at(&s, "failure.source_path()") + "failure.".len();
    let d = p.goto("Outer.java", off).expect("the accessor resolves");
    assert_eq!(
        d.file, "Outer.java",
        "it must land in the file that declares the record"
    );
    let component = at(&s, "String source_path)") + "String ".len();
    assert_eq!(
        d.start,
        component,
        "it must land on the component in the header, got {:?}",
        &s[d.start..d.end.min(s.len())]
    );
}

#[test]
fn a_nested_project_type_is_recognised_as_project_code() {
    // The guard behind "go to declaration opened a decompiled stub of my own record":
    // `library_binary` refuses to open a library view for a binary the project declares. If that
    // check says "not mine" for a nested type, the last-resort library jump happily renders the
    // class from `target/classes` — the user's own code, shown as `throw new
    // RuntimeException("compiled code")`.
    let p = record_lambda_project();
    assert!(
        p.is_project_type("app/Outer"),
        "the outer class is obviously project code"
    );
    assert!(
        p.is_project_type("app/Outer/Failure"),
        "a nested type is project code too — source spelling"
    );
    assert!(
        p.is_project_type("app/Outer$Failure"),
        "…and under the JVM spelling, which is what comes back from its own compiled classes"
    );
    assert!(
        !p.is_project_type("java/util/Map"),
        "a real library type is not project code"
    );
}
