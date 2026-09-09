//! **Introduce field** — a local variable becomes a field of the class it is written in.
//!
//! ```java
//! void render() {                    void render() {
//!     var buffer = new StringBuilder();  buffer = new StringBuilder();
//!     buffer.append(head);      →        buffer.append(head);
//! }                                  }
//!                                    private StringBuilder buffer;
//! ```
//!
//! ## What moves and what does not
//!
//! The **declaration** moves; the **initialisation stays where it was**. That is the whole design
//! decision, and it is the conservative half of what IntelliJ offers: initialising the field at its
//! own declaration instead would run the expression at construction time rather than where the
//! method reaches it, which is a different program whenever the expression reads a parameter, throws,
//! or costs anything.
//!
//! So `int total = a + b;` becomes a field `private int total;` and the statement `total = a + b;`,
//! in that place, in that order.
//!
//! ## What it refuses, and why each one is a real defect and not a nicety
//!
//! - **A name the class already declares.** Writing the field anyway gives two members with one
//!   name — or, worse, silently binds every mention in the method to the wrong one.
//! - **An interface, an annotation type, a record.** A field of the first two is implicitly
//!   `public static final` and must be initialised where it is declared (JLS §9.3), and a record may
//!   not declare an instance field at all (JLS §8.10.1). None of these is a placement problem: there
//!   is nowhere for the field to go.
//! - **A declaration inside an anonymous class.** The enclosing *type* is then the anonymous body,
//!   and the field would land on the outer class, where the method that assigns it cannot see it as
//!   its own.
//! - **`int x[] = …`** and **`int[] a = {…}`** — the two declaration-only syntaxes. The brackets on
//!   the name and the braces of an array initialiser are both illegal in the assignment the
//!   statement becomes; `split-declaration` refuses them for exactly the same reason.
//!
//! ## The one thing it cannot see
//!
//! A field of a **superclass** with the same name. The new field shadows it, and every mention of
//! that name elsewhere in the class quietly changes what it reads. Seeing it needs the hierarchy,
//! which this crate does not have — the caller with a resolver does, and this is the case to reach
//! for it if the refactoring is ever measured short.

use tree_sitter::Node;

use crate::body::{body_of, has_modifier, insertion_point, member_indent, members_of};
use crate::declaration::{declaration_at, sole_declarator};
use crate::plan::{Outcome, Plan, RefactorEdit, Refusal, TypeNeed, TypeSlot};
use crate::selection::{
    descendants, enclosing, enclosing_callable, enclosing_type, is_inferred_type, newline, text,
};

const ID: (&str, &str) = ("introduce-field", "Introduce field");

/// The spelling written into the field while nobody has named its type — the same placeholder the
/// other type-needing refactorings use, so one caller fills them all.
pub const TYPE_PLACEHOLDER: &str = crate::extract_var::TYPE_PLACEHOLDER;

/// Plan an *introduce field* for the local declaration the caret is on.
pub fn introduce_field(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = ID;
    let decl = declaration_at(root, source, start, end)?;
    // Inside a method, a constructor or an initialiser — never at the top of a type, where the
    // declaration IS already a field and the row would be an offer to do nothing.
    let type_decl = enclosing_type(decl)?;
    let body = body_of(&type_decl)?;

    let Some(declarator) = sole_declarator(&decl) else {
        return Some(Err(Refusal::new(
            id,
            label,
            "this statement declares more than one variable, and a caret cannot say which one \
             becomes the field — split it first",
        )));
    };
    let name_node = declarator.child_by_field_name("name")?;
    let name = text(&name_node, source).to_string();
    let ty = decl.child_by_field_name("type")?;
    let value = declarator.child_by_field_name("value");

    if let Some(reason) = unfit(&type_decl, &decl, &declarator, value.as_ref()) {
        return Some(Err(Refusal::new(id, label, reason)));
    }
    // An anonymous class body is not in `TYPE_DECLS`, so the enclosing *type* of a local declared
    // inside one is the class around it — and a field written there is invisible to the body that
    // would assign it.
    if crosses_a_class_body(decl, type_decl) {
        return Some(Err(Refusal::new(
            id,
            label,
            "this local is inside an anonymous class, and the field would land on the class around \
             it rather than the one that reads it",
        )));
    }
    // A type parameter of the METHOD exists only inside it. `private T value;` on a class that
    // never declared `T` does not compile, and the class's own type parameters — which do work — are
    // deliberately not in this list.
    if let Some(method) = enclosing_callable(decl) {
        let borrowed = crate::body::type_parameters(&method, source);
        if let Some(taken) = crate::body::mentions_type(&ty, source, &borrowed) {
            return Some(Err(Refusal::new(
                id,
                label,
                format!("`{taken}` is a type parameter of this method, and a field of the class cannot be declared with it"),
            )));
        }
    }
    if declares_field(&body, source, &name) {
        return Some(Err(Refusal::new(
            id,
            label,
            format!("`{}` already declares a member called `{name}`", type_name(&type_decl, source)),
        )));
    }

    // A field assigned from a static context has to be static itself, and the context is whatever
    // the declaration sits in: a static method, or a static initialiser block.
    let is_static = enclosing_callable(decl)
        .map(|c| has_modifier(&c, source, "static"))
        .unwrap_or_else(|| enclosing(decl, &["static_initializer"]).is_some());

    let written_type = text(&ty, source);
    let inferred = is_inferred_type(written_type);
    let field_type = if inferred { TYPE_PLACEHOLDER } else { written_type };

    let indent = member_indent(source, &body);
    let nl = newline(source);
    let modifiers = if is_static { "private static " } else { "private " };
    let declaration = format!("{indent}{modifiers}{field_type} {name};{nl}");
    let insert_at = insertion_point(&body, source)?;

    // The statement keeps its place and loses everything before the name — modifiers and type
    // both: `final int total = a + b;` becomes `total = a + b;`, and a `final` carried over would
    // be a field assigned outside a constructor, which does not compile.
    // A declaration with no value has nothing left to say and goes away with its line.
    let (statement_start, statement_end) = match value {
        Some(_) => (decl.start_byte(), name_node.start_byte()),
        None => (
            crate::body::line_start(source, decl.start_byte()),
            crate::body::line_after(source, decl.end_byte()),
        ),
    };

    let edits = vec![
        RefactorEdit::new(insert_at, insert_at, declaration, "declaration"),
        RefactorEdit::new(statement_start, statement_end, String::new(), "statement"),
    ];
    // The caret lands on the field's NAME — the one thing worth typing over, and the line the
    // user wants to see now that it exists. Measured in the text the plan PRODUCES, so a statement
    // edit that lands above the field — a class whose fields are written after its methods, which
    // is unusual and legal — has to be taken off it.
    let shrunk = if statement_start < insert_at { statement_end - statement_start } else { 0 };
    let on_the_name =
        insert_at + indent.len() + modifiers.len() + field_type.len() + 1 - shrunk;
    let plan = Plan::new(id, label, edits).named(name);
    if !inferred {
        return Some(Ok(plan.caret_at(on_the_name)));
    }
    // `var` was written, and a FIELD is never `var` in any Java version — so the caller has to name
    // the type or leave the plan alone. Which is exactly what `extract-constant` needs, and the
    // same slot answers both.
    let slot_index = plan.edits.iter().position(|e| e.reason == "declaration")?;
    // `var` without an initialiser does not parse, so a `var` declaration always has a value.
    let value = value?;
    Some(Ok(plan.caret_at(on_the_name).needing_type(TypeSlot {
        start: value.start_byte(),
        end: value.end_byte(),
        edit_index: slot_index,
        at: indent.len() + modifiers.len(),
        placeholder: TYPE_PLACEHOLDER.to_string(),
        need: TypeNeed::Required,
    })))
}

/// Why this declaration cannot become a field — `None` when it can.
fn unfit(
    type_decl: &Node<'_>,
    decl: &Node<'_>,
    declarator: &Node<'_>,
    value: Option<&Node<'_>>,
) -> Option<String> {
    match type_decl.kind() {
        "interface_declaration" | "annotation_type_declaration" => {
            return Some(
                "a field of an interface is implicitly `public static final` and has to be \
                 initialised where it is declared"
                    .to_string(),
            )
        }
        "record_declaration" => {
            return Some("a record may not declare an instance field".to_string())
        }
        _ => {}
    }
    // `int x[] = …` puts the brackets on the NAME, so a field written from the type text alone is
    // `int x;` and the assignment `x = new int[3];` does not compile.
    if declarator.child_by_field_name("dimensions").is_some() {
        return Some(
            "the array brackets are written on the name rather than the type — move them to the \
             type first"
                .to_string(),
        );
    }
    // `int[] a = {1, 2};` — braces are declaration syntax. Left behind as `a = {1, 2};` they are an
    // illegal start of expression.
    if value.is_some_and(|v| v.kind() == "array_initializer") {
        return Some(
            "an array written with braces is declaration syntax and cannot stand alone in an \
             assignment — write `new int[]{…}` first"
                .to_string(),
        );
    }
    let _ = decl;
    None
}

/// Whether anything between `decl` and its enclosing type declaration is a class body of its own —
/// an anonymous class, which owns its members and is not in `TYPE_DECLS`.
fn crosses_a_class_body(decl: Node<'_>, type_decl: Node<'_>) -> bool {
    let mut node = decl;
    while let Some(parent) = node.parent() {
        if parent.id() == type_decl.id() {
            return false;
        }
        if parent.kind() == "class_body" && parent.parent().map(|p| p.id()) != Some(type_decl.id()) {
            return true;
        }
        node = parent;
    }
    false
}

/// Whether this type body already declares a member of this name — a field, an enum constant, or a
/// nested type. Methods are deliberately not in the list: Java keeps them in their own namespace,
/// so a field `total` beside a method `total()` is legal and refusing it would be a rule about
/// nothing.
fn declares_field(body: &Node<'_>, source: &str, name: &str) -> bool {
    let named = |node: &Node<'_>| {
        node.child_by_field_name("name").map(|n| text(&n, source)) == Some(name)
    };
    for member in members_of(body) {
        match member.kind() {
            "field_declaration" => {
                if descendants(member, "variable_declarator").iter().any(named) {
                    return true;
                }
            }
            // A nested type shares the member namespace with the fields around it.
            kind if crate::selection::TYPE_DECLS.contains(&kind) => {
                if named(&member) {
                    return true;
                }
            }
            _ => {}
        }
    }
    // Enum constants ARE fields of the enum.
    let mut cursor = body.walk();
    let clash = body.named_children(&mut cursor).any(|c| c.kind() == "enum_constant" && named(&c));
    clash
}

/// The name of a type declaration, for a sentence about it.
fn type_name<'a>(type_decl: &Node<'_>, source: &'a str) -> &'a str {
    type_decl.child_by_field_name("name").map(|n| text(&n, source)).unwrap_or("this type")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn outcome(source: &str, needle: &str) -> Outcome {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap();
        introduce_field(tree.root_node(), source, at, at)
    }

    fn applied(source: &str, needle: &str) -> String {
        match outcome(source, needle) {
            Some(Ok(plan)) => plan.apply(source),
            other => panic!("expected a plan, got {other:?}"),
        }
    }

    fn refusal(source: &str, needle: &str) -> String {
        match outcome(source, needle) {
            Some(Err(r)) => r.reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_local_becomes_a_field_and_the_initialisation_stays_put() {
        let src = "class A {\n    void f(int a, int b) {\n        int total = a + b;\n        use(total);\n    }\n}";
        let out = applied(src, "int total");
        assert!(out.contains("    private int total;"), "{out}");
        assert!(out.contains("        total = a + b;"), "{out}");
        assert!(!out.contains("int total = a + b"), "{out}");
    }

    /// A static method can only assign a static field.
    #[test]
    fn a_local_of_a_static_method_becomes_a_static_field() {
        let src = "class A {\n    static void f() {\n        String s = \"x\";\n        use(s);\n    }\n}";
        let out = applied(src, "String s");
        assert!(out.contains("private static String s;"), "{out}");
    }

    /// The new field goes after the last field, which is where a person writes one.
    #[test]
    fn the_field_lands_with_the_other_fields() {
        let src = "class A {\n    int a;\n    void f() {\n        int b = 1;\n        use(b);\n    }\n}";
        let out = applied(src, "int b");
        let field = out.find("private int b;").unwrap();
        assert!(field > out.find("int a;").unwrap() && field < out.find("void f()").unwrap(), "{out}");
    }

    /// A declaration with no value has nothing to leave behind; the whole line goes.
    #[test]
    fn a_declaration_without_a_value_moves_whole() {
        let src = "class A {\n    void f() {\n        int n;\n        n = 3;\n    }\n}";
        let out = applied(src, "int n;");
        assert!(out.contains("private int n;"), "{out}");
        assert!(!out.contains("        int n;"), "{out}");
        assert!(out.contains("        n = 3;"), "{out}");
    }

    /// `var` is not a thing a field can be declared as, in any Java version — so the plan asks for
    /// the type and refuses to be applied without it.
    #[test]
    fn a_var_local_asks_for_its_type() {
        let src = "class A {\n    void f() {\n        var list = make();\n        use(list);\n    }\n}";
        let Some(Ok(plan)) = outcome(src, "var list") else { panic!("expected a plan") };
        let slot = plan.type_slot.clone().expect("a type is needed");
        assert_eq!(slot.need, TypeNeed::Required);
        assert_eq!(&src[slot.start..slot.end], "make()");
        let mut plan = plan;
        plan.fill_type("List<String>");
        assert!(plan.apply(src).contains("private List<String> list;"), "{}", plan.apply(src));
    }

    /// A type parameter of the method exists only inside it.
    #[test]
    fn a_method_type_parameter_cannot_become_a_field() {
        let src = "class A {\n    <T> void f(T in) {\n        T held = in;\n        use(held);\n    }\n}";
        assert!(refusal(src, "T held").contains("type parameter of this method"));
    }

    /// The CLASS's own type parameters do work, and a field may be declared with one.
    #[test]
    fn a_class_type_parameter_is_fine() {
        let src = "class A<T> {\n    void f(T in) {\n        T held = in;\n        use(held);\n    }\n}";
        assert!(applied(src, "T held").contains("private T held;"));
    }

    #[test]
    fn a_name_the_class_already_uses_is_refused() {
        let src = "class A {\n    int total;\n    void f() {\n        int total = 1;\n        use(total);\n    }\n}";
        assert!(refusal(src, "int total = 1").contains("already declares"));
    }

    #[test]
    fn an_interface_and_a_record_have_nowhere_to_put_it() {
        let iface = "interface A {\n    default void f() {\n        int n = 1;\n        use(n);\n    }\n}";
        assert!(refusal(iface, "int n = 1").contains("public static final"));
        let rec = "record A(int x) {\n    void f() {\n        int n = 1;\n        use(n);\n    }\n}";
        assert!(refusal(rec, "int n = 1").contains("instance field"));
    }

    #[test]
    fn a_local_inside_an_anonymous_class_is_refused() {
        let src = "class A {\n    void f() {\n        Runnable r = new Runnable() {\n            public void run() {\n                int n = 1;\n                use(n);\n            }\n        };\n    }\n}";
        assert!(refusal(src, "int n = 1").contains("anonymous class"));
    }

    #[test]
    fn the_two_declaration_only_syntaxes_are_refused() {
        let brackets = "class A {\n    void f() {\n        int x[] = new int[3];\n        use(x);\n    }\n}";
        assert!(refusal(brackets, "int x[]").contains("brackets"));
        let braces = "class A {\n    void f() {\n        int[] a = {1, 2};\n        use(a);\n    }\n}";
        assert!(refusal(braces, "int[] a").contains("declaration syntax"));
    }

    #[test]
    fn several_variables_at_once_cannot_say_which_one() {
        let src = "class A {\n    void f() {\n        int a = 1, b = 2;\n        use(a, b);\n    }\n}";
        assert!(refusal(src, "int a = 1").contains("more than one"));
    }

    /// A caret in a field declaration is not a caret in a local one.
    #[test]
    fn a_field_offers_nothing() {
        let src = "class A {\n    int a = 1;\n}";
        assert!(outcome(src, "int a").is_none());
    }
}
