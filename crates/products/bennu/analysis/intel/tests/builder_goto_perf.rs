//! Fluent-builder go-to + performance guard.
//!
//! Reproduces the reported `stepper.add_step("a","b","c")` scenarios (the receiver is a project
//! builder — a local, a field, or an inherited builder base) and asserts go-to + find-usages
//! resolve. Also measures single-query latency on a LARGE synthetic file to guard against the
//! per-query re-parse pathology (a method go-to must not take hundreds of ms on a big file).

mod common;
use common::*;
use std::time::Instant;

// ── Correctness: the reported builder call in its several receiver shapes ─────────────────────

fn inherited_builder() -> Project {
    Project::new(&[
        (
            "BaseStepper.java",
            "package w;\n\
             public class BaseStepper {\n\
             \x20   public BaseStepper add_step(String id, String open, String label) { return this; }\n\
             }\n",
        ),
        (
            "WizardStepper.java",
            "package w;\n\
             public class WizardStepper extends BaseStepper {\n\
             }\n",
        ),
        (
            "Helper.java",
            "package w;\n\
             public class Helper {\n\
             \x20   private WizardStepper stepper = new WizardStepper();\n\
             \x20   public void build() {\n\
             \x20       stepper.add_step(\"datiimpresa\", \"stepOpenDatiImpresa\", \"LABEL_DATI_ANAGRAFICI\");\n\
             \x20   }\n\
             }\n",
        ),
    ])
}

#[test]
fn goto_add_step_on_field_receiver_inherited() {
    // `stepper` is a FIELD whose type WizardStepper inherits add_step from BaseStepper.
    let p = inherited_builder();
    let s = p.source("Helper.java").to_string();
    let off = at(&s, "stepper.add_step(") + "stepper.".len();
    let d = p
        .goto("Helper.java", off)
        .expect("go-to resolves the inherited builder method");
    assert_eq!(
        d.file, "BaseStepper.java",
        "add_step is declared on the base builder"
    );
    assert_eq!(d.label, "method w.BaseStepper.add_step()");
}

#[test]
fn find_usages_add_step_inherited_from_subtype_receiver() {
    // The reference walk indexes the `stepper.add_step(...)` call under the SAME owner the query
    // resolves (the declaring base) — otherwise find-usages silently reports nothing.
    let p = inherited_builder();
    let b = p.source("BaseStepper.java").to_string();
    let off = at(&b, "add_step(String") + 0;
    let n = p.usage_count("BaseStepper.java", off);
    assert_eq!(
        n, 1,
        "the one stepper.add_step(...) call is found from the declaration"
    );
}

#[test]
fn goto_add_step_on_local_receiver() {
    let p = Project::new(&[
        (
            "WizardStepper.java",
            "package w;\n\
             public class WizardStepper {\n\
             \x20   public WizardStepper add_step(String id, String open, String label) { return this; }\n\
             }\n",
        ),
        (
            "Helper.java",
            "package w;\n\
             public class Helper {\n\
             \x20   public void build() {\n\
             \x20       WizardStepper stepper = new WizardStepper();\n\
             \x20       stepper.add_step(\"a\", \"b\", \"c\");\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    let s = p.source("Helper.java").to_string();
    let off = at(&s, "stepper.add_step(") + "stepper.".len();
    let d = p
        .goto("Helper.java", off)
        .expect("go-to resolves via the local receiver");
    assert_eq!(d.file, "WizardStepper.java");
    assert_eq!(d.label, "method w.WizardStepper.add_step()");
}

#[test]
fn goto_add_step_chained_return_this() {
    // `stepper.add_step(...).add_step(...)` — the SECOND call's receiver is the first call's
    // return type (WizardStepper, via `return this`). Method-return-type chaining.
    let p = Project::new(&[
        (
            "WizardStepper.java",
            "package w;\n\
             public class WizardStepper {\n\
             \x20   public WizardStepper add_step(String id, String open, String label) { return this; }\n\
             }\n",
        ),
        (
            "Helper.java",
            "package w;\n\
             public class Helper {\n\
             \x20   private WizardStepper stepper = new WizardStepper();\n\
             \x20   public void build() {\n\
             \x20       stepper.add_step(\"a\", \"b\", \"c\").add_step(\"d\", \"e\", \"f\");\n\
             \x20   }\n\
             }\n",
        ),
    ]);
    let s = p.source("Helper.java").to_string();
    // The SECOND add_step (after the `)` of the first call).
    let off = at_last(&s, ".add_step(") + ".".len();
    let d = p
        .goto("Helper.java", off)
        .expect("go-to resolves the chained builder method");
    assert_eq!(d.file, "WizardStepper.java");
    assert_eq!(d.label, "method w.WizardStepper.add_step()");
}

// ── Performance guard: a single query must stay cheap on a LARGE file ─────────────────────────

/// Build a large helper file: `count` methods, each invoking `stepper.add_step(...)`, plus the
/// builder types. Returns the project + the large file's source.
fn big_project(count: usize) -> (Project, String) {
    let mut body = String::from(
        "package w;\n\
         public class Big {\n\
         \x20   private WizardStepper stepper = new WizardStepper();\n",
    );
    for i in 0..count {
        body.push_str(&format!(
            "    public void m{i}() {{ stepper.add_step(\"a{i}\", \"b{i}\", \"c{i}\"); }}\n"
        ));
    }
    body.push_str("}\n");

    let files: Vec<(&str, &str)> = vec![
        (
            "WizardStepper.java",
            "package w;\n\
             public class WizardStepper {\n\
             \x20   public WizardStepper add_step(String id, String open, String label) { return this; }\n\
             }\n",
        ),
        ("Big.java", Box::leak(body.into_boxed_str())),
    ];
    let p = Project::new(&files);
    let s = p.source("Big.java").to_string();
    (p, s)
}

#[test]
fn goto_on_large_file_is_not_pathological() {
    // A ~1500-method file (tens of thousands of lines). A single method go-to should complete
    // well under a second even with the current per-query parses — if this blows up, the caret
    // path has a super-linear pathology worth fixing before it becomes a UI freeze.
    let (p, s) = big_project(1500);
    // Land on the LAST add_step call (worst case for any position-dependent scan).
    let off = at_last(&s, "stepper.add_step(") + "stepper.".len();

    let start = Instant::now();
    let d = p.goto("Big.java", off);
    let elapsed = start.elapsed();

    assert!(d.is_some(), "go-to still resolves on a large file");
    assert_eq!(d.unwrap().file, "WizardStepper.java");
    eprintln!("goto_on_large_file: single go-to took {elapsed:?}");
    // Post-fix a single query parses the buffer ONCE + one symbol extraction (was 3 parses +
    // a re-extract → ~600ms). This threshold catches a regression back to the re-parse path.
    assert!(
        elapsed.as_millis() < 450,
        "a single go-to took {elapsed:?} — that is a per-query pathology, not acceptable for an IDE"
    );
}

#[test]
fn find_usages_on_large_file_is_not_pathological() {
    let (p, s) = big_project(1500);
    let off = at_last(&s, "stepper.add_step(") + "stepper.".len();

    let start = Instant::now();
    let n = p.usage_count("Big.java", off);
    let elapsed = start.elapsed();

    eprintln!("find_usages_on_large_file: found {n} usages in {elapsed:?}");
    assert_eq!(n, 1500, "every stepper.add_step(...) call is bucketed");
    assert!(
        elapsed.as_millis() < 450,
        "find-usages took {elapsed:?} on a large file — pathological"
    );
}

#[test]
fn repeated_gotos_stay_cheap() {
    // Ten consecutive go-tos (simulating a user navigating) must each stay cheap — catches a
    // per-call rebuild / cache-miss that would compound into a freeze.
    let (p, s) = big_project(800);
    let offsets: Vec<usize> = (0..10)
        .map(|i| at(&s, &format!("stepper.add_step(\"a{}\"", i * 50)) + "stepper.".len())
        .collect();

    let start = Instant::now();
    for &off in &offsets {
        let _ = p.goto("Big.java", off);
    }
    let elapsed = start.elapsed();
    eprintln!(
        "repeated_gotos: 10 go-tos took {elapsed:?} (avg {:?})",
        elapsed / 10
    );
    // Old re-parse path: ~3.6s for 10. Post-fix: ~0.7s. This catches a regression to the former.
    assert!(
        elapsed.as_millis() < 1800,
        "10 go-tos took {elapsed:?} — compounding cost"
    );
}
