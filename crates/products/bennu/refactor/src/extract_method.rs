//! **Extract method** — turn a run of statements into a method, and a call where they were.
//!
//! ## The three questions, and why each one can refuse
//!
//! Extracting is not moving text. Three things have to be true, and this asks each of them of the
//! parse rather than assuming:
//!
//! 1. **What does it need?** Every local declared *before* the selection and read *inside* it
//!    becomes a parameter, typed as it was declared. A local this cannot type — declared with
//!    `var` — is a refusal rather than a guess, because the signature is the one part of the result
//!    nobody re-reads.
//! 2. **What does it give back?** A local declared *inside* the selection and read *after* it is
//!    the return value. Exactly one is a method; **two is not**, and Java has no way to write it, so
//!    that is refused in the words of the code — *"the selection produces `total` and `count`, and
//!    a method can only return one"*.
//! 3. **Can it be left?** A `return`, `break` or `continue` inside the selection that jumps out of
//!    it does not survive being moved into another method. Refused, again by name.
//!
//! ## What it deliberately does not do
//!
//! Reassigning a parameter's *caller* variable. `total += x` inside the selection, where `total` is
//! declared before it, changes a variable the caller can see — and Java passes by value, so the
//! extracted method cannot. This is the case that produces silently wrong code in every
//! naive implementation, and it is refused unless `total` is the single value returned.

use tree_sitter::Node;

use crate::plan::{Outcome, Plan, RefactorEdit, Refusal, ThrowsSlot};
use crate::selection::{
    descendants, enclosing_callable, identifiers, indent_at, is_static, newline, statements_for,
    text,
};

const ID: (&str, &str) = ("extract-method", "Extract method");

/// A local the extracted method needs handed to it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Parameter {
    name: String,
    /// The type exactly as it was declared, so the signature reads like the code around it.
    type_text: String,
}

/// Plan an *extract method* over a selection.
pub fn extract_method(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = ID;
    let statements = statements_for(root, source, start, end)?;
    let first = *statements.first()?;
    let last = *statements.last()?;
    let method = enclosing_callable(first)?;
    // A selection at the top level of a class is not a body; nothing to extract from.
    let body = method.child_by_field_name("body")?;
    if first.start_byte() < body.start_byte() {
        return None;
    }

    // `this(…)` / `super(…)` must be the first statement of a constructor, so it cannot become a
    // call to anything.
    if statements.iter().any(|s| !descendants(*s, "explicit_constructor_invocation").is_empty()) {
        return Some(Err(Refusal::new(
            id,
            label,
            "the selection contains a `this(…)` or `super(…)` call, which has to stay the first statement of its constructor",
        )));
    }
    // A selection that ENDS the flow cannot become a call: the compiler does not know that
    // `extracted();` always throws, so every path the throw used to end is suddenly a path with no
    // `return` on it.
    if last.kind() == "throw_statement" {
        return Some(Err(Refusal::new(
            id,
            label,
            "the selection ends by throwing, and a call cannot tell the compiler that the code after it is unreachable",
        )));
    }
    if let Some(jump) = escaping_jump(&statements, source) {
        return Some(Err(Refusal::new(
            id,
            label,
            format!("the selection contains a `{jump}` that leaves it — a method cannot carry that out"),
        )));
    }

    if let Some(field) = assigned_final_field(&statements, &method, source) {
        return Some(Err(Refusal::new(
            id,
            label,
            format!("the selection assigns `{field}`, a final field — only a constructor may do that"),
        )));
    }

    let mut declared_before = locals_before(&method, first.start_byte(), source);
    let declared_inside = locals_in(&statements, source);
    let read_inside = names_read(&statements, source);

    // The lambdas the selection sits inside declare locals too, and `locals_before` walks the
    // METHOD, so it never sees them. An untyped one cannot be passed at all — see the function.
    for (name, declared) in lambda_parameters_around(first, &method, source) {
        if !read_inside.contains(&name) {
            continue;
        }
        match declared {
            Some(type_text) => declared_before.push((name, type_text)),
            None => {
                return Some(Err(Refusal::new(
                    id,
                    label,
                    format!(
                        "`{name}` is a lambda parameter whose type is not written, so it cannot be \
                         given to a method — write the parameter's type first"
                    ),
                )))
            }
        }
    }

    // 1. Parameters: what the selection reads and did not declare.
    let mut parameters: Vec<Parameter> = Vec::new();
    for (name, type_text) in &declared_before {
        if !read_inside.contains(name) {
            continue;
        }
        // A name declared twice in two sibling blocks appears twice here; the selection can only
        // see one of them, and a signature with a repeated parameter does not compile.
        if parameters.iter().any(|p| &p.name == name) {
            continue;
        }
        if crate::selection::is_inferred_type(type_text) {
            return Some(Err(Refusal::new(
                id,
                label,
                format!(
                    "`{name}` is declared with `{}`, so its type is inferred and cannot be written into a signature",
                    type_text.trim()
                ),
            )));
        }
        parameters.push(Parameter { name: name.clone(), type_text: type_text.clone() });
    }

    // 2. The return value: what the selection declares and the code after it reads.
    let after_start = last.end_byte();
    let read_after = names_read_in_range(&method, after_start, body.end_byte(), source);
    let mut produced: Vec<(String, String)> = declared_inside
        .iter()
        .filter(|(name, _)| read_after.contains(name))
        .cloned()
        .collect();
    // A local the selection DECLARES but never gives a value cannot be returned — and dropping it
    // from the return list is worse than returning it, which is what the first attempt at this did:
    // moving the statements away takes the declaration with them, so every later use loses its
    // symbol. The selection is simply the wrong one, and saying which piece is missing is the
    // repair.
    if let Some((name, _)) = produced.iter().find(|(name, _)| !assigns(&statements, name, source)) {
        return Some(Err(Refusal::new(
            id,
            label,
            format!(
                "the selection declares `{name}` without giving it a value and the code after it \
                 reads `{name}` — extend the selection to where `{name}` is assigned"
            ),
        )));
    }
    produced.sort();
    if produced.len() > 1 {
        let names: Vec<String> = produced.iter().map(|(n, _)| format!("`{n}`")).collect();
        return Some(Err(Refusal::new(
            id,
            label,
            format!(
                "the selection produces {} and a method can only return one — extract a smaller piece, \
                 or introduce a type to hold them",
                names.join(" and ")
            ),
        )));
    }
    let returned = produced.into_iter().next();
    if let Some((name, type_text)) = &returned {
        if type_text == "var" {
            return Some(Err(Refusal::new(
                id,
                label,
                format!("`{name}` is declared with `var`, so the return type cannot be written"),
            )));
        }
    }

    // 3. The case that quietly produces wrong code: the selection assigns a caller-visible local
    // that is not the one value coming back.
    let assigned = assigned_names(&statements, source);
    for (name, _) in &declared_before {
        if !assigned.contains(name) {
            continue;
        }
        if returned.as_ref().map(|(r, _)| r) == Some(name) {
            continue;
        }
        return Some(Err(Refusal::new(
            id,
            label,
            format!(
                "the selection assigns `{name}`, which is declared outside it — Java passes by value, \
                 so the change would be lost"
            ),
        )));
    }

    let name = suggest_method_name(&statements, source);
    let nl = newline(source);
    let call_indent = indent_at(source, first.start_byte());
    let method_indent = indent_at(source, method.start_byte());
    let body_indent = format!("{method_indent}    ");

    let signature_params = parameters
        .iter()
        .map(|p| format!("{} {}", p.type_text, p.name))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = parameters.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ");
    let return_type = returned.as_ref().map(|(_, t)| t.clone()).unwrap_or_else(|| "void".into());
    let modifiers = if is_static(&method, source) { "private static " } else { "private " };
    // A type parameter the METHOD declares does not exist in a sibling method, so the extracted one
    // has to declare it too. A CLASS's type parameter needs nothing — it is in scope for every
    // member — which is why this looks like it works until the first `<T> void f(…)`.
    let type_params = borrowed_type_parameters(&method, &parameters, &return_type, source);
    // The enclosing method's `throws`, carried over. Precisely which of them the moved statements
    // can actually raise is a question for the resolver; declaring the same set is sound — the body
    // was legal inside a method that declared them — and it is what keeps a checked exception from
    // becoming an "unreported exception" the moment it moves.
    let throws = declared_throws(&method, first, source);

    // The moved statements, re-indented from the block they were in to the block they are going to.
    let moved = reindent(
        &source[first.start_byte()..last.end_byte()],
        &call_indent,
        &body_indent,
        nl,
    );
    let return_line = match &returned {
        Some((returned_name, _)) => format!("{nl}{body_indent}return {returned_name};"),
        None => String::new(),
    };
    // The signature sits where the enclosing method's does; its body one level in. Writing both at
    // the same depth is the tell that a refactoring pasted text instead of placing it.
    // Built in two halves so the `throws` has an address: everything up to the `)` is the offset a
    // caller with a resolver writes the real clause at. See `ThrowsSlot`.
    let head = format!(
        "{nl}{nl}{method_indent}{modifiers}{type_params}{return_type} {name}({signature_params})"
    );
    let throws_at = head.len();
    let extracted = format!(
        "{head}{throws} {{{nl}{body_indent}{moved}{return_line}{nl}{method_indent}}}"
    );

    let call = match &returned {
        Some((returned_name, type_text)) => {
            format!("{type_text} {returned_name} = {name}({arguments});")
        }
        None => format!("{name}({arguments});"),
    };

    let insert_at = method.end_byte();
    let plan = Plan::new(
        id,
        label,
        vec![
            RefactorEdit::new(first.start_byte(), last.end_byte(), call, "call"),
            RefactorEdit::new(insert_at, insert_at, extracted, "declaration"),
        ],
    )
    .named(name);
    let slot_index = plan.edits.iter().position(|e| e.reason == "declaration")?;
    Some(Ok(plan.needing_throws(ThrowsSlot {
        start: first.start_byte(),
        end: last.end_byte(),
        edit_index: slot_index,
        at: throws_at,
        placeholder: throws,
    })))
}

/// A `return` / `break` / `continue` that leaves the selection.
///
/// A `break` inside a loop **that is itself inside the selection** is fine — it does not leave. So
/// this counts loop and switch depth as it walks, which is the difference between refusing a real
/// problem and refusing every extraction that contains a `for`.
fn escaping_jump(statements: &[Node<'_>], source: &str) -> Option<&'static str> {
    // The labels the selection itself declares: a `break outer;` is only local if `outer:` came
    // with it.
    let labels: Vec<String> = statements
        .iter()
        .flat_map(|s| {
            // The statement ITSELF counts: `descendants` does not include the node it is asked
            // about, and selecting a labelled loop whole is the ordinary way to move one.
            let mut all = descendants(*s, "labeled_statement");
            if s.kind() == "labeled_statement" {
                all.push(*s);
            }
            all
        })
        .filter_map(|l| l.named_child(0).map(|n| text(&n, source).to_string()))
        .collect();
    for statement in statements {
        if let Some(kind) = escaping_in(statement, source, 0, &labels) {
            return Some(kind);
        }
    }
    None
}

fn escaping_in(
    node: &Node<'_>,
    source: &str,
    loop_depth: usize,
    labels: &[String],
) -> Option<&'static str> {
    let deeper = match node.kind() {
        "for_statement" | "enhanced_for_statement" | "while_statement" | "do_statement"
        | "switch_expression" => loop_depth + 1,
        // A lambda or an anonymous class has its own `return`, which is not this method's.
        "lambda_expression" | "object_creation_expression" | "method_declaration" => return None,
        _ => loop_depth,
    };
    match node.kind() {
        "return_statement" => return Some("return"),
        // A LABELLED jump leaves the selection whenever its label does — being inside a loop here
        // says nothing, because the label names a loop further out.
        "break_statement" | "continue_statement" => {
            let target = node.named_child(0).map(|n| text(&n, source).to_string());
            match target {
                Some(name) if !labels.contains(&name) => {
                    return Some(if node.kind() == "break_statement" { "break" } else { "continue" })
                }
                None if loop_depth == 0 => {
                    return Some(if node.kind() == "break_statement" { "break" } else { "continue" })
                }
                _ => {}
            }
        }
        "yield_statement" if loop_depth == 0 => return Some("yield"),
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(kind) = escaping_in(&child, source, deeper, labels) {
            return Some(kind);
        }
    }
    let _ = source;
    None
}

/// What the extracted method must declare it throws, as ` throws A, B`.
///
/// Two sources, and the second is the one that is easy to miss. The enclosing method's own `throws`
/// covers what escapes it. But a checked exception the body raises and a surrounding `try` CATCHES
/// never reaches that clause — and once the body moves into a method of its own, the call has to be
/// able to throw it or the `try` around it stops compiling ("exception is never thrown in body of
/// corresponding try statement"). So every `catch` between the selection and the method contributes
/// its types too.
///
/// Over-declaring is safe: `throws IOException` on a method that cannot raise it is legal, and the
/// `try` that prompted it is right there to catch it. Under-declaring is not.
fn declared_throws(method: &Node<'_>, first: Node<'_>, source: &str) -> String {
    let mut kinds: Vec<String> = Vec::new();
    let mut push = |text: &str| {
        for part in text.split('|') {
            let part = part.trim().to_string();
            if !part.is_empty() && !kinds.contains(&part) {
                kinds.push(part);
            }
        }
    };

    let mut cursor = method.walk();
    let declared = method
        .named_children(&mut cursor)
        .find(|c| c.kind() == "throws")
        .map(|c| text(&c, source).trim_start_matches("throws").trim().to_string());
    if let Some(declared) = declared {
        for part in declared.split(',') {
            push(part);
        }
    }

    let mut node = Some(first);
    while let Some(n) = node {
        if n.id() == method.id() {
            break;
        }
        if n.kind() == "try_statement" || n.kind() == "try_with_resources_statement" {
            for catch_type in descendants(n, "catch_type") {
                push(text(&catch_type, source));
            }
        }
        node = n.parent();
    }

    if kinds.is_empty() {
        String::new()
    } else {
        format!(" throws {}", kinds.join(", "))
    }
}

/// The method's own type parameters that the extracted signature uses, rendered as `<T, U> ` — or
/// empty when it uses none.
///
/// Verbatim, bounds included: `<T extends Comparable<T>>` means something the name alone does not,
/// and a bound dropped in the move is a signature that accepts more than the body can handle.
fn borrowed_type_parameters(
    method: &Node<'_>,
    parameters: &[Parameter],
    return_type: &str,
    source: &str,
) -> String {
    let Some(declared) = method.child_by_field_name("type_parameters") else {
        return String::new();
    };
    // Whole identifiers only: a `T` inside `Type` is not the type parameter `T`.
    let mut used: Vec<&str> = Vec::new();
    let signature: String = parameters
        .iter()
        .map(|p| p.type_text.as_str())
        .chain(std::iter::once(return_type))
        .collect::<Vec<_>>()
        .join(" ");
    let words: Vec<&str> =
        signature.split(|c: char| !c.is_alphanumeric() && c != '_').filter(|w| !w.is_empty()).collect();

    let mut cursor = declared.walk();
    let mut kept: Vec<String> = Vec::new();
    for parameter in declared.named_children(&mut cursor) {
        if parameter.kind() != "type_parameter" {
            continue;
        }
        let Some(name) = parameter.named_child(0).map(|n| text(&n, source)) else { continue };
        if words.contains(&name) && !used.contains(&name) {
            used.push(name);
            kept.push(text(&parameter, source).to_string());
        }
    }
    if kept.is_empty() {
        String::new()
    } else {
        format!("<{}> ", kept.join(", "))
    }
}

/// The parameters of every lambda between `node` and the method it is in, with their declared type
/// when they have one.
///
/// A lambda's parameter is a local like any other to the code inside it, and invisible to
/// [`locals_before`], which walks the method. Missing it does not produce a missing argument: it
/// produces a method whose `row` resolves to nothing, or — worse, and silently — to a field of the
/// same name.
///
/// The type is the catch. `row -> …` writes none, and a type that was never written cannot be
/// written into a signature; the compiler infers it from the functional interface, which is a
/// question for the resolver and not for this crate. So an untyped parameter the selection reads is
/// refused, for exactly the reason `var` is.
fn lambda_parameters_around<'t>(
    node: Node<'t>,
    method: &Node<'t>,
    source: &str,
) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut current = node.parent();
    while let Some(n) = current {
        if n.id() == method.id() {
            break;
        }
        if n.kind() == "lambda_expression" {
            if let Some(params) = n.child_by_field_name("parameters") {
                match params.kind() {
                    // `row -> …`
                    "identifier" => out.push((text(&params, source).to_string(), None)),
                    // `(row, other) -> …`
                    "inferred_parameters" => {
                        for id in descendants(params, "identifier") {
                            out.push((text(&id, source).to_string(), None));
                        }
                    }
                    // `(String row) -> …` — the one case that CAN be passed on.
                    _ => {
                        for param in descendants(params, "formal_parameter") {
                            if let (Some(t), Some(name)) = (
                                param.child_by_field_name("type"),
                                param.child_by_field_name("name"),
                            ) {
                                out.push((
                                    text(&name, source).to_string(),
                                    Some(text(&t, source).to_string()),
                                ));
                            }
                        }
                    }
                }
            }
        }
        current = n.parent();
    }
    out
}

/// Whether the selection gives `name` a value — an initialiser or an assignment.
fn assigns(statements: &[Node<'_>], name: &str, source: &str) -> bool {
    statements.iter().any(|statement| {
        let initialised = descendants(*statement, "variable_declarator").iter().any(|d| {
            d.child_by_field_name("name").is_some_and(|n| text(&n, source) == name)
                && d.child_by_field_name("value").is_some()
        });
        let assigned = descendants(*statement, "assignment_expression").iter().any(|a| {
            a.child_by_field_name("left").is_some_and(|l| text(&l, source) == name)
        });
        initialised || assigned
    })
}

/// A final field of the enclosing type that the selection assigns, if any.
///
/// `final` fields are assignable only in a constructor or an initialiser, so moving such an
/// assignment into a method produces code that does not compile — and the selection that does it is
/// the most ordinary one there is: the body of a constructor.
fn assigned_final_field(
    statements: &[Node<'_>],
    method: &Node<'_>,
    source: &str,
) -> Option<String> {
    let type_decl = crate::selection::enclosing_type(*method)?;
    let finals = final_field_names(&type_decl, source);
    if finals.is_empty() {
        return None;
    }
    // A bare name that this method declares is a local, not the field it happens to share a name
    // with — refusing on those would refuse half the constructors in a codebase.
    let locals: Vec<String> =
        locals_before(method, usize::MAX, source).into_iter().map(|(n, _)| n).collect();
    for statement in statements {
        for assignment in descendants(*statement, "assignment_expression") {
            let Some(left) = assignment.child_by_field_name("left") else { continue };
            let name = match left.kind() {
                "field_access" => left.child_by_field_name("field").map(|f| text(&f, source).to_string()),
                "identifier" if !locals.contains(&text(&left, source).to_string()) => {
                    Some(text(&left, source).to_string())
                }
                _ => None,
            };
            if let Some(name) = name.filter(|n| finals.contains(n)) {
                return Some(name);
            }
        }
    }
    None
}

fn final_field_names(type_decl: &Node<'_>, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for field in descendants(*type_decl, "field_declaration") {
        let is_final = descendants(field, "modifiers")
            .first()
            .is_some_and(|m| text(m, source).split_whitespace().any(|w| w == "final"));
        if !is_final {
            continue;
        }
        for declarator in descendants(field, "variable_declarator") {
            if let Some(name) = declarator.child_by_field_name("name") {
                out.push(text(&name, source).to_string());
            }
        }
    }
    out
}

/// `(name, declared type text)` for every local declared in this method **before** `offset`, plus
/// the method's own parameters — which are locals as far as the extraction is concerned.
fn locals_before(method: &Node<'_>, offset: usize, source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(params) = method.child_by_field_name("parameters") {
        for param in descendants(params, "formal_parameter") {
            if let (Some(t), Some(n)) =
                (param.child_by_field_name("type"), param.child_by_field_name("name"))
            {
                out.push((text(&n, source).to_string(), text(&t, source).to_string()));
            }
        }
    }
    // A varargs parameter is a `spread_parameter`, NOT a `formal_parameter`, and missing it does
    // not produce a missing argument — it produces a method whose body silently binds the name to a
    // FIELD of the same name, which is a different type and a different value. Inside the body a
    // varargs parameter simply is an array, so the extracted method takes `T[]` and the call passes
    // it straight through.
    for param in descendants(params_of(method), "spread_parameter") {
        let name = param
            .child_by_field_name("name")
            .or_else(|| descendants(param, "variable_declarator").first().and_then(|d| d.child_by_field_name("name")));
        // NOT `named_child(0)`: `final String... set` puts a `modifiers` node first, and taking it
        // as the type produced `final[] set` — a parameter list that does not parse.
        let mut cursor = param.walk();
        let ty = param
            .named_children(&mut cursor)
            .find(|c| !matches!(c.kind(), "modifiers" | "variable_declarator"));
        if let (Some(name), Some(ty)) = (name, ty) {
            out.push((text(&name, source).to_string(), format!("{}[]", text(&ty, source))));
        }
    }
    for declaration in descendants(*method, "local_variable_declaration") {
        if declaration.start_byte() >= offset {
            continue;
        }
        out.extend(declared_in(&declaration, source));
    }
    // The three other ways Java declares a local, none of which is a `local_variable_declaration`:
    // the variable of an enhanced `for`, a `catch` parameter, and a try-with-resources resource.
    // Each one missed is a name the extracted method reads and cannot see.
    for kind in ["enhanced_for_statement", "catch_formal_parameter", "resource"] {
        for node in descendants(*method, kind) {
            if node.start_byte() >= offset {
                continue;
            }
            let ty = node
                .child_by_field_name("type")
                .or_else(|| descendants(node, "catch_type").first().copied());
            if let (Some(name), Some(ty)) = (node.child_by_field_name("name"), ty) {
                out.push((text(&name, source).to_string(), text(&ty, source).to_string()));
            }
        }
    }
    out
}

/// The parameter list of a callable, or the callable itself when it has none — so a caller can walk
/// it without a second `if let`.
fn params_of<'t>(method: &Node<'t>) -> Node<'t> {
    method.child_by_field_name("parameters").unwrap_or(*method)
}

/// The same, for the statements of the selection.
fn locals_in(statements: &[Node<'_>], source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for statement in statements {
        for declaration in descendants(*statement, "local_variable_declaration") {
            out.extend(declared_in(&declaration, source));
        }
    }
    out
}

fn declared_in(declaration: &Node<'_>, source: &str) -> Vec<(String, String)> {
    let type_text = declaration
        .child_by_field_name("type")
        .map(|t| text(&t, source).to_string())
        .unwrap_or_default();
    descendants(*declaration, "variable_declarator")
        .iter()
        .filter_map(|d| d.child_by_field_name("name"))
        .map(|n| (text(&n, source).to_string(), type_text.clone()))
        .collect()
}

/// Every identifier the statements read, declaration names excluded.
fn names_read(statements: &[Node<'_>], source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for statement in statements {
        for ident in identifiers(*statement) {
            if is_declaration_name(&ident) {
                continue;
            }
            out.push(text(&ident, source).to_string());
        }
    }
    out
}

/// The same over a byte range of the method — what the code *after* the selection reads.
fn names_read_in_range(method: &Node<'_>, from: usize, to: usize, source: &str) -> Vec<String> {
    identifiers(*method)
        .into_iter()
        .filter(|n| n.start_byte() >= from && n.end_byte() <= to)
        .filter(|n| !is_declaration_name(n))
        .map(|n| text(&n, source).to_string())
        .collect()
}

/// Every name the statements assign to.
fn assigned_names(statements: &[Node<'_>], source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for statement in statements {
        for kind in ["assignment_expression", "update_expression"] {
            for node in descendants(*statement, kind) {
                if let Some(target) = node.child_by_field_name("left").or_else(|| node.named_child(0))
                {
                    out.push(text(&target, source).to_string());
                }
            }
        }
    }
    out
}

fn is_declaration_name(node: &Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        matches!(p.kind(), "variable_declarator" | "formal_parameter" | "catch_formal_parameter")
            && p.child_by_field_name("name").map(|n| n.id()) == Some(node.id())
    })
}

/// Move a block of source from one indentation to another, line by line.
///
/// Only the *continuation* lines are touched — the first one is written at the call site's position
/// by the caller. Getting this wrong is what makes an extracted method look like it was pasted.
fn reindent(block: &str, from: &str, to: &str, nl: &str) -> String {
    let mut lines = block.split('\n');
    let mut out = String::new();
    if let Some(first) = lines.next() {
        out.push_str(first.trim_end_matches('\r'));
    }
    for line in lines {
        let line = line.trim_end_matches('\r');
        out.push_str(nl);
        match line.strip_prefix(from) {
            Some(rest) => {
                out.push_str(to);
                out.push_str(rest);
            }
            None => out.push_str(line.trim_start()),
        }
    }
    out
}

/// A name for the extracted method, from what the statements do.
///
/// Deliberately modest: the caret lands on it and the editor offers a rename, so the job here is to
/// produce something typeable rather than something clever. A selection that plainly computes one
/// thing is named after it; everything else is `extracted`.
fn suggest_method_name(statements: &[Node<'_>], source: &str) -> String {
    if let [only] = statements {
        if let Some(call) = descendants(*only, "method_invocation").first() {
            if let Some(name) = call.child_by_field_name("name") {
                let base = text(&name, source);
                if !base.is_empty() {
                    return format!("do{}{}", base[..1].to_uppercase(), &base[1..]);
                }
            }
        }
    }
    "extracted".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn run(source: &str, from: &str, to: &str) -> Outcome {
        let tree = parse_java(source).unwrap();
        let start = source.find(from).unwrap();
        let end = source.find(to).unwrap() + to.len();
        extract_method(tree.root_node(), source, start, end)
    }

    const SRC: &str = "class A {\n\
    \x20   int report(int n) {\n\
    \x20       int base = n * 2;\n\
    \x20       int scaled = base + 10;\n\
    \x20       return scaled;\n\
    \x20   }\n\
}";

    /// The ordinary case, end to end: parameters in, one value out, a call in place.
    #[test]
    fn statements_become_a_method_with_the_locals_they_need() {
        let Some(Ok(plan)) = run(SRC, "int base", "base + 10;") else { panic!("no plan") };
        let applied = plan.apply(SRC);
        assert!(applied.contains("int scaled = extracted(n);"), "{applied}");
        assert!(applied.contains("private int extracted(int n) {"), "{applied}");
        assert!(applied.contains("return scaled;"), "{applied}");
        // The original statements are gone from the caller — counted rather than pattern-matched,
        // because the extracted body legitimately contains the very text the caller no longer does.
        assert_eq!(applied.matches("int base = n * 2;").count(), 1, "{applied}");
    }

    /// A static method extracts a static one — otherwise the result does not compile.
    #[test]
    fn a_static_method_extracts_a_static_one() {
        let source = SRC.replace("int report", "static int report");
        let Some(Ok(plan)) = run(&source, "int base", "base + 10;") else { panic!("no plan") };
        assert!(plan.apply(&source).contains("private static int extracted("), "{}", plan.apply(&source));
    }

    /// Nothing read afterwards means nothing to return.
    #[test]
    fn a_selection_nothing_reads_afterwards_returns_void() {
        let source = "class A {\n    void f(int n) {\n        int a = n + 1;\n        log(a);\n        done();\n    }\n    void log(int x) {}\n    void done() {}\n}";
        let Some(Ok(plan)) = run(source, "int a =", "log(a);") else { panic!("no plan") };
        let applied = plan.apply(source);
        assert!(applied.contains("private void extracted(int n) {"), "{applied}");
        assert!(applied.contains("        extracted(n);"), "{applied}");
    }

    /// The refusal that keeps this honest: Java cannot return two things.
    #[test]
    fn two_values_used_afterwards_is_refused_by_name() {
        let source = "class A {\n    int f(int n) {\n        int a = n + 1;\n        int b = n + 2;\n        return a + b;\n    }\n}";
        let Some(Err(refusal)) = run(source, "int a =", "int b = n + 2;") else { panic!("expected a refusal") };
        assert!(refusal.reason.contains("`a` and `b`"), "{}", refusal.reason);
        assert!(refusal.reason.contains("only return one"), "{}", refusal.reason);
    }

    /// A `return` inside the selection does not survive the move.
    #[test]
    fn an_escaping_return_is_refused() {
        let source = "class A {\n    int f(int n) {\n        if (n < 0) {\n            return 0;\n        }\n        return n;\n    }\n}";
        let Some(Err(refusal)) = run(source, "if (n < 0)", "}\n        return n;") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("`return`"), "{}", refusal.reason);
    }

    /// …but a `break` belonging to a loop inside the selection is not escaping, and refusing it
    /// would refuse every extraction containing a `for`.
    #[test]
    fn a_break_inside_a_loop_in_the_selection_is_not_escaping() {
        let source = "class A {\n    void f(int n) {\n        for (int i = 0; i < n; i++) {\n            if (i == 2) break;\n        }\n        done();\n    }\n    void done() {}\n}";
        let Some(Ok(_)) = run(source, "for (int i", "}\n        done") else {
            panic!("a self-contained loop is extractable")
        };
    }

    /// The one that produces silently wrong code in a naive implementation.
    #[test]
    fn assigning_a_caller_visible_local_is_refused() {
        let source = "class A {\n    int f(int n) {\n        int total = 0;\n        total = total + n;\n        log(total);\n        return total;\n    }\n    void log(int x) {}\n}";
        let Some(Err(refusal)) = run(source, "total = total + n;", "log(total);") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("passes by value"), "{}", refusal.reason);
    }

    /// A `var` local cannot be written into a signature, and guessing produces a method that does
    /// not compile.
    #[test]
    fn a_var_local_read_by_the_selection_is_refused() {
        let source = "class A {\n    void f() {\n        var list = make();\n        use(list);\n        use(list);\n    }\n    java.util.List<String> make() { return null; }\n    void use(Object o) {}\n}";
        let Some(Err(refusal)) = run(source, "use(list);", "use(list);") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("`var`"), "{}", refusal.reason);
    }

    /// Regression: a varargs parameter is a `spread_parameter`, which the parameter sweep did not
    /// look at. The extracted method then took no argument and its `params` bound to the FIELD of
    /// the same name — a different type, and code that compiles right up until it does not.
    #[test]
    fn a_varargs_parameter_is_passed_rather_than_left_to_bind_to_a_field() {
        let source = "class A {\n    java.util.List<Object> params = new java.util.ArrayList<>();\n    void add(Object... params) {\n        int n = params.length;\n        take(n);\n    }\n    void take(int x) {}\n}";
        let Some(Ok(plan)) = run(source, "int n =", "take(n);") else { panic!("no plan") };
        let applied = plan.apply(source);
        assert!(applied.contains("extracted(params);"), "{applied}");
        assert!(applied.contains("private void extracted(Object[] params) {"), "{applied}");
    }

    /// Regression: a constructor assigning a `final` field. The extraction compiles everywhere
    /// except where it matters — `final` is assignable only from a constructor.
    #[test]
    fn assigning_a_final_field_is_refused_because_only_a_constructor_may() {
        let source = "class A {\n    private final int n;\n    A(int n) {\n        this.n = n;\n        log();\n    }\n    void log() {}\n}";
        let Some(Err(refusal)) = run(source, "this.n = n;", "log();") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("final field"), "{}", refusal.reason);
    }

    /// …and a non-final field of the same shape is extracted as usual.
    #[test]
    fn assigning_an_ordinary_field_is_not_refused() {
        let source = "class A {\n    private int n;\n    A(int n) {\n        this.n = n;\n        log();\n    }\n    void log() {}\n}";
        assert!(matches!(run(source, "this.n = n;", "log();"), Some(Ok(_))));
    }

    /// A `switch` rule holds an EXPRESSION, and walking up from inside one used to land on the
    /// whole switch — extracted into a method whose call is then not a statement.
    #[test]
    fn a_switch_rule_arm_is_not_a_run_of_statements() {
        let source = "class A {\n    String f(int k) {\n        return switch (k) {\n            case 1 -> \"one\";\n            default -> \"other\";\n        };\n    }\n}";
        let start = source.find("\"one\"").unwrap();
        let tree = parse_java(source).unwrap();
        assert!(extract_method(tree.root_node(), source, start, start + 5).is_none());
    }

    /// Regression, twice over. `Map<K, V> hash;` declares a local and gives it nothing, so it
    /// cannot be returned — and it cannot be quietly left off the return list either, which is what
    /// the first fix did: moving the statements takes the declaration with them and every later use
    /// loses its symbol. The honest answer is that the selection is the wrong one.
    #[test]
    fn a_local_the_selection_only_declares_is_refused_not_dropped() {
        let source = "class A {\n    void f() {\n        String name;\n        int n = 1;\n        name = \"x\";\n        take(name, n);\n    }\n    void take(String s, int i) {}\n}";
        let Some(Err(refusal)) = run(source, "String name;", "int n = 1;") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("without giving it a value"), "{}", refusal.reason);
    }

    /// …and extending the selection to the assignment is exactly what unblocks it.
    #[test]
    fn extending_the_selection_to_the_assignment_works() {
        let source = "class A {\n    void f() {\n        String name;\n        name = \"x\";\n        take(name);\n    }\n    void take(String s) {}\n}";
        let Some(Ok(plan)) = run(source, "String name;", "name = \"x\";") else {
            panic!("expected a plan")
        };
        assert!(plan.apply(source).contains("return name;"), "{}", plan.apply(source));
    }

    /// Regression: statements inside a lambda's body. `row` is the lambda's parameter and
    /// `locals_before` walks the METHOD, so it never saw it — the extracted method read a `row`
    /// that does not exist there, or a field of that name.
    #[test]
    fn a_lambda_parameter_with_a_written_type_is_passed_on() {
        let source = "class A {\n    void f(java.util.List<String> rows) {\n        rows.forEach((String row) -> {\n            String t = row.trim();\n            take(t);\n        });\n    }\n    void take(String s) {}\n}";
        let Some(Ok(plan)) = run(source, "String t =", "take(t);") else { panic!("no plan") };
        let applied = plan.apply(source);
        assert!(applied.contains("extracted(row);"), "{applied}");
        assert!(applied.contains("private void extracted(String row) {"), "{applied}");
    }

    /// …and one written WITHOUT a type cannot be, for the same reason `var` cannot: the type was
    /// never written down, so there is nothing to put in a signature.
    #[test]
    fn a_lambda_parameter_with_no_written_type_is_refused() {
        let source = "class A {\n    void f(java.util.List<String> rows) {\n        rows.forEach(row -> {\n            String t = row.trim();\n            take(t);\n        });\n    }\n    void take(String s) {}\n}";
        let Some(Err(refusal)) = run(source, "String t =", "take(t);") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("lambda parameter"), "{}", refusal.reason);
    }

    /// A selection that merely CONTAINS a lambda is ordinary code and moves as it is.
    #[test]
    fn a_selection_containing_a_lambda_is_extracted_normally() {
        let source = "class A {\n    void f(java.util.List<String> rows, String suffix) {\n        int n = 1;\n        rows.forEach(row -> take(row + suffix + n));\n    }\n    void take(String s) {}\n}";
        let Some(Ok(plan)) = run(source, "int n = 1;", "suffix + n));") else { panic!("no plan") };
        assert!(
            plan.apply(source).contains("private void extracted(java.util.List<String> rows, String suffix) {"),
            "{}",
            plan.apply(source)
        );
    }

    /// Regression: a type parameter the METHOD declares does not exist in a sibling method, so the
    /// extracted one has to declare it. The class-level case below is what made this look fine.
    #[test]
    fn a_methods_own_type_parameter_moves_with_the_signature() {
        let source = "import java.util.List;\nclass A {\n    <T> void f(List<T> items) {\n        T first = items.get(0);\n        take(first);\n    }\n    <U> void take(U u) {}\n}";
        let Some(Ok(plan)) = run(source, "T first", "take(first);") else { panic!("no plan") };
        assert!(plan.apply(source).contains("private <T> void extracted(List<T> items) {"), "{}", plan.apply(source));
    }

    /// …with its bound, which says something the name alone does not.
    #[test]
    fn a_bounded_type_parameter_keeps_its_bound() {
        let source = "import java.util.List;\nclass A {\n    <T extends Comparable<T>> void f(List<T> items) {\n        T first = items.get(0);\n        take(first);\n    }\n    <U> void take(U u) {}\n}";
        let Some(Ok(plan)) = run(source, "T first", "take(first);") else { panic!("no plan") };
        assert!(
            plan.apply(source).contains("private <T extends Comparable<T>> void extracted(List<T> items) {"),
            "{}",
            plan.apply(source)
        );
    }

    /// A CLASS's type parameter is in scope for every member and must NOT be re-declared — doing so
    /// would shadow it and quietly make the method generic over a different `T`.
    #[test]
    fn a_class_type_parameter_is_not_redeclared() {
        let source = "import java.util.List;\nclass A<T> {\n    void f(List<T> items) {\n        T first = items.get(0);\n        take(first);\n    }\n    void take(T t) {}\n}";
        let Some(Ok(plan)) = run(source, "T first", "take(first);") else { panic!("no plan") };
        assert!(plan.apply(source).contains("private void extracted(List<T> items) {"), "{}", plan.apply(source));
    }

    /// A nested generic parameter type is copied exactly as it was declared.
    #[test]
    fn a_nested_generic_parameter_type_is_copied_verbatim() {
        let source = "import java.util.*;\nclass A {\n    void f(Map<String, List<String>> m, String k) {\n        List<String> rows = m.get(k);\n        take(rows);\n    }\n    void take(List<String> r) {}\n}";
        let Some(Ok(plan)) = run(source, "List<String> rows", "take(rows);") else { panic!("no plan") };
        assert!(
            plan.apply(source).contains("private void extracted(Map<String, List<String>> m, String k) {"),
            "{}",
            plan.apply(source)
        );
    }

    /// Regression, and one the first varargs fix introduced: `final String... set` puts a
    /// `modifiers` node first, so taking the type as "the first named child" produced `final[] set`
    /// — a parameter list that does not parse. The test that passed used `Object... x`, with none.
    #[test]
    fn a_final_varargs_parameter_keeps_its_real_type() {
        let source = "class A {\n    static boolean f(final String str, final String... set) {\n        int n = set.length;\n        take(n);\n        return true;\n    }\n    static void take(int x) {}\n}";
        let Some(Ok(plan)) = run(source, "int n = set.length;", "take(n);") else { panic!("no plan") };
        assert!(plan.apply(source).contains("(String[] set)"), "{}", plan.apply(source));
    }

    /// The three other ways Java declares a local, none of them a `local_variable_declaration`.
    #[test]
    fn an_enhanced_for_variable_is_a_local_like_any_other() {
        let source = "class A {\n    void f(String[] rows) {\n        for (final String v : rows) {\n            int n = v.length();\n            take(n);\n        }\n    }\n    void take(int x) {}\n}";
        let Some(Ok(plan)) = run(source, "int n = v.length();", "take(n);") else { panic!("no plan") };
        assert!(plan.apply(source).contains("extracted(v)"), "{}", plan.apply(source));
        assert!(plan.apply(source).contains("(String v)"), "{}", plan.apply(source));
    }

    #[test]
    fn a_catch_parameter_is_a_local_like_any_other() {
        let source = "class A {\n    void f() {\n        try {\n            g();\n        } catch (RuntimeException e) {\n            String m = e.getMessage();\n            take(m);\n        }\n    }\n    void g() {}\n    void take(String s) {}\n}";
        let Some(Ok(plan)) = run(source, "String m = e.getMessage();", "take(m);") else { panic!("no plan") };
        assert!(plan.apply(source).contains("(RuntimeException e)"), "{}", plan.apply(source));
    }

    /// A checked exception the moved body may raise has to stay declared, or it becomes an
    /// "unreported exception" the instant it moves.
    #[test]
    fn the_enclosing_throws_clause_moves_with_the_body() {
        let source = "import java.io.*;\nclass A {\n    void f(Reader r) throws IOException {\n        int c = r.read();\n        take(c);\n    }\n    void take(int x) {}\n}";
        let Some(Ok(plan)) = run(source, "int c = r.read();", "take(c);") else { panic!("no plan") };
        assert!(plan.apply(source).contains("extracted(Reader r) throws IOException {"), "{}", plan.apply(source));
    }

    /// Regression: a checked exception the body raises and a surrounding `try` catches never
    /// reaches the method's own `throws`, so copying only that left the `try` with nothing to catch.
    #[test]
    fn an_exception_the_surrounding_try_catches_is_declared_too() {
        let source = "import java.io.*;\nclass A {\n    void f(Reader r) {\n        try {\n            int c = r.read();\n            take(c);\n        } catch (IOException e) {\n        }\n    }\n    void take(int x) {}\n}";
        let Some(Ok(plan)) = run(source, "int c = r.read();", "take(c);") else { panic!("no plan") };
        assert!(plan.apply(source).contains("throws IOException {"), "{}", plan.apply(source));
    }

    /// Regression: a selection that ends by throwing. Replaced by `extracted();` the compiler no
    /// longer knows the path ends, and the caller loses its `return` — "missing return statement",
    /// reported on a method the user did not touch.
    #[test]
    fn a_selection_that_ends_by_throwing_is_refused() {
        let source = "class A {\n    int f(boolean a) {\n        if (a) {\n            return 1;\n        }\n        log();\n        throw new IllegalStateException();\n    }\n    void log() {}\n}";
        let Some(Err(refusal)) = run(source, "log();", "throw new IllegalStateException();") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("ends by throwing"), "{}", refusal.reason);
    }

    /// `this(…)` has to stay the first statement of its constructor, so it cannot become a call.
    #[test]
    fn a_constructor_delegation_is_refused() {
        let source = "class A {\n    A() {\n        this(1);\n        log();\n    }\n    A(int n) {}\n    void log() {}\n}";
        let Some(Err(refusal)) = run(source, "this(1);", "log();") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("first statement"), "{}", refusal.reason);
    }

    /// A labelled `break` leaves the selection whenever its label does — being inside a loop says
    /// nothing, because the label names a loop further out.
    #[test]
    fn a_labelled_break_targeting_an_outer_loop_is_escaping() {
        let source = "class A {\n    void f(int n) {\n        outer:\n        for (int i = 0; i < n; i++) {\n            for (int j = 0; j < n; j++) {\n                take(j);\n                break outer;\n            }\n        }\n    }\n    void take(int x) {}\n}";
        let Some(Err(refusal)) = run(source, "take(j);", "break outer;") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("`break`"), "{}", refusal.reason);
    }

    /// …and one whose label the selection brings with it is ordinary code.
    #[test]
    fn a_labelled_break_whose_label_is_inside_the_selection_is_not() {
        let source = "class A {\n    void f(int n) {\n        outer:\n        for (int i = 0; i < n; i++) {\n            take(i);\n            break outer;\n        }\n        done();\n    }\n    void take(int x) {}\n    void done() {}\n}";
        let Some(Ok(plan)) = run(source, "outer:", "break outer;\n        }") else {
            panic!("expected a plan")
        };
        assert!(plan.apply(source).contains("break outer;"), "{}", plan.apply(source));
    }

    /// Lombok's `val` is not Java, but it is everywhere in the code this editor exists for — and
    /// like `var` it leaves a declaration whose type text is not a type.
    #[test]
    fn a_lombok_val_local_is_refused_like_a_var_one() {
        let source = "class A {\n    void f() {\n        val rows = load();\n        int n = rows.size();\n        take(n);\n    }\n    java.util.List<String> load() { return null; }\n    void take(int x) {}\n}";
        let Some(Err(refusal)) = run(source, "int n = rows.size();", "take(n);") else {
            panic!("expected a refusal")
        };
        assert!(refusal.reason.contains("`val`"), "{}", refusal.reason);
    }

    /// The modifier order a reader expects, and the one the surrounding code is written in.
    #[test]
    fn a_static_extraction_reads_private_static_in_that_order() {
        let source = "class A {\n    static int f(int n) {\n        int base = n * 2;\n        int scaled = base + 10;\n        return scaled;\n    }\n}";
        let Some(Ok(plan)) = run(source, "int base", "base + 10;") else { panic!("no plan") };
        assert!(plan.apply(source).contains("private static int extracted(int n) {"), "{}", plan.apply(source));
    }

    #[test]
    fn the_moved_body_is_reindented_rather_than_pasted() {
        let Some(Ok(plan)) = run(SRC, "int base", "base + 10;") else { panic!("no plan") };
        let applied = plan.apply(SRC);
        // Both moved lines sit at the extracted method's body indentation.
        assert!(applied.contains("\n        int base = n * 2;\n        int scaled = base + 10;\n"), "{applied}");
    }
}
