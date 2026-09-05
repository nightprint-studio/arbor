//! The three refactorings that reshape a **local declaration** — split it from its assignment,
//! join it back, and swap between a written type and `var`.
//!
//! They are grouped because they all read the same node, `local_variable_declaration`, and because
//! they are inverses of each other in pairs: whichever direction the user wants, the other one is
//! what they will want next.
//!
//! ## `var` is the one that needs the resolver
//!
//! Explicit → `var` is text: the type is already written, so removing it is a deletion. The other
//! direction is not, and it goes through the same [`TypeSlot`] seam as extract variable — the plan
//! names the span whose type it needs and the caller with a resolver fills it. Required, not
//! optional: a declaration whose type could not be worked out has nothing to put where `var` was,
//! and writing `var` back would be a refactoring that did nothing while reporting success.
//!
//! ## What they will not do
//!
//! - **Split** a `final` local. `final int x; x = 1;` is legal Java in exactly one shape — a
//!   definite-assignment analysis away — and getting it wrong turns compiling code into an error
//!   about a variable that might already be assigned.
//! - **Split** a declaration of several names at once. `int a = 1, b = 2;` splits into a statement
//!   list, not a statement, and the interesting question — which of them did you mean — has no
//!   answer at a caret.
//! - **Join** across anything that reads or writes the variable in between.

use tree_sitter::Node;

use crate::plan::{Outcome, Plan, RefactorEdit, Refusal, TypeNeed, TypeSlot};
use crate::selection::{indent_at, is_block, newline, text};

const SPLIT: (&str, &str) = ("split-declaration", "Split declaration and assignment");
const JOIN: (&str, &str) = ("join-declaration", "Join declaration and assignment");
const TO_VAR: (&str, &str) = ("declaration-to-var", "Replace explicit type with `var`");
const FROM_VAR: (&str, &str) = ("var-to-declaration", "Replace `var` with the explicit type");

/// The placeholder a type slot is written with until a resolver fills it — the same one extract
/// variable uses, because the seam is the same.
const TYPE_PLACEHOLDER: &str = "var";

/// Plan a *split*: `int x = f();` becomes `int x;` and `x = f();`.
pub fn split_declaration(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = SPLIT;
    let decl = declaration_at(root, source, start, end)?;
    let ty = decl.child_by_field_name("type")?;
    let declarator = sole_declarator(&decl)?;
    let name = declarator.child_by_field_name("name")?;
    let value = declarator.child_by_field_name("value")?;

    if is_final(&decl, source) {
        return Some(Err(Refusal::new(
            id,
            label,
            "the local is `final`, and splitting it leaves an assignment Java allows only when it can prove the variable is still unset",
        )));
    }
    // An array declared the C way — `int x[] = …` — puts its brackets on the NAME, so a split that
    // copies the type text alone would produce `int x; x = new int[3];`, which does not compile.
    if declarator.child_by_field_name("dimensions").is_some() {
        return Some(Err(Refusal::new(
            id,
            label,
            "the array brackets are written on the name rather than the type — move them to the type first",
        )));
    }

    let indent = indent_at(source, decl.start_byte());
    let nl = newline(source);
    let split = format!(
        "{ty_text} {name_text};{nl}{indent}{name_text} = ",
        ty_text = text(&ty, source),
        name_text = text(&name, source),
    );
    // One edit from the start of the type to the start of the value: everything between them —
    // the name, the `=`, whatever spacing — is replaced by the two halves.
    let edits = vec![RefactorEdit::new(ty.start_byte(), value.start_byte(), split, "split")];
    Some(Ok(Plan::new(id, label, edits).named(text(&name, source)).caret_at(value.start_byte())))
}

/// Plan a *join*: `int x;` followed by `x = f();` becomes `int x = f();`.
pub fn join_declaration(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = JOIN;
    let decl = declaration_at(root, source, start, end)?;
    let declarator = sole_declarator(&decl)?;
    let name = declarator.child_by_field_name("name")?;
    // Only a declaration with NO value can be joined to anything.
    if declarator.child_by_field_name("value").is_some() {
        return None;
    }
    let target = text(&name, source);

    let next = next_statement(&decl)?;
    let Some((assigned, value)) = plain_assignment(&next, source) else {
        return Some(Err(Refusal::new(
            id,
            label,
            "the next statement is not a plain assignment to this variable",
        )));
    };
    if assigned != target {
        return Some(Err(Refusal::new(
            id,
            label,
            format!("the next statement assigns `{assigned}`, not `{target}`"),
        )));
    }

    let edits = vec![
        // The assignment statement goes away entirely, trailing newline and indent included.
        RefactorEdit::new(line_start(source, next.start_byte()), next.end_byte(), String::new(), "assignment"),
        // …and its value lands on the declaration.
        RefactorEdit::new(
            name.end_byte(),
            decl.end_byte(),
            format!(" = {};", text(&value, source)),
            "value",
        ),
    ];
    Some(Ok(Plan::new(id, label, edits).named(target).caret_at(name.end_byte())))
}

/// Plan an *explicit type → `var`*.
pub fn to_var(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = TO_VAR;
    let decl = declaration_at(root, source, start, end)?;
    let ty = decl.child_by_field_name("type")?;
    let declarator = sole_declarator(&decl)?;

    // Already `var`, or a Lombok `val` — nothing to do, and a greyed row would be noise.
    if bennu_java::prelude::is_inferred_type(text(&ty, source)) {
        return None;
    }
    // `var` infers from the initialiser, so there must be one — and it must not be one of the
    // three things Java refuses to infer from.
    let Some(value) = declarator.child_by_field_name("value") else {
        return Some(Err(Refusal::new(
            id,
            label,
            "`var` takes its type from an initialiser, and this declaration has none",
        )));
    };
    if let Some(reason) = uninferable(&value) {
        return Some(Err(Refusal::new(id, label, reason)));
    }
    if let Some(reason) = type_is_doing_work(&ty, &value, source) {
        return Some(Err(Refusal::new(id, label, reason)));
    }
    if declarator.child_by_field_name("dimensions").is_some() {
        return Some(Err(Refusal::new(
            id,
            label,
            "the array brackets are written on the name, and `var x[]` is not Java",
        )));
    }

    let edits = vec![RefactorEdit::new(
        ty.start_byte(),
        ty.end_byte(),
        "var".to_string(),
        "type",
    )];
    Some(Ok(Plan::new(id, label, edits).caret_at(ty.start_byte())))
}

/// Plan a *`var` → explicit type*. The type comes from the caller's resolver.
pub fn from_var(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = FROM_VAR;
    let decl = declaration_at(root, source, start, end)?;
    let ty = decl.child_by_field_name("type")?;
    let declarator = sole_declarator(&decl)?;

    if !bennu_java::prelude::is_inferred_type(text(&ty, source)) {
        return None;
    }
    let value = declarator.child_by_field_name("value")?;

    let edits = vec![RefactorEdit::new(
        ty.start_byte(),
        ty.end_byte(),
        TYPE_PLACEHOLDER.to_string(),
        "type",
    )];
    let plan = Plan::new(id, label, edits).caret_at(ty.start_byte());
    Some(Ok(plan.needing_type(TypeSlot {
        // The type wanted is the INITIALISER's, which is what `var` stands for.
        start: value.start_byte(),
        end: value.end_byte(),
        edit_index: 0,
        at: 0,
        placeholder: TYPE_PLACEHOLDER.to_string(),
        // Writing `var` back where `var` already is would report success and change nothing.
        need: TypeNeed::Required,
    })))
}

// ── the pieces ───────────────────────────────────────────────────────────────

/// The local declaration the caret is on — through its type or its name, not from inside its
/// initialiser, where the user is looking at an expression and not at the declaration.
fn declaration_at<'t>(root: Node<'t>, source: &str, start: usize, end: usize) -> Option<Node<'t>> {
    let at = crate::selection::node_covering(root, start, end)?;
    let decl = crate::selection::enclosing(at, &["local_variable_declaration"])?;
    // Only inside a block: a `for (int i = 0; …)` header declares a local too, and none of these
    // three refactorings mean anything there.
    if !decl.parent().is_some_and(|p| is_block(&p)) {
        return None;
    }
    let head_end = sole_declarator(&decl)
        .and_then(|d| d.child_by_field_name("name"))
        .map(|n| n.end_byte())
        .unwrap_or_else(|| decl.end_byte());
    let _ = source;
    (start >= decl.start_byte() && start <= head_end).then_some(decl)
}

/// The one declarator this statement declares — `None` when it declares several, where a caret
/// cannot say which one was meant.
fn sole_declarator<'t>(decl: &Node<'t>) -> Option<Node<'t>> {
    let mut cursor = decl.walk();
    let declarators: Vec<Node<'t>> = decl
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "variable_declarator")
        .collect();
    match declarators.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

fn is_final(decl: &Node<'_>, source: &str) -> bool {
    let mut cursor = decl.walk();
    let is_final = decl
        .named_children(&mut cursor)
        .any(|c| c.kind() == "modifiers" && text(&c, source).split_whitespace().any(|w| w == "final"));
    is_final
}

/// The statement immediately after `decl` in the same block, skipping comments.
fn next_statement<'t>(decl: &Node<'t>) -> Option<Node<'t>> {
    let mut sibling = decl.next_named_sibling();
    while let Some(node) = sibling {
        if !matches!(node.kind(), "line_comment" | "block_comment") {
            return Some(node);
        }
        sibling = node.next_named_sibling();
    }
    None
}

/// `x = value;` — the assigned NAME and the value, when the statement is exactly that.
///
/// Deliberately narrow: `x += 1` reads `x` before writing it, so joining it onto the declaration
/// would read a variable that does not exist yet. Only `=` qualifies.
fn plain_assignment<'t>(stmt: &Node<'t>, source: &str) -> Option<(String, Node<'t>)> {
    if stmt.kind() != "expression_statement" {
        return None;
    }
    let expr = stmt.named_child(0)?;
    if expr.kind() != "assignment_expression" {
        return None;
    }
    let operator = expr.child_by_field_name("operator")?;
    if text(&operator, source) != "=" {
        return None;
    }
    let left = expr.child_by_field_name("left")?;
    if left.kind() != "identifier" {
        return None;
    }
    Some((text(&left, source).to_string(), expr.child_by_field_name("right")?))
}

/// Why the written type is **carrying** the declaration rather than merely describing it.
///
/// `var` is only a safe swap while the declared type is exactly what the initialiser infers to. It
/// often is not, and the two shapes below are the ones where the difference is visible without a
/// resolver — both measured on `commons-lang3`, where they were 12 of the 16 files this refactoring
/// broke before they were refused.
///
/// The third shape is an upcast — `Format f = someObject` — where the written type is simply wider
/// than the initialiser. Nothing in the text says so, so it needs the resolver to compare the two,
/// and until it does this refactoring is not offered where it would be wrong the other 4 times.
fn type_is_doing_work(ty: &Node<'_>, value: &Node<'_>, source: &str) -> Option<&'static str> {
    let written = text(ty, source);

    // The diamond takes its type arguments FROM the declared type: `List<String> xs = new
    // ArrayList<>()` is an `ArrayList<String>` only because the left-hand side says so. Remove the
    // type and it becomes `ArrayList<Object>`, which every later use rejects.
    if value.kind() == "object_creation_expression" {
        let created = value
            .child_by_field_name("type")
            .map(|t| text(&t, source))
            .unwrap_or_default();
        if created.replace(char::is_whitespace, "").ends_with("<>") {
            return Some(
                "the `<>` takes its type arguments from the written type — spell them out first, or the \
                 declaration becomes a collection of `Object`",
            );
        }
    }

    // A declaration widens its initialiser: `long total = 0` holds an `int` literal in a `long`.
    // `var` infers the literal's own type, so the next `total += aLong` stops compiling. Only `int`
    // and `boolean` are safe, because those ARE what their literals infer to.
    if matches!(written, "long" | "float" | "double" | "char" | "byte" | "short") {
        return Some(
            "the written type widens the initialiser — `var` would infer the narrower one and the \
             first arithmetic after it would not compile",
        );
    }
    None
}

/// Why `var` cannot stand for this initialiser — `None` when it can.
///
/// Java's own list, and it is short: a lambda, a method reference and an array initialiser have no
/// type of their own to infer, and `null` has one that means nothing. Each is a compile error under
/// `var`, so each is a refusal here rather than a plan that breaks the file.
fn uninferable(value: &Node<'_>) -> Option<&'static str> {
    match value.kind() {
        "lambda_expression" => {
            Some("a lambda takes its type from what it is assigned to, so `var` has nothing to read")
        }
        "method_reference" => {
            Some("a method reference takes its type from what it is assigned to, so `var` has nothing to read")
        }
        "array_initializer" => {
            Some("a bare `{…}` initialiser has no type of its own — write `new int[]{…}` first")
        }
        "null_literal" => Some("`var x = null` is not Java: null has no type to infer"),
        _ => None,
    }
}

/// The offset of the start of the line `offset` is on, so a removed statement takes its indent
/// with it instead of leaving a ragged blank.
fn line_start(source: &str, offset: usize) -> usize {
    source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn plan_at(source: &str, needle: &str, f: fn(Node<'_>, &str, usize, usize) -> Outcome) -> Outcome {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap();
        f(tree.root_node(), source, at, at)
    }

    fn applied(source: &str, needle: &str, f: fn(Node<'_>, &str, usize, usize) -> Outcome) -> String {
        match plan_at(source, needle, f) {
            Some(Ok(plan)) => plan.apply(source),
            other => panic!("expected a plan, got {other:?}"),
        }
    }

    fn refusal(source: &str, needle: &str, f: fn(Node<'_>, &str, usize, usize) -> Outcome) -> String {
        match plan_at(source, needle, f) {
            Some(Err(r)) => r.reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    // ── split / join ──────────────────────────────────────────────────────────

    #[test]
    fn splitting_leaves_the_declaration_and_the_assignment() {
        let src = "class A {\n    void f() {\n        int x = compute();\n    }\n}";
        let out = applied(src, "int x", split_declaration);
        assert!(out.contains("int x;\n        x = compute();"), "{out}");
    }

    /// The inverse, and it must land back on the source it started from.
    #[test]
    fn joining_is_the_inverse_of_splitting() {
        let src = "class A {\n    void f() {\n        int x;\n        x = compute();\n    }\n}";
        let out = applied(src, "int x", join_declaration);
        assert!(out.contains("int x = compute();"), "{out}");
        assert!(!out.contains("x = compute();\n        x"), "the assignment is gone: {out}");
    }

    /// `final int x; x = 1;` compiles only where Java can prove the variable is still unset —
    /// which is an analysis this crate does not have.
    #[test]
    fn a_final_local_is_refused() {
        let src = "class A {\n    void f() {\n        final int x = 1;\n    }\n}";
        assert!(refusal(src, "final int x", split_declaration).contains("`final`"));
    }

    /// `int a = 1, b = 2;` has no single answer to "which one did you mean".
    #[test]
    fn several_names_at_once_offer_nothing() {
        let src = "class A {\n    void f() {\n        int a = 1, b = 2;\n    }\n}";
        assert!(plan_at(src, "int a", split_declaration).is_none());
    }

    /// `x += 1` reads `x` before it writes it, so it cannot move onto the declaration.
    #[test]
    fn a_compound_assignment_is_refused_by_join() {
        let src = "class A {\n    void f() {\n        int x;\n        x += 1;\n    }\n}";
        assert!(refusal(src, "int x", join_declaration).contains("plain assignment"));
    }

    #[test]
    fn joining_the_wrong_variable_says_which() {
        let src = "class A {\n    void f(int y) {\n        int x;\n        y = 3;\n    }\n}";
        assert!(refusal(src, "int x", join_declaration).contains("assigns `y`"));
    }

    // ── var ───────────────────────────────────────────────────────────────────

    #[test]
    fn an_explicit_type_becomes_var() {
        let src = "class A {\n    void f() {\n        String s = read();\n    }\n}";
        let out = applied(src, "String s", to_var);
        assert!(out.contains("var s = read();"), "{out}");
    }

    /// Java's own list of what `var` cannot read — each of these is a compile error, so each is a
    /// refusal rather than a plan that breaks the file.
    #[test]
    fn the_four_things_var_cannot_infer_from_are_refused() {
        for (init, expected) in [
            ("Runnable r = () -> {};", "lambda"),
            ("Runnable r = A::go;", "method reference"),
            ("int[] r = {1, 2};", "no type of its own"),
            ("String r = null;", "null has no type"),
        ] {
            let src = format!("class A {{\n    void f() {{\n        {init}\n    }}\n}}");
            let reason = refusal(&src, " r =", to_var);
            assert!(reason.contains(expected), "for `{init}` got: {reason}");
        }
    }

    /// Measured on `commons-lang3`: 9 of the 16 files this refactoring broke were this one shape.
    #[test]
    fn a_diamond_takes_its_arguments_from_the_written_type() {
        let src = "class A {\n    void f() {\n        java.util.List<String> xs = new java.util.ArrayList<>();\n    }\n}";
        assert!(refusal(src, "java.util.List<String> xs", to_var).contains("`<>`"));
    }

    /// And 3 more were this one: `var total = 0` is an `int`, whatever the declaration said.
    #[test]
    fn a_widening_primitive_declaration_is_refused() {
        for ty in ["long", "double", "float", "char", "byte", "short"] {
            let src = format!("class A {{\n    void f() {{\n        {ty} n = 0;\n    }}\n}}");
            assert!(refusal(&src, &format!("{ty} n"), to_var).contains("widens"), "{ty}");
        }
    }

    /// …but the two that ARE what their literals infer to stay on offer.
    #[test]
    fn int_and_boolean_are_still_offered() {
        for (ty, init) in [("int", "0"), ("boolean", "true")] {
            let src = format!("class A {{\n    void f() {{\n        {ty} n = {init};\n    }}\n}}");
            let out = match plan_at(&src, &format!("{ty} n"), to_var) {
                Some(Ok(plan)) => plan.apply(&src),
                other => panic!("expected a plan for {ty}, got {other:?}"),
            };
            assert!(out.contains("var n ="), "{out}");
        }
    }

    #[test]
    fn a_declaration_already_var_offers_nothing() {
        let src = "class A {\n    void f() {\n        var s = read();\n    }\n}";
        assert!(plan_at(src, "var s", to_var).is_none());
    }

    /// Going back needs the resolver, and it may not settle for the placeholder — writing `var`
    /// where `var` already is would report success and change nothing.
    #[test]
    fn going_back_from_var_demands_a_real_type() {
        let src = "class A {\n    void f() {\n        var s = read();\n    }\n}";
        let Some(Ok(plan)) = plan_at(src, "var s", from_var) else { panic!("expected a plan") };
        assert_eq!(plan.type_slot.as_ref().map(|s| s.need), Some(TypeNeed::Required));
        // The span whose type is wanted is the initialiser's, which is what `var` stands for.
        let slot = plan.type_slot.unwrap();
        assert_eq!(&src[slot.start..slot.end], "read()");
    }

    /// A Lombok `val` is a `final var`, and the engine has one answer for both.
    #[test]
    fn lombok_val_counts_as_inferred() {
        let src = "class A {\n    void f() {\n        val s = read();\n    }\n}";
        assert!(plan_at(src, "val s", to_var).is_none());
    }

    /// Standing in the initialiser is standing in an expression, not on the declaration.
    #[test]
    fn a_caret_in_the_initialiser_offers_nothing() {
        let src = "class A {\n    void f() {\n        String s = readTheThing();\n    }\n}";
        assert!(plan_at(src, "readTheThing", to_var).is_none());
    }
}
