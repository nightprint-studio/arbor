//! Hover category — the hover card for the symbol under the caret.
//!
//! Hover shares the caret classifier with go-to / find-usages, then renders a card: a method
//! reports its signature + owning type; a field its type + owner; a type reads like its
//! declaration (`class Widget`) with its package as the container. A member's owner is the type
//! that DECLARES it (the supertype walk), not the receiver's class. A leading `/** … */` Javadoc
//! on a PROJECT declaration is attached. A local variable / parameter isn't keyed here, so hover
//! on one is `None` (never a panic).

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

/// A library type reached through an import-on-demand hovers like any other.
///
/// The reported case was an annotation — `@Service` under `import org.springframework.stereotype.*;`
/// showed nothing, the same annotation under a single-type import showed its card — but the
/// annotation was incidental. The caret was classified with the WALK resolver, which is project-only
/// and stops before the probe that reads a star import of a library package; a single-type import
/// binds before any probe, which is why that shape always worked.
#[test]
fn a_library_type_under_a_star_import_hovers() {
    let p = Project::new(&[(
        "Mapper.java",
        "package app;\n\
         import java.util.function.*;\n\
         public class Mapper {\n\
         \x20   Function<String, String> f;\n\
         }\n",
    )]);
    let s = p.source("Mapper.java").to_string();
    let caret = at(&s, "Function<String");

    // Before the library tier arrives the engine can see only the project — nothing to show, and
    // nothing wrong shown either.
    assert!(p.hover("Mapper.java", caret).is_none());

    // Once it has, the star import binds the name exactly as a single-type import would.
    p.grant_library_policy();
    let h = p.hover("Mapper.java", caret).expect("hover on a star-imported library type");
    assert_eq!(h.kind, "interface", "{h:?}");
    assert!(h.signature.contains("Function"), "{h:?}");
}

/// Two packages declare `Riga`, and the file wildcard-imports one of them. The caret has to land on
/// the key the WALK filed the type under — the imported one — not on whichever `Riga` the
/// project's simple-name map happens to keep. The classifier used to ask that map first.
#[test]
fn a_star_imported_project_type_keys_to_the_package_it_was_imported_from() {
    let p = Project::new(&[
        ("a/Riga.java", "package a;\npublic class Riga {}\n"),
        ("b/Riga.java", "package b;\npublic class Riga {}\n"),
        (
            "c/Uses.java",
            "package c;\n\
             import b.*;\n\
             public class Uses {\n\
             \x20   Riga r;\n\
             }\n",
        ),
    ]);
    let s = p.source("c/Uses.java").to_string();
    let h = p.hover("c/Uses.java", at(&s, "Riga r")).expect("hover on the star-imported type");
    assert_eq!(h.owner.as_deref(), Some("b/Riga"), "{h:?}");
}

#[test]
fn hover_method_reports_signature_and_owner() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p
        .hover("Screen.java", at(&s, "w.render(2)") + "w.".len())
        .expect("hover on method");
    assert_eq!(h.kind, "method");
    assert_eq!(h.container.as_deref(), Some("ui.Widget"));
    assert!(
        h.signature.contains("render"),
        "signature names the method, got {:?}",
        h.signature
    );
}

#[test]
fn hover_field_reports_owner() {
    let p = ui();
    let s = p.source("Widget.java").to_string();
    let h = p
        .hover(
            "Widget.java",
            at(&s, "this.size * this.size") + "this.".len(),
        )
        .expect("hover field");
    assert_eq!(h.kind, "field");
    assert_eq!(h.container.as_deref(), Some("ui.Widget"));
}

#[test]
fn hover_type_reads_like_its_declaration_with_the_package_beside_it() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p
        .hover("Screen.java", at(&s, "Widget w"))
        .expect("hover on type");
    assert_eq!(h.kind, "class");
    assert_eq!(h.signature, "class Widget");
    assert_eq!(
        h.container.as_deref(),
        Some("ui"),
        "the package, not the whole FQCN again"
    );
}

/// An interface must not report itself as a class — the card would be stating something false
/// about the thing under the pointer.
#[test]
fn hover_interface_says_interface() {
    let p = Project::new(&[
        (
            "Shape.java",
            "package ui;\npublic interface Shape { int area(); }\n",
        ),
        (
            "Use.java",
            "package ui;\npublic class Use { public int go(Shape s) { return s.area(); } }\n",
        ),
    ]);
    let s = p.source("Use.java").to_string();
    let h = p
        .hover("Use.java", at(&s, "Shape s"))
        .expect("hover on interface");
    assert_eq!(h.kind, "interface");
    assert_eq!(h.signature, "interface Shape");
}

#[test]
fn hover_inherited_method_owner_is_declaring_type() {
    // `w.baseM()` on a Widget receiver → the member is DECLARED on Base; hover reports Base.
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p
        .hover("Screen.java", at(&s, "w.baseM()") + "w.".len())
        .expect("hover inherited method");
    assert_eq!(h.kind, "method");
    assert_eq!(
        h.container.as_deref(),
        Some("ui.Base"),
        "owner is the declaring supertype"
    );
}

#[test]
fn hover_attaches_leading_javadoc() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p
        .hover("Screen.java", at(&s, "w.render(2)") + "w.".len())
        .expect("hover on documented method");
    let doc = h.doc.unwrap_or_default();
    assert!(
        doc.contains("Render the widget"),
        "javadoc attached, got {doc:?}"
    );
}

#[test]
fn hover_without_javadoc_has_no_doc() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    let h = p
        .hover("Screen.java", at(&s, "w.plain()") + "w.".len())
        .expect("hover on plain method");
    assert_eq!(h.doc, None, "a method with no Javadoc carries no doc");
}

#[test]
fn hover_on_local_is_none() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    // `local` is a local variable — not keyed for hover.
    let off = at(&s, "return local +") + "return ".len();
    assert!(
        p.hover("Screen.java", off).is_none(),
        "a local is not keyed for hover"
    );
}

#[test]
fn hover_on_keyword_is_none() {
    let p = ui();
    let s = p.source("Screen.java").to_string();
    assert!(
        p.hover("Screen.java", at(&s, "return local +")).is_none(),
        "keyword has no hover"
    );
}

#[test]
fn hover_on_literal_is_none() {
    let p = ui();
    let s = p.source("Widget.java").to_string();
    assert!(
        p.hover("Widget.java", at(&s, "return 0;") + "return ".len())
            .is_none(),
        "literal has no hover"
    );
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

// ── the kind is read off the class flags, so it needs a resolver that can read them ───────────

#[test]
fn hover_annotation_says_annotation() {
    let p = Project::new(&[
        ("Marker.java", "package ui;\npublic @interface Marker { }\n"),
        ("Use.java", "package ui;\npublic class Use { Marker m; }\n"),
    ]);
    let s = p.source("Use.java").to_string();
    let h = p.hover("Use.java", at(&s, "Marker m")).expect("hover on the annotation");
    assert_eq!(h.kind, "annotation");
    assert_eq!(h.signature, "annotation Marker");
}
