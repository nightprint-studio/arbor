//! Hover category — the hover card for the symbol under the caret.
//!
//! Hover shares the caret classifier with go-to / find-usages, then renders a card: a method
//! reports its signature + owning type; a field its type + owner; a type its dotted FQCN (no
//! container). A member's owner is the type that DECLARES it (the supertype walk), not the
//! receiver's class. A leading `/** … */` Javadoc on a PROJECT declaration is attached. A local
//! variable / parameter isn't keyed here, so hover on one is `None` (never a panic).

mod common;
use common::*;

fn ui() -> Project {
    Project::new(&[
        (
            "Base.java",
            "package ui;\n\
             public class Base {\n\
             \x20   public int baseM() { return 1; }\n\
             }\n",
        ),
        (
            "Widget.java",
            "package ui;\n\
             public class Widget extends Base {\n\
             \x20   private int size;\n\
             \x20   /**\n\
             \x20    * Render the widget.\n\
             \x20    */\n\
             \x20   public int render(int scale) { return size * scale; }\n\
             \x20   public int plain() { return 0; }\n\
             \x20   public int area() { return this.size * this.size; }\n\
             }\n",
        ),
        (
            "Screen.java",
            "package ui;\n\
             public class Screen {\n\
             \x20   public int draw(Widget w) {\n\
             \x20       int local = w.render(2);\n\
             \x20       return local + w.plain() + w.baseM();\n\
             \x20   }\n\
             }\n",
        ),
    ])
}

#[test]
fn hover_method_reports_signature_and_owner() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p.hover("Screen.java", at(&s, "w.render(2)") + "w.".len()).expect("hover on method");
    assert_eq!(h.kind, "method");
    assert_eq!(h.container.as_deref(), Some("ui.Widget"));
    assert!(h.signature.contains("render"), "signature names the method, got {:?}", h.signature);
}

#[test]
fn hover_field_reports_owner() {
    let p = ui();
    let s = p.source("Widget.java").to_string();
    let h = p.hover("Widget.java", at(&s, "this.size * this.size") + "this.".len()).expect("hover field");
    assert_eq!(h.kind, "field");
    assert_eq!(h.container.as_deref(), Some("ui.Widget"));
}

#[test]
fn hover_type_has_fqcn_and_no_container() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p.hover("Screen.java", at(&s, "Widget w")).expect("hover on type");
    assert_eq!(h.kind, "class");
    assert_eq!(h.signature, "ui.Widget");
    assert_eq!(h.container, None, "a type reports no container");
}

#[test]
fn hover_inherited_method_owner_is_declaring_type() {
    // `w.baseM()` on a Widget receiver → the member is DECLARED on Base; hover reports Base.
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p.hover("Screen.java", at(&s, "w.baseM()") + "w.".len()).expect("hover inherited method");
    assert_eq!(h.kind, "method");
    assert_eq!(h.container.as_deref(), Some("ui.Base"), "owner is the declaring supertype");
}

#[test]
fn hover_attaches_leading_javadoc() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p.hover("Screen.java", at(&s, "w.render(2)") + "w.".len()).expect("hover on documented method");
    let doc = h.doc.unwrap_or_default();
    assert!(doc.contains("Render the widget"), "javadoc attached, got {doc:?}");
}

#[test]
fn hover_without_javadoc_has_no_doc() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p.hover("Screen.java", at(&s, "w.plain()") + "w.".len()).expect("hover on plain method");
    assert_eq!(h.doc, None, "a method with no Javadoc carries no doc");
}

#[test]
fn hover_on_local_is_none() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    // `local` is a local variable — not keyed for hover.
    let off = at(&s, "return local +") + "return ".len();
    assert!(p.hover("Screen.java", off).is_none(), "a local is not keyed for hover");
}

#[test]
fn hover_on_keyword_is_none() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    assert!(p.hover("Screen.java", at(&s, "return local +")).is_none(), "keyword has no hover");
}

#[test]
fn hover_on_literal_is_none() {
    let p = ui();
    let s = p.source("Widget.java").to_string();
    assert!(p.hover("Widget.java", at(&s, "return 0;") + "return ".len()).is_none(), "literal has no hover");
}

#[test]
fn hover_does_not_panic_on_broken_file() {
    let p = Project::new(&[(
        "Broken.java",
        "package b;\npublic class Broken { int f( { return this.\n",
    )]);
    let s = p.source("Broken.java").to_string();
    let _ = p.hover("Broken.java", at(&s, "this.") + "this.".len());
    let _ = p.hover("Broken.java", 0);
}
