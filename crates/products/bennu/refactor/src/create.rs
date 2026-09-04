//! **Create method** — write the method a call is already asking for.
//!
//! ## Why this is a fix and not an offer
//!
//! Every other transform in this crate is offered wherever it applies, because "can this be
//! extracted" is a question about the text. "Does this method exist" is not: it is a question about
//! the whole classpath, and a class that inherits `handle()` from a superclass three jars away
//! looks, in this file, exactly like one that is calling a method nobody wrote.
//!
//! So nothing here decides that. The **resolver** decides, by emitting `unknown-member` /
//! `unresolved-symbol`, and this renders the repair for the span it points at. That is the same
//! split the rest of the editor makes — text-only fixes in the pure crate, type-aware ones where
//! the types are — and it is why [`create_method`] takes a span rather than a caret.
//!
//! ## What the signature is read from
//!
//! The call site is a specification, and a surprisingly complete one:
//!
//! * **parameter types** — a literal is its own type; an argument that is a local, a parameter or a
//!   field of this class is that declaration's type, copied verbatim so the signature reads like the
//!   code around it; anything else is `Object`, which is honest rather than clever.
//! * **parameter names** — an argument that is a name keeps it. `total(count, 3)` writes
//!   `total(int count, int arg2)`, and the one name the call gave us is worth more than two
//!   invented ones.
//! * **the return type** — from what the call's result is used AS: nothing (`void`), a declared
//!   local's type, the enclosing method's return type under a `return`, `boolean` in a condition.
//! * **`static`** — a call from a static method must reach a static one, or the stub does not
//!   compile at the only site that asked for it.
//!
//! The body is `throw new UnsupportedOperationException`, which is the one body that compiles for
//! every return type — including `void` — and fails loudly rather than returning a plausible `null`.

use tree_sitter::Node;

use crate::plan::{Outcome, Plan, RefactorEdit, Refusal};
use crate::selection::{descendants, enclosing_callable, enclosing_type, indent_at, is_static, newline, node_at, text};

const ID: (&str, &str) = ("create-method", "Create method");

/// Plan the method a call at `[start, end)` is asking for.
///
/// `[start, end)` is the diagnostic's span — the call's *name*. A caret works too: pass it as an
/// empty range.
pub fn create_method(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = ID;
    let at = node_at(root, start)?;
    let call = crate::selection::enclosing(at, &["method_invocation"])?;
    let name_node = call.child_by_field_name("name")?;
    // The span has to be the NAME. A diagnostic on the receiver or on an argument is about
    // something else, and generating a method from it would be answering a question nobody asked.
    if end > start && (name_node.start_byte() > start || name_node.end_byte() < end) {
        return None;
    }
    let name = text(&name_node, source).to_string();

    // A call on another object belongs in that object's class, which is another file — a bigger
    // action than an edit to this one, and saying so is better than writing the method in the
    // wrong place.
    if let Some(receiver) = call.child_by_field_name("object") {
        if text(&receiver, source) != "this" {
            return Some(Err(Refusal::new(
                id,
                label,
                format!("`{}` is not this class — create the method in its own file", text(&receiver, source)),
            )));
        }
    }

    let method = enclosing_callable(call)?;
    let type_decl = enclosing_type(call)?;
    if has_method(&type_decl, &name, source) {
        return None; // it exists here; the diagnostic was about something else
    }

    let parameters = parameters_for(&call, &method, &type_decl, source);
    let returns = return_type_for(&call, &method, source);
    // `private static`, in that order: any order compiles, and every Java style guide and the JLS's
    // own examples write the access modifier first. A generated method that does not look like the
    // ones around it is one the reader stops to check.
    let statics = if is_static(&method, source) { "static " } else { "" };

    let indent = indent_at(source, method.start_byte());
    let nl = newline(source);
    let signature = format!("private {statics}{returns} {name}({})", parameters.join(", "));
    let stub = format!(
        "{nl}{nl}{indent}{signature} {{{nl}{indent}    throw new UnsupportedOperationException(\"TODO: {name}\");{nl}{indent}}}"
    );
    let insert_at = method.end_byte();

    let plan = Plan::new(id, &format!("Create method '{name}'"), vec![RefactorEdit::new(insert_at, insert_at, stub, "declaration")])
        .named(name)
        // The caret lands on the stub's body, which is the only line the user has to replace.
        .caret_at(insert_at + 2 * nl.len() + indent.len() + signature.len() + 3);
    Some(Ok(plan))
}

/// Whether this type already declares a method of that name — any arity.
///
/// Any arity on purpose: an overload the call does not match is a *different* diagnostic, and
/// generating a second `handle` beside the first is not what "create method" means.
fn has_method(type_decl: &Node<'_>, name: &str, source: &str) -> bool {
    descendants(*type_decl, "method_declaration").iter().any(|m| {
        m.child_by_field_name("name").is_some_and(|n| text(&n, source) == name)
    })
}

/// `Type name` for every argument of the call.
fn parameters_for(
    call: &Node<'_>,
    method: &Node<'_>,
    type_decl: &Node<'_>,
    source: &str,
) -> Vec<String> {
    let Some(arguments) = call.child_by_field_name("arguments") else { return Vec::new() };
    let mut cursor = arguments.walk();
    let mut taken: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (i, argument) in arguments.named_children(&mut cursor).enumerate() {
        let type_text = argument_type(&argument, method, type_decl, source);
        let mut name = argument_name(&argument, source).unwrap_or_else(|| format!("arg{}", i + 1));
        while taken.contains(&name) {
            name = format!("{name}{}", i + 1);
        }
        taken.push(name.clone());
        out.push(format!("{type_text} {name}"));
    }
    out
}

/// The declared type of an argument, or `Object` when the text does not say.
fn argument_type(
    argument: &Node<'_>,
    method: &Node<'_>,
    type_decl: &Node<'_>,
    source: &str,
) -> String {
    if let Some(literal) = literal_type(argument, source) {
        return literal;
    }
    // A `var` / `val` local says nothing about its type, so `Object` is the honest answer — writing
    // `var` into a parameter list is not a worse guess, it is a syntax error.
    let written = |t: Option<String>| {
        t.filter(|t| !crate::selection::is_inferred_type(t)).unwrap_or_else(|| "Object".to_string())
    };
    match argument.kind() {
        "identifier" => {
            written(declared_type_of(text(argument, source), method, type_decl, source))
        }
        "field_access" => written(
            argument
                .child_by_field_name("field")
                .and_then(|f| declared_type_of(text(&f, source), method, type_decl, source)),
        ),
        "object_creation_expression" | "cast_expression" => argument
            .child_by_field_name("type")
            .map(|t| text(&t, source).to_string())
            .unwrap_or_else(|| "Object".to_string()),
        _ => "Object".to_string(),
    }
}

fn literal_type(node: &Node<'_>, source: &str) -> Option<String> {
    let ty = match node.kind() {
        "string_literal" => "String",
        "character_literal" => "char",
        "true" | "false" => "boolean",
        "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal"
        | "binary_integer_literal" => {
            if text(node, source).ends_with(['l', 'L']) {
                "long"
            } else {
                "int"
            }
        }
        "decimal_floating_point_literal" | "hex_floating_point_literal" => {
            if text(node, source).ends_with(['f', 'F']) {
                "float"
            } else {
                "double"
            }
        }
        _ => return None,
    };
    Some(ty.to_string())
}

/// An argument that is a name lends it to the parameter — the one piece of naming the call site
/// actually knows.
fn argument_name(argument: &Node<'_>, source: &str) -> Option<String> {
    match argument.kind() {
        "identifier" => Some(text(argument, source).to_string()),
        "field_access" => argument.child_by_field_name("field").map(|f| text(&f, source).to_string()),
        _ => None,
    }
}

/// The declared type of a name visible here: a local, a parameter, or a field of this class.
fn declared_type_of(
    name: &str,
    method: &Node<'_>,
    type_decl: &Node<'_>,
    source: &str,
) -> Option<String> {
    let named = |declaration: &Node<'_>, declarator: &Node<'_>| -> Option<String> {
        let matches = declarator
            .child_by_field_name("name")
            .is_some_and(|n| text(&n, source) == name);
        let ty = declaration.child_by_field_name("type")?;
        matches.then(|| text(&ty, source).to_string())
    };
    for declaration in descendants(*method, "local_variable_declaration") {
        for declarator in descendants(declaration, "variable_declarator") {
            if let Some(found) = named(&declaration, &declarator) {
                return Some(found);
            }
        }
    }
    for parameter in descendants(*method, "formal_parameter") {
        if parameter.child_by_field_name("name").is_some_and(|n| text(&n, source) == name) {
            return parameter.child_by_field_name("type").map(|t| text(&t, source).to_string());
        }
    }
    for field in descendants(*type_decl, "field_declaration") {
        for declarator in descendants(field, "variable_declarator") {
            if let Some(found) = named(&field, &declarator) {
                return Some(found);
            }
        }
    }
    None
}

/// What the call's result is used as.
fn return_type_for(call: &Node<'_>, method: &Node<'_>, source: &str) -> String {
    // A condition's parentheses are a node of their own, so `if (ready())` reaches the `if` only
    // through them — and without this step every condition answers `Object`.
    let mut node = *call;
    while node.parent().is_some_and(|p| p.kind() == "parenthesized_expression") {
        node = node.parent().unwrap_or(node);
    }
    let call = &node;
    let Some(parent) = call.parent() else { return "void".to_string() };
    match parent.kind() {
        // Nobody reads it.
        "expression_statement" => "void".to_string(),
        // `Foo f = call();` — the declaration says exactly what is wanted.
        "variable_declarator" => parent
            .parent()
            .and_then(|d| d.child_by_field_name("type"))
            .map(|t| text(&t, source).to_string())
            .unwrap_or_else(|| "Object".to_string()),
        // `return call();` — the enclosing method already declared it.
        "return_statement" => method
            .child_by_field_name("type")
            .map(|t| text(&t, source).to_string())
            .unwrap_or_else(|| "Object".to_string()),
        // `if (call())`, `while (call())` — the grammar leaves no doubt.
        "if_statement" | "while_statement" | "do_statement" => "boolean".to_string(),
        "unary_expression" if text(&parent, source).starts_with('!') => "boolean".to_string(),
        _ => "Object".to_string(),
    }
}

/// A type that does not exist, and what the code using it says it should be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingType {
    pub name: String,
    /// `class`, `interface` or `@interface` — the declaration keyword, verbatim.
    pub keyword: &'static str,
}

/// Read a missing type off the span an `unresolved-type` diagnostic points at.
///
/// The **use** decides the kind, because it is the only evidence there is and it is usually right:
/// a name in an `implements` clause has to be an interface, a name after `@` has to be an
/// annotation type, and everything else is a class until somebody says otherwise.
pub fn missing_type_at(
    root: Node<'_>,
    source: &str,
    start: usize,
    end: usize,
) -> Option<MissingType> {
    let at = node_at(root, start)?;
    let named = matches!(at.kind(), "type_identifier" | "identifier");
    if !named || (end > start && (at.start_byte() > start || at.end_byte() < end)) {
        return None;
    }
    let name = text(&at, source).to_string();
    // A type name is capitalised, and without that this fires on every unresolved variable —
    // "create class `count`" is not a repair anybody wants offered.
    if !name.chars().next().is_some_and(char::is_uppercase) {
        return None;
    }
    let mut keyword = "class";
    let mut node = at;
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "super_interfaces" | "extends_interfaces" => {
                keyword = "interface";
                break;
            }
            "annotation" | "marker_annotation" => {
                keyword = "@interface";
                break;
            }
            // The places a type name is simply a type: stop, and leave it a class.
            "object_creation_expression" | "superclass" | "formal_parameter"
            | "local_variable_declaration" | "field_declaration" | "method_declaration"
            | "cast_expression" | "catch_type" | "array_type" => break,
            _ => node = parent,
        }
    }
    Some(MissingType { name, keyword })
}

/// The whole source of a new file declaring `name`.
///
/// Deliberately empty inside: a generated class with invented members is a class somebody has to
/// read before deleting. The package line is the one thing that must be right, because it is the
/// one thing the compiler will not let you fix by typing.
pub fn new_type_source(package: Option<&str>, keyword: &str, name: &str) -> String {
    let header = match package.filter(|p| !p.is_empty()) {
        Some(package) => format!("package {package};\n\n"),
        None => String::new(),
    };
    format!("{header}public {keyword} {name} {{\n}}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn run(source: &str, call: &str) -> Outcome {
        let tree = parse_java(source).unwrap();
        let at = source.find(call).unwrap();
        create_method(tree.root_node(), source, at, at + call.find('(').unwrap_or(call.len()))
    }

    #[test]
    fn a_call_that_reads_nothing_becomes_a_void_method() {
        let source = "class A {\n    void f() {\n        report();\n    }\n}";
        let Some(Ok(plan)) = run(source, "report()") else { panic!("no plan") };
        let applied = plan.apply(source);
        assert!(applied.contains("private void report() {"), "{applied}");
        assert!(applied.contains("throw new UnsupportedOperationException(\"TODO: report\");"), "{applied}");
    }

    /// The call site is the specification: the arguments' declared types, and their names.
    #[test]
    fn the_arguments_declared_types_and_names_become_the_signature() {
        let source = "class A {\n    void f(String label) {\n        int count = 2;\n        report(label, count, 3);\n    }\n}";
        let Some(Ok(plan)) = run(source, "report(label") else { panic!("no plan") };
        assert!(
            plan.apply(source).contains("private void report(String label, int count, int arg3) {"),
            "{}",
            plan.apply(source)
        );
    }

    #[test]
    fn the_declaration_it_initialises_names_the_return_type() {
        let source = "class A {\n    void f() {\n        java.util.List<String> rows = load();\n    }\n}";
        let Some(Ok(plan)) = run(source, "load()") else { panic!("no plan") };
        assert!(
            plan.apply(source).contains("private java.util.List<String> load() {"),
            "{}",
            plan.apply(source)
        );
    }

    #[test]
    fn a_condition_asks_for_a_boolean() {
        let source = "class A {\n    void f() {\n        if (ready()) {\n            return;\n        }\n    }\n}";
        let Some(Ok(plan)) = run(source, "ready()") else { panic!("no plan") };
        assert!(plan.apply(source).contains("private boolean ready() {"), "{}", plan.apply(source));
    }

    /// A `static` caller can only reach a `static` method — the stub has to compile at the one site
    /// that asked for it.
    #[test]
    fn a_static_caller_gets_a_static_method() {
        let source = "class A {\n    static void f() {\n        report();\n    }\n}";
        let Some(Ok(plan)) = run(source, "report()") else { panic!("no plan") };
        assert!(plan.apply(source).contains("private static void report() {"), "{}", plan.apply(source));
    }

    #[test]
    fn a_method_that_already_exists_is_not_offered() {
        let source = "class A {\n    void f() {\n        report();\n    }\n    void report() {}\n}";
        assert!(run(source, "report();").is_none());
    }

    #[test]
    fn a_call_on_another_object_says_where_it_belongs() {
        let source = "class A {\n    void f(B b) {\n        b.report();\n    }\n}\nclass B {}";
        let Some(Err(refusal)) = run(source, "report()") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("its own file"), "{}", refusal.reason);
    }

    fn missing(source: &str, needle: &str) -> MissingType {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap();
        missing_type_at(tree.root_node(), source, at, at + needle.len()).expect("a missing type")
    }

    #[test]
    fn a_name_in_an_implements_clause_has_to_be_an_interface() {
        let m = missing("class A implements Handler {\n}", "Handler");
        assert_eq!((m.name.as_str(), m.keyword), ("Handler", "interface"));
    }

    #[test]
    fn a_name_after_an_at_sign_has_to_be_an_annotation_type() {
        let m = missing("class A {\n    @Audited\n    void f() {}\n}", "Audited");
        assert_eq!(m.keyword, "@interface");
    }

    #[test]
    fn everything_else_is_a_class() {
        assert_eq!(
            missing("class A {\n    void f() {\n        new Widget();\n    }\n}", "Widget").keyword,
            "class"
        );
        assert_eq!(missing("class A extends Base {\n}", "Base").keyword, "class");
    }

    /// A lowercase name is a variable somebody has not declared, not a type.
    #[test]
    fn a_lowercase_name_is_not_offered_as_a_type() {
        let source = "class A {\n    void f() {\n        count();\n    }\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("count").unwrap();
        assert!(missing_type_at(tree.root_node(), source, at, at + 5).is_none());
    }

    #[test]
    fn a_new_file_carries_the_package_and_nothing_else() {
        assert_eq!(
            new_type_source(Some("it.acme.web"), "interface", "Handler"),
            "package it.acme.web;\n\npublic interface Handler {\n}\n"
        );
        assert_eq!(new_type_source(None, "class", "Widget"), "public class Widget {\n}\n");
    }

    /// The stub lands after the method that called it, not at the end of the class — near its use is
    /// where it gets written.
    #[test]
    fn the_stub_lands_after_the_method_that_calls_it() {
        let source = "class A {\n    void f() {\n        report();\n    }\n\n    void z() {}\n}";
        let Some(Ok(plan)) = run(source, "report()") else { panic!("no plan") };
        let applied = plan.apply(source);
        let stub = applied.find("private void report").unwrap();
        assert!(stub < applied.find("void z()").unwrap(), "{applied}");
    }
}
