//! Go-to on a member call split across lines from its receiver.
//!
//! Real code (and reformatters) put the `.method(...)` on a NEW line relative to the receiver,
//! especially in fluent chains:
//!
//! ```java
//! stepper
//!     .add_step("a", "b", "c")
//!     .add_step("d", "e", "f");
//! ```
//!
//! Go-to on such a `.method` must resolve exactly like the single-line form — the whitespace /
//! newline between the receiver and the `.` must not defeat receiver-type inference.

mod common;
use common::*;

fn fluent() -> Project {
    Project::new(&[
        (
            "Fluent.java",
            "package w;\n\
             public class Fluent {\n\
             \x20   public Fluent add_step(String a, String b, String c) { return this; }\n\
             \x20   public int size() { return 0; }\n\
             }\n",
        ),
        (
            "Helper.java",
            "package w;\n\
             public class Helper {\n\
             \x20   private Fluent stepper = new Fluent();\n\
             \x20   public void build() {\n\
             \x20       stepper\n\
             \x20           .add_step(\"a\", \"b\", \"c\");\n\
             \x20   }\n\
             \x20   public void chain() {\n\
             \x20       stepper\n\
             \x20           .add_step(\"a\", \"b\", \"c\")\n\
             \x20           .add_step(\"d\", \"e\", \"f\");\n\
             \x20   }\n\
             \x20   public int field_nl() {\n\
             \x20       return stepper\n\
             \x20           .size();\n\
             \x20   }\n\
             }\n",
        ),
    ])
}

#[test]
fn goto_method_on_new_line_from_receiver() {
    let p = fluent();
    let s = p.source("Helper.java").to_string();
    // The `.add_step` inside build(), on its own line after `stepper`.
    let off = at(&s, ".add_step(\"a\", \"b\", \"c\");") + ".".len();
    let d = p
        .goto("Helper.java", off)
        .expect("go-to resolves across the newline");
    assert_eq!(d.file, "Fluent.java");
    assert_eq!(d.label, "method w.Fluent.add_step()");
}

#[test]
fn goto_first_call_of_multiline_chain() {
    let p = fluent();
    let s = p.source("Helper.java").to_string();
    // The first `.add_step` of the chain() fluent chain.
    let off = at(&s, ".add_step(\"a\", \"b\", \"c\")\n") + ".".len();
    let d = p
        .goto("Helper.java", off)
        .expect("first chained call resolves");
    assert_eq!(d.file, "Fluent.java");
    assert_eq!(d.label, "method w.Fluent.add_step()");
}

#[test]
fn goto_second_call_of_multiline_chain() {
    let p = fluent();
    let s = p.source("Helper.java").to_string();
    // The second `.add_step` — its receiver is the first call's return value, also on a prior line.
    let off = at(&s, ".add_step(\"d\", \"e\", \"f\")") + ".".len();
    let d = p
        .goto("Helper.java", off)
        .expect("second chained call resolves");
    assert_eq!(d.file, "Fluent.java");
    assert_eq!(d.label, "method w.Fluent.add_step()");
}

#[test]
fn goto_field_style_method_on_new_line() {
    let p = fluent();
    let s = p.source("Helper.java").to_string();
    // `stepper\n  .size()` — a no-arg call split across lines.
    let off = at(&s, ".size()") + ".".len();
    let d = p
        .goto("Helper.java", off)
        .expect("newline no-arg call resolves");
    assert_eq!(d.file, "Fluent.java");
    assert_eq!(d.label, "method w.Fluent.size()");
}

#[test]
fn find_usages_counts_multiline_calls() {
    // All three add_step call sites (one in build, two in chain) are bucketed regardless of layout.
    let p = fluent();
    let f = p.source("Fluent.java").to_string();
    let off = at(&f, "add_step(String") + 0;
    assert_eq!(
        p.usage_count("Fluent.java", off),
        3,
        "multiline calls are still indexed"
    );
}
