//! The assignment a constructor is missing, as ghost text: add a field and a parameter for it, and the constructor's last line follows.
//!
//! On the empty line inside a constructor body, the first instance field that no assignment in that body sets and that a parameter of
//! the same name could set is offered as `this.field = field;`. It is what `flc_derived` writes for a constructor that does not exist
//! yet, for the one that does and has just been given a parameter. Nothing is guessed: a field with a value, a `static` one, and
//! one the body already sets are left alone, and so is a parameter that has no field by its name.

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

use crate::flc_derived::text_of;

/// A line that completes a constructor, and how sure that is.
pub(crate) struct Missing {
    pub text: String,
    pub confidence: f32,
}

pub(crate) fn suggest(source: &str, offset: usize) -> Option<Missing> {
    if !source.is_char_boundary(offset) {
        return None;
    }
    let tree = parse_java(source)?;
    let at = tree.root_node().descendant_for_byte_range(offset, offset)?;
    let body = std::iter::successors(Some(at), |n| n.parent()).find(|n| n.kind() == "constructor_body")?;
    let ctor = body.parent().filter(|c| c.kind() == "constructor_declaration")?;
    // the line under the caret is empty: nothing but blanks between the last statement (or the brace) and the caret
    let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
    let line_end = source[offset..].find('\n').map_or(source.len(), |p| offset + p);
    if !source[line_start..line_end].trim().is_empty() {
        return None;
    }
    let class_body = ctor.parent().filter(|c| c.kind() == "class_body")?;
    let params = parameter_names(ctor, source);
    let assigned = assigned_names(body, source);
    let field = fields_without_value(class_body, source).into_iter().find(|f| params.contains(f) && !assigned.contains(f))?;
    Some(Missing { text: format!("this.{field} = {field};"), confidence: 0.9 })
}

fn parameter_names(ctor: Node<'_>, source: &str) -> Vec<String> {
    let Some(params) = ctor.child_by_field_name("parameters") else { return Vec::new() };
    let mut cursor = params.walk();
    params
        .named_children(&mut cursor)
        .filter(|p| matches!(p.kind(), "formal_parameter" | "spread_parameter"))
        .filter_map(|p| p.child_by_field_name("name").or_else(|| last_identifier(p)))
        .map(|n| text_of(n, source).to_string())
        .collect()
}

fn last_identifier(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).filter(|c| c.kind() == "variable_declarator" || c.kind() == "identifier").last().and_then(|c| {
        if c.kind() == "variable_declarator" { c.child_by_field_name("name") } else { Some(c) }
    })
}

/// The names `this.x = …` and `x = …` assign anywhere in the body.
fn assigned_names(body: Node<'_>, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "assignment_expression" {
            if let Some(left) = node.child_by_field_name("left") {
                let name = match left.kind() {
                    "field_access" => left.child_by_field_name("field").map(|f| text_of(f, source).to_string()),
                    "identifier" => Some(text_of(left, source).to_string()),
                    _ => None,
                };
                out.extend(name);
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    out
}

/// The instance fields declared without a value, in declaration order.
fn fields_without_value(class_body: Node<'_>, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = class_body.walk();
    for field in class_body.named_children(&mut cursor).filter(|n| n.kind() == "field_declaration") {
        let mut c = field.walk();
        let modifiers = field.named_children(&mut c).find(|n| n.kind() == "modifiers").map(|m| text_of(m, source)).unwrap_or("");
        if modifiers.split_whitespace().any(|w| w == "static") {
            continue;
        }
        let mut c = field.walk();
        for decl in field.named_children(&mut c).filter(|n| n.kind() == "variable_declarator") {
            if decl.child_by_field_name("value").is_none() {
                out.extend(decl.child_by_field_name("name").map(|n| text_of(n, source).to_string()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ctor_body: &str, params: &str) -> Option<Missing> {
        let source = format!("class Order {{\n    private long id;\n    private String customer;\n    private static int made;\n    private int total = 0;\n    Order({params}) {{\n{ctor_body}        \n    }}\n}}\n");
        let offset = source.rfind("        \n").unwrap() + 8;
        suggest(&source, offset)
    }

    #[test]
    fn the_first_field_with_a_parameter_and_no_assignment_is_offered() {
        let m = at("        this.id = id;\n", "long id, String customer").expect("customer is missing");
        assert_eq!(m.text, "this.customer = customer;");
        let m = at("", "long id, String customer").expect("id is first");
        assert_eq!(m.text, "this.id = id;");
    }

    #[test]
    fn a_complete_constructor_a_field_with_a_value_or_without_a_parameter_gets_nothing() {
        assert!(at("        this.id = id;\n        this.customer = customer;\n", "long id, String customer").is_none());
        assert!(at("        this.id = id;\n", "long id, int total").is_none(), "total has a value, made is static");
        assert!(at("", "long other").is_none(), "no parameter by a field's name");
        assert!(at("        id = id2;\n", "long id, long id2").is_none(), "id is set by its bare name, and customer has no parameter");
    }

    #[test]
    fn a_caret_on_a_line_with_text_or_outside_a_constructor_gets_nothing() {
        let source = "class A {\n int x;\n A(int x) {\n  foo();\n }\n void m(int x) {\n  \n }\n}\n";
        assert!(suggest(source, source.find("  foo();").unwrap() + 4).is_none());
        assert!(suggest(source, source.rfind("  \n").unwrap() + 2).is_none());
    }
}
