//! Ghost text category — **the next `case` of an enum `switch`**.
//!
//! Ghost text is drawn inline, ahead of the caret, where it reads like text that is already there.
//! Being wrong costs trust rather than a keystroke, so these tests are about the one case where the
//! proposal is a fact — a closed list, in a declared order, with some of it already written — and
//! just as much about every case where it must stay silent.
//!
//! The project here is real: the enum is declared in its own file and reaches the switch through the
//! index, exactly as it does in the editor.

mod common;
use common::*;

const COLOR: &str = "package shop;\npublic enum Color { RED, GREEN, BLUE }\n";

/// `Paint.java` built from `marked` (the `¶` marks the caret and is removed), with the enum beside
/// it; returns the project and the caret's offset.
fn paint(marked: &str) -> (Project, usize) {
    let offset = marked.find('¶').expect("a caret marker");
    let source = marked.replacen('¶', "", 1);
    (Project::new(&[("Color.java", COLOR), ("Paint.java", &source)]), offset)
}

/// A switch over a `Color` with `arms` as its body.
fn switch(arms: &str) -> String {
    format!(
        "package shop;\npublic class Paint {{\n    void run() {{}}\n    void m(Color c) {{\n        switch (c) {{\n{arms}\n        }}\n    }}\n}}\n"
    )
}

fn insert_at(marked: &str) -> Option<String> {
    let (p, offset) = paint(marked);
    p.next_case_hint("Paint.java", offset).map(|h| h.insert)
}

const RED_COLON: &str = "            case RED:\n                run();\n                break;\n";
const RED_ARROW: &str = "            case RED -> run();\n";

// ── What is proposed ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_finished_arm_is_followed_by_the_next_constant() {
    assert_eq!(insert_at(&switch(&format!("{RED_COLON}            ¶"))).as_deref(), Some("case GREEN:"));
}

#[test]
fn the_arrow_style_is_the_one_already_in_the_switch() {
    assert_eq!(insert_at(&switch(&format!("{RED_ARROW}            ¶"))).as_deref(), Some("case GREEN ->"));
}

#[test]
fn constants_are_taken_in_declaration_order_skipping_what_is_covered() {
    let red_green = "            case RED -> a();\n            case GREEN -> b();\n            ¶";
    assert_eq!(insert_at(&switch(red_green)).as_deref(), Some("case BLUE ->"));
    let red_blue = "            case RED -> a();\n            case BLUE -> b();\n            ¶";
    assert_eq!(insert_at(&switch(red_blue)).as_deref(), Some("case GREEN ->"));
}

/// The caret on a truly empty line still gets an aligned arm.
#[test]
fn an_empty_line_gets_the_arms_indentation() {
    let (p, offset) = paint(&switch(&format!("{RED_COLON}¶")));
    let hint = p.next_case_hint("Paint.java", offset).expect("a hint");
    assert_eq!(hint.insert, "            case GREEN:");
    assert_eq!((hint.replace_start, hint.replace_end), (offset, offset));
}

// ── Where it stays quiet ─────────────────────────────────────────────────────────────────────

#[test]
fn nothing_when_every_constant_is_covered() {
    let all = "            case RED -> a();\n            case GREEN -> b();\n            case BLUE -> c();\n            ¶";
    assert_eq!(insert_at(&switch(all)), None);
}

#[test]
fn nothing_once_there_is_a_default() {
    let arms = "            case RED -> a();\n            default -> b();\n            ¶";
    assert_eq!(insert_at(&switch(arms)), None);
}

/// `case RED:` on its own is the first half of `case RED: case GREEN:`, and an arm whose body is
/// still being written is not an invitation to start another.
#[test]
fn nothing_after_an_arm_that_is_not_finished() {
    assert_eq!(insert_at(&switch("            case RED:\n            ¶")), None);
    assert_eq!(insert_at(&switch("            case RED:\n                run();\n            ¶")), None);
}

#[test]
fn nothing_when_the_line_is_not_blank() {
    assert_eq!(insert_at(&switch(&format!("{RED_ARROW}            ca¶"))), None);
    assert_eq!(insert_at(&switch(&format!("{RED_ARROW}            ¶run();"))), None);
}

#[test]
fn nothing_for_a_switch_that_is_not_over_an_enum() {
    let marked = "package shop;\npublic class Paint {\n    void m(int n) {\n        switch (n) {\n            case 1 -> a();\n            ¶\n        }\n    }\n}\n";
    let (p, offset) = paint(marked);
    assert!(p.next_case_hint("Paint.java", offset).is_none());
}

#[test]
fn nothing_for_a_selector_that_does_not_resolve() {
    let marked = "package shop;\npublic class Paint {\n    void m(Mystery c) {\n        switch (c) {\n            case RED -> a();\n            ¶\n        }\n    }\n}\n";
    let (p, offset) = paint(marked);
    assert!(p.next_case_hint("Paint.java", offset).is_none());
}

#[test]
fn nothing_outside_a_switch() {
    let marked = "package shop;\npublic class Paint {\n    void m(Color c) {\n        run();\n        ¶\n    }\n}\n";
    let (p, offset) = paint(marked);
    assert!(p.next_case_hint("Paint.java", offset).is_none());
}
