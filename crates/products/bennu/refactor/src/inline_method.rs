//! **Inline method** — replace a call with what the method does.
//!
//! Narrow on purpose. Inlining is only safe when the body is a single expression, and the general
//! case — several statements, control flow, a `this` that means something different at the call
//! site — is where every implementation of this either refuses or produces code that compiles and
//! is wrong. So this does the case that is both common and decidable:
//!
//! > a method whose whole body is `return <expression>;` (or one expression statement, for `void`),
//! > called somewhere in the same file.
//!
//! That covers the delegating one-liners and the getters that accumulate in a legacy codebase,
//! which is what people actually reach for this on.
//!
//! ## What is checked before anything is written
//!
//! - **One declaration of that name.** An overloaded method cannot be resolved from the call's text
//!   alone — that needs the type of every argument — so it is refused rather than guessed.
//! - **Not recursive.** A body that calls itself would be inlined into itself.
//! - **No `super.`** — it means something different outside the method that declares it.
//! - **Each parameter used at most once**, unless its argument is a plain name or literal.
//!   `f(next())` inlined into a body that reads the parameter twice runs `next()` twice.
//!
//! ## Substitution
//!
//! Parameters are replaced by their arguments **structurally** — the identifier nodes of the body,
//! not a string search, so a local or a field that happens to share a parameter's name is untouched.
//! Compound arguments are parenthesised on the way in, and the whole inlined expression is
//! parenthesised where the call site binds tighter, for the reason [`crate::inline_var`] explains.

use tree_sitter::Node;

use crate::plan::{Outcome, Plan, RefactorEdit, Refusal};
use crate::selection::{descendants, enclosing, identifiers, is_expression, node_at, text};

const ID: (&str, &str) = ("inline-method", "Inline method");

/// Plan an *inline method* for the call under the caret.
pub fn inline_method(root: Node<'_>, source: &str, offset: usize) -> Outcome {
    let (id, label) = ID;
    let call = enclosing(node_at(root, offset)?, &["method_invocation"])?;
    // A qualified call on something other than `this` is a call on another object, whose body is not
    // in this file and whose `this` is not this one.
    if let Some(receiver) = call.child_by_field_name("object") {
        if !matches!(text(&receiver, source), "this") {
            return None;
        }
    }
    let name_node = call.child_by_field_name("name")?;
    let name = text(&name_node, source);

    // Declared by the SAME type the call sits in. Searching the whole file finds a method of a
    // sibling or nested class, and injecting its body here reads that class's private fields from
    // outside it — `contentEnd has private access`, from a refactoring that looked local.
    let here = crate::selection::enclosing_type(call).map(|t| t.id());
    let declarations: Vec<Node<'_>> = descendants(root, "method_declaration")
        .into_iter()
        .filter(|d| d.child_by_field_name("name").map(|n| text(&n, source)) == Some(name))
        .filter(|d| crate::selection::enclosing_type(*d).map(|t| t.id()) == here)
        .collect();
    let declaration = match declarations.as_slice() {
        [] => return None, // declared elsewhere — this crate reads one file
        [only] => *only,
        _ => {
            return Some(Err(Refusal::new(
                id,
                label,
                format!("`{name}` is overloaded — which one a call means needs the type of every argument"),
            )))
        }
    };

    // A generic method's body is typed by inference at each call. Substituting the text loses the
    // inference — `Object` where the call site wanted `E` — and no rearrangement of the text gets it
    // back.
    if declaration.child_by_field_name("type_parameters").is_some() {
        return Some(Err(Refusal::new(
            id,
            label,
            format!("`{name}` is generic, and its type arguments are inferred per call — inlining the text loses them"),
        )));
    }
    let body = declaration.child_by_field_name("body")?;
    let Some(expression) = single_expression_body(&body) else {
        return Some(Err(Refusal::new(
            id,
            label,
            format!("`{name}` does more than compute one expression — only a one-line body can be inlined"),
        )));
    };
    if descendants(body, "method_invocation")
        .iter()
        .any(|c| c.child_by_field_name("name").map(|n| text(&n, source)) == Some(name))
    {
        return Some(Err(Refusal::new(id, label, format!("`{name}` calls itself"))));
    }
    if text(&body, source).contains("super.") {
        return Some(Err(Refusal::new(
            id,
            label,
            format!("`{name}` uses `super`, which means something else outside the method that declares it"),
        )));
    }

    let parameters = parameter_names(&declaration, source);
    let arguments = argument_texts(&call, source);
    if parameters.len() != arguments.len() {
        return Some(Err(Refusal::new(
            id,
            label,
            "the call does not pass one argument per parameter — a varargs or a mismatch",
        )));
    }

    // The substitutions, as edits into the BODY's text.
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for (index, parameter) in parameters.iter().enumerate() {
        let argument = &arguments[index];
        let sites: Vec<Node<'_>> = identifiers(expression)
            .into_iter()
            .filter(|n| text(n, source) == parameter)
            .filter(|n| !is_field_name(n))
            .collect();
        if sites.len() > 1 && !is_simple(argument) {
            return Some(Err(Refusal::new(
                id,
                label,
                format!(
                    "`{parameter}` is read more than once and the call passes an expression for it — \
                     inlining would evaluate it twice"
                ),
            )));
        }
        let written = match needs_wrapping(argument) {
            true => format!("({argument})"),
            false => argument.clone(),
        };
        for site in sites {
            replacements.push((site.start_byte(), site.end_byte(), written.clone()));
        }
    }

    let inlined = substitute(source, expression.start_byte(), expression.end_byte(), &mut replacements);
    let text_at_call = match call_needs_parentheses(&expression, &call) {
        true => format!("({inlined})"),
        false => inlined,
    };

    Some(Ok(Plan::new(
        id,
        label,
        vec![RefactorEdit::new(call.start_byte(), call.end_byte(), text_at_call, "call")],
    )
    .named(name.to_string())))
}

/// The one expression a body computes, when that is all it does.
fn single_expression_body<'t>(body: &Node<'t>) -> Option<Node<'t>> {
    let mut cursor = body.walk();
    let statements: Vec<Node<'t>> = body.named_children(&mut cursor).collect();
    let [only] = statements.as_slice() else { return None };
    match only.kind() {
        "return_statement" => only.named_child(0),
        "expression_statement" => only.named_child(0),
        _ => None,
    }
}

fn parameter_names(declaration: &Node<'_>, source: &str) -> Vec<String> {
    declaration
        .child_by_field_name("parameters")
        .map(|params| {
            descendants(params, "formal_parameter")
                .iter()
                .filter_map(|p| p.child_by_field_name("name"))
                .map(|n| text(&n, source).to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn argument_texts(call: &Node<'_>, source: &str) -> Vec<String> {
    call.child_by_field_name("arguments")
        .map(|args| {
            let mut cursor = args.walk();
            args.named_children(&mut cursor).map(|a| text(&a, source).to_string()).collect()
        })
        .unwrap_or_default()
}

/// Whether an identifier is the *field* half of `a.b` — a name that is not the parameter even when
/// it is spelled like one.
fn is_field_name(node: &Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        matches!(p.kind(), "field_access" | "method_invocation")
            && p.child_by_field_name("field").or_else(|| p.child_by_field_name("name")).map(|n| n.id())
                == Some(node.id())
    })
}

/// Whether an argument can be substituted more than once without changing what runs.
fn is_simple(argument: &str) -> bool {
    let trimmed = argument.trim();
    !trimmed.is_empty()
        && trimmed.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '"')
        && !trimmed.contains('(')
}

/// Whether an argument needs wrapping on the way into the body.
fn needs_wrapping(argument: &str) -> bool {
    let trimmed = argument.trim();
    trimmed.contains(' ') && !trimmed.starts_with('(')
}

/// Whether the inlined expression needs wrapping where the call was — the same rule as
/// [`crate::inline_var`], read off the node kinds.
fn call_needs_parentheses(expression: &Node<'_>, call: &Node<'_>) -> bool {
    let compound = matches!(
        expression.kind(),
        "binary_expression" | "ternary_expression" | "assignment_expression"
            | "instanceof_expression" | "lambda_expression" | "cast_expression"
    );
    if !compound {
        return false;
    }
    let Some(parent) = call.parent() else { return false };
    let standalone = matches!(
        parent.kind(),
        "argument_list"
            | "expression_statement"
            | "variable_declarator"
            | "return_statement"
            | "parenthesized_expression"
            | "array_initializer"
    );
    !standalone && is_expression(&parent)
}

/// The body's text with the parameter sites replaced. Back to front, so nothing that is still to be
/// written has moved — the same reason [`crate::plan::Plan`] sorts its edits.
fn substitute(
    source: &str,
    start: usize,
    end: usize,
    replacements: &mut Vec<(usize, usize, String)>,
) -> String {
    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = source[start..end].to_string();
    for (site_start, site_end, text) in replacements.iter() {
        if *site_start < start || *site_end > end {
            continue;
        }
        out.replace_range(site_start - start..site_end - start, text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn run(source: &str, needle: &str) -> Outcome {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap() + 1;
        inline_method(tree.root_node(), source, at)
    }

    #[test]
    fn a_one_line_method_becomes_its_expression_at_the_call() {
        let source = "class A {\n    int twice(int n) { return n * 2; }\n    int f(int a) {\n        return twice(a);\n    }\n}";
        let Some(Ok(plan)) = run(source, "twice(a)") else { panic!("no plan") };
        assert!(plan.apply(source).contains("return a * 2;"), "{}", plan.apply(source));
    }

    /// The substitution is structural: a local spelled like the parameter is not touched.
    #[test]
    fn an_argument_is_substituted_where_the_parameter_is_read_and_nowhere_else() {
        let source = "class A {\n    int sum(int n) { return n + FIELD_n; }\n    int FIELD_n = 1;\n    int f(int x) {\n        return sum(x);\n    }\n}";
        let Some(Ok(plan)) = run(source, "sum(x)") else { panic!("no plan") };
        let applied = plan.apply(source);
        assert!(applied.contains("return x + FIELD_n;"), "{applied}");
    }

    #[test]
    fn a_compound_argument_is_parenthesised_on_the_way_in() {
        let source = "class A {\n    int twice(int n) { return n * 2; }\n    int f(int a, int b) {\n        return twice(a + b);\n    }\n}";
        let Some(Ok(plan)) = run(source, "twice(a + b)") else { panic!("no plan") };
        assert!(plan.apply(source).contains("return (a + b) * 2;"), "{}", plan.apply(source));
    }

    #[test]
    fn the_inlined_expression_is_parenthesised_where_the_call_site_binds_tighter() {
        let source = "class A {\n    int plus(int n) { return n + 1; }\n    int f(int a) {\n        return plus(a) * 3;\n    }\n}";
        let Some(Ok(plan)) = run(source, "plus(a)") else { panic!("no plan") };
        assert!(plan.apply(source).contains("return (a + 1) * 3;"), "{}", plan.apply(source));
    }

    #[test]
    fn an_overloaded_method_is_refused_rather_than_guessed() {
        let source = "class A {\n    int f(int n) { return n; }\n    int f(String s) { return 0; }\n    int g() {\n        return f(1);\n    }\n}";
        let Some(Err(refusal)) = run(source, "f(1)") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("overloaded"), "{}", refusal.reason);
    }

    /// Regression: the method was looked up across the WHOLE file, so a call in one class inlined a
    /// method of a sibling or nested one — and read its private fields from outside.
    #[test]
    fn a_method_of_another_class_in_the_same_file_is_not_inlined() {
        let source = "class A {\n    int f() {\n        return size();\n    }\n}\nclass B {\n    private int n;\n    int size() { return n; }\n}";
        assert!(run(source, "size()").is_none());
    }

    /// A generic method's type arguments are inferred at each call; the text does not carry them.
    #[test]
    fn a_generic_method_is_refused() {
        let source = "class A {\n    Object f() {\n        return first();\n    }\n    <T> T first() { return null; }\n}";
        let Some(Err(refusal)) = run(source, "first()") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("generic"), "{}", refusal.reason);
    }

    #[test]
    fn a_multi_statement_body_is_refused() {
        let source = "class A {\n    int f(int n) { int t = n; return t; }\n    int g() {\n        return f(1);\n    }\n}";
        let Some(Err(refusal)) = run(source, "f(1)") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("more than compute one expression"), "{}", refusal.reason);
    }

    #[test]
    fn a_recursive_method_is_refused() {
        let source = "class A {\n    int f(int n) { return f(n); }\n    int g() {\n        return f(1);\n    }\n}";
        let Some(Err(refusal)) = run(source, "f(1);\n") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("calls itself"), "{}", refusal.reason);
    }

    /// The one that would silently run somebody's call twice.
    #[test]
    fn a_parameter_read_twice_with_an_impure_argument_is_refused() {
        let source = "class A {\n    int square(int n) { return n * n; }\n    int next() { return 2; }\n    int g() {\n        return square(next());\n    }\n}";
        let Some(Err(refusal)) = run(source, "square(next())") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("twice"), "{}", refusal.reason);
    }

    /// …and a plain name read twice is fine, which is most of what a one-liner does.
    #[test]
    fn a_parameter_read_twice_with_a_plain_argument_is_allowed() {
        let source = "class A {\n    int square(int n) { return n * n; }\n    int g(int a) {\n        return square(a);\n    }\n}";
        let Some(Ok(plan)) = run(source, "square(a)") else { panic!("no plan") };
        assert!(plan.apply(source).contains("return a * a;"), "{}", plan.apply(source));
    }

    /// A call on another object is not this file's business.
    #[test]
    fn a_call_on_another_object_is_left_alone() {
        let source = "class A {\n    int f() { return 1; }\n    int g(B b) {\n        return b.f();\n    }\n}\nclass B { int f() { return 2; } }";
        assert!(run(source, "b.f()").is_none());
    }
}
