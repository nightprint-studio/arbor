//! Completion after `::` in every shape a method reference is written, including the half-typed
//! ones the editor actually sees: no closing parenthesis yet, no statement end, the end of the file.
//!
//! One trigger per file — a half-written reference is a syntax error, and several in one class body
//! let tree-sitter's recovery swallow the declarations between them.

mod common;
use common::*;

const FOO: &str = "package com.acme;\n\
     public class Foo {\n\
     \x20   public Foo() {}\n\
     \x20   public Foo(String name) {}\n\
     \x20   public String label() { return \"\"; }\n\
     \x20   public static Foo of(String name) { return null; }\n\
     }\n";

const BASE: &str = "package com.acme;\n\
     public class Base {\n\
     \x20   public void greet() {}\n\
     }\n";

const BOX: &str = "package com.acme;\n\
     public class Box<T> {\n\
     \x20   public T content() { return null; }\n\
     }\n";

const USER: &str = "com/acme/User.java";

/// A class whose method body is `body` and whose remaining text closes it.
fn project(body: &str) -> Project {
    let user = format!(
        "package com.acme;\n\
         import java.util.Optional;\n\
         public class User extends Base {{\n\
         \x20   Foo foo;\n\
         \x20   void run(Optional<Foo> opt) {{\n\
         \x20       {body}\n\
         \x20   }}\n\
         \x20   String describe(Foo f) {{ return \"\"; }}\n\
         }}\n"
    );
    Project::new(&[
        ("com/acme/Foo.java", FOO),
        ("com/acme/Base.java", BASE),
        ("com/acme/Box.java", BOX),
        (USER, &user),
    ])
}

/// A file that ENDS at the reference, as it does while the last line of a class is being typed.
fn project_ending_with(tail: &str) -> Project {
    let user = format!(
        "package com.acme;\n\
         import java.util.Optional;\n\
         public class User extends Base {{\n\
         \x20   void run(Optional<Foo> opt) {{\n\
         \x20       {tail}"
    );
    Project::new(&[
        ("com/acme/Foo.java", FOO),
        ("com/acme/Base.java", BASE),
        ("com/acme/Box.java", BOX),
        (USER, &user),
    ])
}

fn labels_after(p: &Project, needle: &str) -> Vec<String> {
    let offset = at_last(p.source(USER), needle) + needle.len();
    p.complete_labels(USER, offset)
}

#[test]
fn a_type_before_the_colons_offers_statics_instance_methods_and_new() {
    let p = project("opt.map(Foo::)");
    let labels = labels_after(&p, "Foo::");
    for want in ["label", "of", "new"] {
        assert!(labels.contains(&want.to_string()), "{want}: {labels:?}");
    }
}

/// The state while typing: the call is not closed and the statement is not finished.
#[test]
fn an_unclosed_call_still_completes_the_reference() {
    let p = project("opt.map(Foo::");
    let labels = labels_after(&p, "Foo::");
    assert!(labels.contains(&"label".to_string()), "{labels:?}");
    assert!(labels.contains(&"new".to_string()), "{labels:?}");
}

#[test]
fn a_reference_at_the_very_end_of_the_file_completes() {
    let p = project_ending_with("opt.map(Foo::");
    let labels = p.complete_labels(USER, p.source(USER).len());
    assert!(labels.contains(&"label".to_string()), "{labels:?}");
}

#[test]
fn a_bare_type_reference_at_the_end_of_the_file_completes() {
    let p = project_ending_with("Foo::");
    let labels = p.complete_labels(USER, p.source(USER).len());
    assert!(labels.contains(&"of".to_string()), "{labels:?}");
}

#[test]
fn a_typed_prefix_narrows_an_unclosed_reference() {
    let p = project("opt.map(Foo::la");
    let labels = labels_after(&p, "Foo::la");
    assert!(labels.contains(&"label".to_string()), "{labels:?}");
    assert!(!labels.contains(&"of".to_string()), "{labels:?}");
}

#[test]
fn an_expression_before_the_colons_offers_its_instance_methods() {
    let p = project("opt.map(foo::");
    let labels = labels_after(&p, "foo::");
    assert!(labels.contains(&"label".to_string()), "{labels:?}");
    assert!(!labels.contains(&"new".to_string()), "a value cannot be constructed: {labels:?}");
}

#[test]
fn this_before_the_colons_offers_the_enclosing_classes_methods() {
    let p = project("opt.map(this::");
    let labels = labels_after(&p, "this::");
    assert!(labels.contains(&"describe".to_string()), "{labels:?}");
}

#[test]
fn super_before_the_colons_offers_the_superclasss_methods() {
    let p = project("opt.map(super::");
    let labels = labels_after(&p, "super::");
    assert!(labels.contains(&"greet".to_string()), "{labels:?}");
}

#[test]
fn type_arguments_before_the_colons_do_not_hide_the_type() {
    let p = project("Box<String>::");
    let labels = labels_after(&p, "Box<String>::");
    assert!(labels.contains(&"content".to_string()), "{labels:?}");
}

#[test]
fn an_array_type_offers_only_its_constructor() {
    for body in ["Foo[]::", "int[]::"] {
        let p = project(body);
        let items = p.complete(USER, at_last(p.source(USER), body) + body.len());
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["new"], "{body}");
        assert_eq!(items[0].kind, "constructor");
        assert!(items[0].insert_text.is_none());
    }
}

#[test]
fn an_indexed_expression_is_not_an_array_type() {
    let p = project("Foo[] all = null; all[0]::");
    let labels = labels_after(&p, "all[0]::");
    assert!(!labels.contains(&"new".to_string()), "{labels:?}");
}

#[test]
fn a_reference_inserts_the_name_without_parentheses() {
    let p = project("opt.map(Foo::");
    let offset = at_last(p.source(USER), "Foo::") + "Foo::".len();
    for item in p.complete(USER, offset) {
        assert!(item.insert_text.is_none(), "{} inserts {:?}", item.label, item.insert_text);
    }
}
