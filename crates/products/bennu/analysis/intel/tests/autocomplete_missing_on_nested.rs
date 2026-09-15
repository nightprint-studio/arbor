//! The method another class asks this one for — read from OUTSIDE the type being edited.
//!
//! `undeclared_calls` reads the type's own subtree, which is the whole story for a top-level class
//! and half of it for a nested one. `c.randomico()` is written in the outer class, on an instance
//! of the inner, so standing inside the inner there was nothing to offer: the one call that
//! describes the method lives outside the walk. The receiver's type is what connects them.

mod common;
use common::*;

/// The outer calls a method on an instance of its own nested class; the caret is in the nested
/// class's body, with `ra` typed.
const SRC: &str = "package shop;\n\
     public class Cfg {\n\
     \x20   void go() {\n\
     \x20       MyProva c = new MyProva();\n\
     \x20       int n = c.randomico(\"ciao\");\n\
     \x20   }\n\
     \x20   public static class MyProva {\n\
     \x20       ra\n\
     \x20   }\n\
     }\n";

fn generated(src: &str, typed_at: &str) -> Vec<String> {
    let p = Project::new(&[("Cfg.java", src)]);
    p.generated("Cfg.java", at(src, typed_at) + typed_at.len())
        .into_iter()
        .filter(|c| c.kind == "generate")
        .map(|c| c.label)
        .collect()
}

#[test]
fn a_call_made_in_the_outer_class_is_offered_inside_the_nested_one() {
    let got = generated(SRC, "       ra");
    assert!(got.iter().any(|l| l == "randomico"), "offered: {got:?}");
}

#[test]
fn accepting_it_writes_a_public_method_with_the_signature_the_call_specified() {
    let p = Project::new(&[("Cfg.java", SRC)]);
    let items = p.generated("Cfg.java", at(SRC, "       ra") + "       ra".len());
    let item = items
        .iter()
        .find(|c| c.label == "randomico" && c.kind == "generate")
        .unwrap_or_else(|| panic!("not offered: {:?}", items.iter().map(|c| &c.label).collect::<Vec<_>>()));
    let text = item.insert_text.as_deref().unwrap_or_default();
    // Public: it is called from another class. And typed by the call — a `String` in, an `int` out.
    assert!(text.contains("public int randomico(String"), "{text:?}");
}

#[test]
fn a_method_the_nested_class_already_declares_is_not_offered() {
    let src = SRC.replace(
        "\x20       ra\n",
        "\x20       public int randomico(String s) { return 0; }\n        ra\n",
    );
    let got = generated(&src, "        ra");
    assert!(!got.iter().any(|l| l == "randomico"), "offered anyway: {got:?}");
}

#[test]
fn a_call_on_a_different_type_is_not_offered_here() {
    let src = "package shop;\n\
         public class Cfg {\n\
         \x20   void go() {\n\
         \x20       Other o = new Other();\n\
         \x20       o.randomico(\"ciao\");\n\
         \x20   }\n\
         \x20   public static class MyProva {\n\
         \x20       ra\n\
         \x20   }\n\
         \x20   public static class Other { }\n\
         }\n";
    let got = generated(src, "       ra");
    assert!(!got.iter().any(|l| l == "randomico"), "offered on the wrong class: {got:?}");
}

/// The pre-existing family still works — a call the type makes on ITSELF.
#[test]
fn a_call_the_type_makes_on_itself_is_still_offered() {
    let src = "package shop;\n\
         public class Cfg {\n\
         \x20   void go() { int n = locale(\"x\"); }\n\
         \x20   loc\n\
         }\n";
    let got = generated(src, "   loc");
    assert!(got.iter().any(|l| l == "locale"), "offered: {got:?}");
}
