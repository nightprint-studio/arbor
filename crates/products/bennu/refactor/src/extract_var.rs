//! **Extract variable** and **extract constant** — give an expression a name.
//!
//! The smallest refactoring there is and the one used most, because it is how an unreadable line
//! becomes a readable one without changing what it does. The two share everything but where the
//! declaration lands: a local goes directly above the statement the expression is in, a constant
//! goes at the top of the class as `private static final`.
//!
//! ## What it will not do
//!
//! - An expression with a **side effect** repeated: only the occurrence under the caret is
//!   replaced, never "all occurrences", because `next()` twice and `next()` once are different
//!   programs and no analysis here can tell which the user meant. (A second occurrence of a pure
//!   expression is a reasonable future offer; it is not free, and doing it wrong is silent.)
//! - An expression that **is already** a whole declaration's initialiser — `int x = a + b;` with
//!   the caret on `a + b` — extracting it would produce `int t = a + b; int x = t;`, which is not
//!   what anybody meant by "give this a name". It already has one.
//! - An expression referring to a **local declared in the same statement**, which cannot be lifted
//!   above it.
//!
//! ## The type
//!
//! Nothing here can name the type of `repo.findAll()` — that is a question for the resolver. The
//! plan carries a [`TypeSlot`] and the caller fills it; see [`crate::plan`] for why `var` is not
//! the answer.

use tree_sitter::Node;

use crate::plan::{Outcome, Plan, RefactorEdit, Refusal, TypeSlot};
use crate::selection::{
    enclosing, enclosing_type, expression_for, identifiers, indent_at, is_statement, newline, text,
    TYPE_DECLS,
};

const EXTRACT_VAR: (&str, &str) = ("extract-variable", "Extract variable");
const EXTRACT_CONST: (&str, &str) = ("extract-constant", "Extract constant");

/// The placeholder a type slot is written with until a resolver fills it.
pub const TYPE_PLACEHOLDER: &str = "var";

/// Plan an *extract variable* at a caret or over a selection.
pub fn extract_variable(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = EXTRACT_VAR;
    let expr = expression_for(root, source, start, end)?;
    if let Some(reason) = unfit(&expr, source) {
        return Some(Err(Refusal::new(id, label, reason)));
    }
    // A `case` label and an annotation's value must be compile-time constants, so a local cannot
    // stand in for one. This lives here and not in `unfit` because extract CONSTANT is the offer
    // that belongs in exactly this place, and `unfit` is shared with it.
    if enclosing(expr, &["switch_label", "annotation", "marker_annotation", "element_value_pair"])
        .is_some()
    {
        return Some(Err(Refusal::new(
            id,
            label,
            "a case label or an annotation value must be a constant — extract a constant instead",
        )));
    }
    // `Processor.Arch` is a nested TYPE, not a value, and naming it produces `var Arch =
    // Processor.Arch;`. Java's convention is the only evidence available without a resolver, and it
    // is reliable: a capitalised member is a type, unless it is SCREAMING_CASE, which is a constant
    // and a perfectly good thing to name. Silence rather than a refusal — nobody points at a type
    // name meaning to extract it.
    if names_a_type(&expr, source) {
        return None;
    }
    // A lambda body is another scope AND another moment. Lifting an expression out of one puts the
    // declaration where the lambda's parameters do not exist, and — even where it would compile —
    // turns something evaluated on every call into something evaluated once. The same argument as
    // the loop header below, and the same answer.
    if let Some(statement) = enclosing(expr, &["expression_statement", "local_variable_declaration",
        "return_statement", "if_statement", "while_statement", "for_statement",
        "enhanced_for_statement", "do_statement", "throw_statement", "switch_expression",
        "assert_statement", "yield_statement"])
    {
        if crosses_a_body(&expr, &statement) {
            return Some(Err(Refusal::new(
                id,
                label,
                "the expression is inside a lambda or an anonymous class, and naming it above the statement would take it out of that scope",
            )));
        }
    }
    // A loop's header is re-evaluated every time round. Naming it hoists it above the loop, where
    // it is computed once — a different program — and where the loop's own `i` does not exist yet.
    if in_loop_header(&expr) {
        return Some(Err(Refusal::new(
            id,
            label,
            "this is part of a loop's header and is re-evaluated every iteration — naming it would compute it once",
        )));
    }
    let statement = enclosing(expr, &["expression_statement", "local_variable_declaration",
        "return_statement", "if_statement", "while_statement", "for_statement",
        "enhanced_for_statement", "do_statement", "throw_statement", "switch_expression",
        "assert_statement", "yield_statement"])?;
    // Only a statement sitting directly in a block can have a line inserted above it. `if (x) doIt();`
    // has no block to insert into, and manufacturing one is a different refactoring.
    if !statement.parent().is_some_and(|p| crate::selection::is_block(&p)) {
        return Some(Err(Refusal::new(
            id,
            label,
            "the statement is not inside a block — add braces first, then extract",
        )));
    }
    if declares_a_local_used_here(&expr, &statement, source) {
        return Some(Err(Refusal::new(
            id,
            label,
            "the expression uses a variable this same statement declares, so it cannot be lifted above it",
        )));
    }

    let name = unique_name(&suggest_name(&expr, source), scope_of(&expr), source);
    let indent = indent_at(source, statement.start_byte());
    let nl = newline(source);

    // `list.add(x);` — the expression IS the whole statement, so there is no use left to replace
    // once it has been named. Emitting one anyway leaves a bare `add;` behind, which is not a
    // statement. Naming it in place is what the user meant and what every IDE does here.
    if statement.kind() == "expression_statement"
        && expr.parent().map(|p| p.id()) == Some(statement.id())
    {
        let at = expr.start_byte();
        let prefix = format!("{TYPE_PLACEHOLDER} {name} = ");
        let plan = Plan::new(
            id,
            label,
            vec![RefactorEdit::new(at, at, prefix, "declaration")],
        )
        .named(name)
        .caret_at(at + TYPE_PLACEHOLDER.len() + 1);
        return Some(Ok(plan.needing_type(TypeSlot {
            start: expr.start_byte(),
            end: expr.end_byte(),
            edit_index: 0,
            at: 0,
            placeholder: TYPE_PLACEHOLDER.to_string(),
            // `obj.setName(x);` may be `void`, and `var` cannot stand in for that.
            required: true,
        })));
    }

    let declaration =
        format!("{TYPE_PLACEHOLDER} {name} = {};{nl}{indent}", text(&expr, source));
    let insert_at = statement.start_byte();

    let plan = Plan::new(
        id,
        label,
        vec![
            RefactorEdit::new(insert_at, insert_at, declaration, "declaration"),
            RefactorEdit::new(expr.start_byte(), expr.end_byte(), name.clone(), "use"),
        ],
    )
    .named(name)
    // The declaration is the LAST edit in application order (highest start wins, and the use sits
    // after it), so its index in the sorted list is the one to point the slot at.
    .caret_at(insert_at + TYPE_PLACEHOLDER.len() + 1);
    let slot_index = plan.edits.iter().position(|e| e.reason == "declaration")?;
    Some(Ok(plan.needing_type(TypeSlot {
        start: expr.start_byte(),
        end: expr.end_byte(),
        edit_index: slot_index,
        at: 0,
        placeholder: TYPE_PLACEHOLDER.to_string(),
        required: false,
    })))
}

/// Plan an *extract constant*: the same expression, lifted to a `private static final` field.
///
/// Offered only for an expression that is **constant to read** — literals and operations over them,
/// plus references to other constants. A call would be evaluated once at class-initialisation time
/// instead of at every use, which is a behavioural change disguised as a tidy-up.
pub fn extract_constant(root: Node<'_>, source: &str, start: usize, end: usize) -> Outcome {
    let (id, label) = EXTRACT_CONST;
    let expr = expression_for(root, source, start, end)?;
    // Constant-ness is asked FIRST: on anything that is not one, "extract constant" is not what the
    // user is reaching for, and a greyed row explaining that a diamond has no type of its own is a
    // sentence about a refactoring nobody asked for.
    if !is_constant_expression(&expr) {
        // Silence rather than a refusal: on a call, "extract constant" is not what the user is
        // reaching for, and a greyed row about it in every Alt+Enter menu is noise.
        return None;
    }
    if let Some(reason) = unfit(&expr, source) {
        return Some(Err(Refusal::new(id, label, reason)));
    }
    // An enum's constants are initialised BEFORE its static fields, so a constant lifted out of a
    // constant's arguments could never be read from there — whichever way round they are written.
    // There is no placement that works, which makes this a refusal and not a layout problem.
    if enclosing(expr, &["enum_constant"]).is_some() {
        return Some(Err(Refusal::new(
            id,
            label,
            "an enum constant is built before the class's static fields, so it cannot read one",
        )));
    }
    let type_decl = enclosing_type(expr)?;
    let body = type_decl.child_by_field_name("body")?;
    // A field may only read a field declared before it. Extracting out of one field's initialiser
    // and appending the constant after the last field puts the declaration BELOW its own use —
    // "illegal forward reference", from a refactoring that looks right on the screen.
    // A field may only read a field declared before it, and the same is true of a static
    // initialiser and of an enum constant's arguments. So the constant goes above whichever MEMBER
    // of the class body the expression lives in — not merely above a `field_declaration`, which was
    // the first version of this and left "illegal forward reference" everywhere else.
    let insert_at = match member_containing(&body, &expr) {
        Some(member) => line_start(source, member.start_byte()),
        None => insertion_point_in_body(&body, source)?,
    };

    let name = unique_name(&screaming(&suggest_name(&expr, source)), type_decl, source);
    let indent = member_indent(source, &body);
    let nl = newline(source);
    // A field of an interface or an annotation type is ALREADY public static final, and saying
    // `private` there is not redundant — it does not compile.
    let modifiers = match type_decl.kind() {
        "interface_declaration" | "annotation_type_declaration" => "",
        _ => "private static final ",
    };
    let declaration = format!(
        "{indent}{modifiers}{TYPE_PLACEHOLDER} {name} = {};{nl}",
        text(&expr, source)
    );

    let plan = Plan::new(
        id,
        label,
        vec![
            RefactorEdit::new(insert_at, insert_at, declaration, "declaration"),
            RefactorEdit::new(expr.start_byte(), expr.end_byte(), name.clone(), "use"),
        ],
    )
    .named(name);
    let slot_index = plan.edits.iter().position(|e| e.reason == "declaration")?;
    let at = indent.len() + modifiers.len();
    Some(Ok(plan.needing_type(TypeSlot {
        start: expr.start_byte(),
        end: expr.end_byte(),
        edit_index: slot_index,
        at,
        placeholder: TYPE_PLACEHOLDER.to_string(),
        // A FIELD is never `var`: the placeholder does not compile here in any Java
        // version, so a caller that cannot name the type must not apply this plan at all.
        required: true,
    })))
}

/// Why this expression is not worth naming — `None` when it is.
fn unfit(expr: &Node<'_>, source: &str) -> Option<&'static str> {
    // A bare name already has a name. So does the whole initialiser of a declaration.
    if matches!(expr.kind(), "identifier" | "this") {
        return Some("this is already a name");
    }
    if expr
        .parent()
        .is_some_and(|p| p.kind() == "variable_declarator" && p.child_by_field_name("value").map(|v| v.id()) == Some(expr.id()))
    {
        return Some("this is already the value of a named variable");
    }
    if text(expr, source).trim().is_empty() {
        return Some("nothing is selected");
    }
    // The LEFT of an assignment is a place, not a value. Naming it turns `this.field = x` into
    // `var field = this.field; field = x;`, which compiles perfectly and writes to the new local
    // instead of the field — the assignment silently stops happening.
    if is_assignment_target(expr) {
        return Some("this is where a value is written, not a value — there is nothing to name");
    }
    // An expression whose type comes from what it is ASSIGNED TO has no type of its own to
    // declare. `var f = () -> x;` and `var e = null;` do not compile at all, and `var l = new
    // ArrayList<>();` compiles as the wrong thing. No resolver fixes these: the answer is not in
    // the expression.
    if matches!(expr.kind(), "lambda_expression" | "method_reference" | "null_literal") {
        return Some("this has no type of its own — it takes one from what it is assigned to");
    }
    if expr.kind() == "object_creation_expression"
        && expr
            .child_by_field_name("type")
            .and_then(|t| crate::selection::descendants(t, "type_arguments").first().copied())
            .is_some_and(|a| a.named_child_count() == 0)
    {
        return Some("the diamond takes its type from what it is assigned to — write the type arguments first");
    }
    None
}

/// Whether this expression reads as a type name rather than a value.
fn names_a_type(expr: &Node<'_>, source: &str) -> bool {
    if expr.kind() != "field_access" {
        return false;
    }
    let Some(field) = expr.child_by_field_name("field") else { return false };
    let name = text(&field, source);
    let capitalised = name.chars().next().is_some_and(char::is_uppercase);
    let screaming = name.chars().all(|c| c.is_uppercase() || c.is_numeric() || c == '_');
    capitalised && !screaming
}

/// Whether a body of its own — a lambda, an anonymous class — sits between the expression and the
/// statement a declaration for it would go above.
fn crosses_a_body(expr: &Node<'_>, statement: &Node<'_>) -> bool {
    let mut node = *expr;
    while let Some(parent) = node.parent() {
        if parent.id() == statement.id() {
            return false;
        }
        if matches!(parent.kind(), "lambda_expression" | "class_body") {
            return true;
        }
        node = parent;
    }
    false
}

/// Whether the expression sits in the part of a loop that runs on every iteration.
///
/// The `init` is not: it runs once, and lifting it above the loop is exactly what it already does.
fn in_loop_header(expr: &Node<'_>) -> bool {
    let mut node = *expr;
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "for_statement" | "while_statement" | "do_statement" | "enhanced_for_statement" => {
                let is = |field: &str| {
                    parent.child_by_field_name(field).map(|c| c.id()) == Some(node.id())
                };
                return !is("body") && !is("init") && !is("value");
            }
            // A block ends the header: past it, this is the loop's body.
            "block" => return false,
            _ => node = parent,
        }
    }
    false
}

/// Whether this expression is the place an assignment writes to, rather than a value it reads.
///
/// The chain matters: in `a.b.c = x` the target is the whole `a.b.c`, so `a.b` reached through the
/// `object` side is part of it — while the `i` of `a[i] = x` is an ordinary value and extracting it
/// is fine.
fn is_assignment_target(expr: &Node<'_>) -> bool {
    let mut node = *expr;
    loop {
        let Some(parent) = node.parent() else { return false };
        let child_is = |field: &str| {
            parent.child_by_field_name(field).map(|c| c.id()) == Some(node.id())
        };
        match parent.kind() {
            "assignment_expression" => return child_is("left"),
            "update_expression" => return true,
            "field_access" if child_is("object") => node = parent,
            "array_access" if child_is("array") => node = parent,
            _ => return false,
        }
    }
}

/// Whether the expression only reads things that are fixed at compile time — literals, operations
/// over them, and `SCREAMING_CASE` names, which by convention are the constants a project has.
fn is_constant_expression(expr: &Node<'_>) -> bool {
    match expr.kind() {
        "string_literal" | "character_literal" | "decimal_integer_literal" | "hex_integer_literal"
        | "octal_integer_literal" | "binary_integer_literal" | "decimal_floating_point_literal"
        | "hex_floating_point_literal" | "true" | "false" | "null_literal" => true,
        "unary_expression" | "parenthesized_expression" | "binary_expression" => {
            let mut cursor = expr.walk();
            let all = expr.named_children(&mut cursor).all(|c| is_constant_expression(&c));
            all
        }
        _ => false,
    }
}

/// Whether the expression reads a variable that this very statement declares — `int n = size() - n;`
/// is not a thing, but `int n = size(); …` with the caret on `size()` inside the same declaration is,
/// and lifting it above would be fine. This is the case where it is not.
fn declares_a_local_used_here(expr: &Node<'_>, statement: &Node<'_>, source: &str) -> bool {
    if statement.kind() != "local_variable_declaration" {
        return false;
    }
    let declared: Vec<&str> = crate::selection::descendants(*statement, "variable_declarator")
        .iter()
        .filter_map(|d| d.child_by_field_name("name"))
        .map(|n| text(&n, source))
        .collect();
    identifiers(*expr).iter().any(|i| declared.contains(&text(i, source)))
}

/// Where a new field goes in a class body: after the last field, else right after the `{`.
///
/// Not "at the top" unconditionally — a class that already groups its constants gets the new one in
/// the group, which is the difference between a refactoring that fits the file and one that has to
/// be tidied up after.
fn insertion_point_in_body(body: &Node<'_>, source: &str) -> Option<usize> {
    // An enum's constants must come before anything else, so "right after the `{`" — fine for a
    // class — puts the field where the grammar expects `INTEGER,`. The member section is what a
    // field belongs in, and an enum that has none yet needs its `;` written first.
    if body.kind() == "enum_body" {
        return enum_insertion_point(body, source);
    }
    let mut cursor = body.walk();
    let mut after: Option<usize> = None;
    for child in body.named_children(&mut cursor) {
        if child.kind() == "field_declaration" {
            after = Some(child.end_byte());
        }
    }
    match after {
        Some(end) => {
            // Just past the field's line, so the new one starts on its own.
            let rest = &source[end..];
            Some(end + rest.find('\n').map(|i| i + 1).unwrap_or(0))
        }
        None => {
            let open = body.start_byte();
            let rest = source.get(open..)?;
            Some(open + rest.find('\n').map(|i| i + 1).unwrap_or(1))
        }
    }
}

/// The direct member of `body` that contains `expr` — a field, an initialiser block, an enum
/// constant — when the expression is NOT inside a method or constructor.
///
/// Inside one there is nothing to sit above: a method body may read a field declared anywhere in
/// the class, so the constant can go with the other fields.
fn member_containing<'t>(body: &Node<'t>, expr: &Node<'t>) -> Option<Node<'t>> {
    let mut node = *expr;
    let mut last = None;
    while let Some(parent) = node.parent() {
        if matches!(parent.kind(), "method_declaration" | "constructor_declaration") {
            return None;
        }
        if parent.id() == body.id() {
            last = Some(node);
            break;
        }
        node = parent;
    }
    last
}

/// The start of the line `offset` sits on — where a declaration inserted "before this one" goes.
fn line_start(source: &str, offset: usize) -> usize {
    source[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Where a field goes in an enum: in the member section after the constants, never before them.
fn enum_insertion_point(body: &Node<'_>, source: &str) -> Option<usize> {
    let mut cursor = body.walk();
    let members = body.named_children(&mut cursor).find(|c| c.kind() == "enum_body_declarations");
    if let Some(members) = members {
        let mut c = members.walk();
        let last_field =
            members.named_children(&mut c).filter(|n| n.kind() == "field_declaration").last();
        let end = last_field.map(|f| f.end_byte()).unwrap_or_else(|| members.start_byte());
        let rest = source.get(end..)?;
        return Some(end + rest.find('\n').map(|i| i + 1).unwrap_or(0));
    }
    None
}

/// The indentation members of this body are written with — read off the first one rather than
/// assumed, so a file indented with tabs or with two spaces keeps its own.
fn member_indent(source: &str, body: &Node<'_>) -> String {
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        let indent = indent_at(source, child.start_byte());
        if !indent.is_empty() {
            return indent;
        }
    }
    format!("{}    ", indent_at(source, body.start_byte()))
}

/// A name for the extracted expression, from what it is.
///
/// The point is a name the user will usually keep, and failing that one they can type over — which
/// is why the caret lands on it. A call is named after the method with its `get`/`is` prefix
/// dropped, a field access after the field, everything else `value`.
/// The scope a new name must not collide with: the method around the expression, else the type.
///
/// The method and not the whole class on purpose. A local that shadows a field the method never
/// mentions is harmless, and widening this to the type would make every second suggestion `value2`
/// in a class of any size.
fn scope_of<'t>(expr: &Node<'t>) -> Node<'t> {
    crate::selection::enclosing_callable(*expr)
        .or_else(|| enclosing_type(*expr))
        .unwrap_or(*expr)
}

/// Every VARIABLE name declared inside `scope` — parameters, locals, fields.
///
/// Variables only, and not every identifier in sight: Java keeps methods and variables in separate
/// namespaces, so a local called `compute` beside a method `compute()` is legal and renaming it
/// away would be a suggestion nobody asked for.
fn declared_names(scope: Node<'_>, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |node: &Node<'_>| {
        if let Some(name) = node.child_by_field_name("name") {
            out.push(text(&name, source).to_string());
        }
    };
    // `enum_constant` is in the list because an enum's constants ARE fields of it: an enum of SQL
    // types with a constant `TEXT` is where a constant named `TEXT` collides.
    for kind in [
        "variable_declarator",
        "formal_parameter",
        "catch_formal_parameter",
        "enhanced_for_statement",
        "enum_constant",
    ] {
        for node in crate::selection::descendants(scope, kind) {
            push(&node);
        }
    }
    // A field the method never mentions is safe to shadow; one it does mention is already covered
    // by the declarators above. What is NOT covered is the enclosing type's fields when the new
    // name is a field itself — extract constant — so the caller passes the type as the scope there.
    out
}

/// `base`, or `base2`, `base3`… — the first spelling nothing in `scope` already answers to.
///
/// Introducing a name that is already taken does not fail to compile in the interesting case: it
/// **shadows**, and every line after it that meant the old binding quietly means the new one. Seen
/// on real code as `var params = this.params;` inside a method whose own parameter was `params`.
fn unique_name(base: &str, scope: Node<'_>, source: &str) -> String {
    let taken = declared_names(scope, source);
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base}{i}"))
        .find(|candidate| !taken.iter().any(|t| t == candidate))
        .unwrap_or_else(|| base.to_string())
}

pub fn suggest_name(expr: &Node<'_>, source: &str) -> String {
    let raw = match expr.kind() {
        "method_invocation" => expr
            .child_by_field_name("name")
            .map(|n| text(&n, source).to_string())
            .unwrap_or_default(),
        "field_access" => expr
            .child_by_field_name("field")
            .map(|n| text(&n, source).to_string())
            .unwrap_or_default(),
        "object_creation_expression" => expr
            .child_by_field_name("type")
            .map(|n| text(&n, source).to_string())
            .unwrap_or_default(),
        "string_literal" => "text".to_string(),
        _ => String::new(),
    };
    let stripped = strip_accessor_prefix(&raw);
    let candidate = lower_first(&stripped);
    // `getClass()` strips to `class`, and `var class = …` is a syntax error rather than a bad name.
    if candidate.is_empty() || !is_identifier(&candidate) || is_reserved(&candidate) {
        return "value".to_string();
    }
    candidate
}

/// Java's reserved words, plus the three literals that cannot be identifiers either.
fn is_reserved(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class",
        "const", "continue", "default", "do", "double", "else", "enum", "extends", "final",
        "finally", "float", "for", "goto", "if", "implements", "import", "instanceof", "int",
        "interface", "long", "native", "new", "package", "private", "protected", "public",
        "return", "short", "static", "strictfp", "super", "switch", "synchronized", "this",
        "throw", "throws", "transient", "try", "void", "volatile", "while", "true", "false",
        "null", "_",
    ];
    RESERVED.contains(&name)
}

fn strip_accessor_prefix(name: &str) -> String {
    for prefix in ["get", "is", "find", "compute", "build", "create"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            if rest.chars().next().is_some_and(|c| c.is_uppercase()) {
                return rest.to_string();
            }
        }
    }
    name.to_string()
}

fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `pageSize` → `PAGE_SIZE`, which is what a constant is called.
fn screaming(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(ch.to_uppercase());
    }
    if out.is_empty() {
        "VALUE".to_string()
    } else {
        out
    }
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Whether a statement kind can host an inserted line above it — exported for the offer list, which
/// decides whether to show the row at all.
pub fn hosts_a_declaration(node: &Node<'_>) -> bool {
    is_statement(node) && node.parent().is_some_and(|p| p.kind() == "block")
}

/// The type declarations a constant can be added to — re-exported so a caller checking whether the
/// offer applies does not import the selection module for one constant.
pub const CONSTANT_HOSTS: &[&str] = TYPE_DECLS;

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    fn plan_var(source: &str, needle: &str) -> Plan {
        let tree = parse_java(source).unwrap();
        let at = source.find(needle).unwrap() + 1;
        match extract_variable(tree.root_node(), source, at, at) {
            Some(Ok(plan)) => plan,
            other => panic!("expected a plan, got {other:?}"),
        }
    }

    const SRC: &str = "class A {\n    void f(int n) {\n        System.out.println(compute(n) + 1);\n    }\n}";

    #[test]
    fn a_call_becomes_a_named_local_above_its_statement() {
        let plan = plan_var(SRC, "compute(n)");
        // `compute` is only stripped when something follows it — `computeTotal` → `total`, but
        // `compute` alone is the name.
        assert_eq!(plan.name.as_deref(), Some("compute"));
        let applied = plan.apply(SRC);
        assert!(applied.contains("var "), "{applied}");
        assert!(applied.contains("System.out.println("), "{applied}");
    }

    #[test]
    fn the_declaration_lands_on_its_own_line_with_the_statements_indentation() {
        let source = "class A {\n    void f() {\n        int x = size() + 1;\n    }\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("size()").unwrap() + 1;
        let plan = match extract_variable(tree.root_node(), source, at, at) {
            Some(Ok(p)) => p,
            other => panic!("{other:?}"),
        };
        let applied = plan.apply(source);
        assert!(applied.contains("\n        var size = size();\n        int x = size + 1;"), "{applied}");
    }

    /// The refusal that saves a nonsense edit: `int x = a + b;` with the caret on `a + b` already
    /// has a name.
    #[test]
    fn an_initialiser_that_is_already_named_is_refused_with_a_reason() {
        let source = "class A {\n    void f(int a, int b) {\n        int x = a + b;\n    }\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("a + b").unwrap() + 2;
        match extract_variable(tree.root_node(), source, at, at) {
            Some(Err(refusal)) => assert!(refusal.reason.contains("already the value")),
            other => panic!("{other:?}"),
        }
    }

    /// Selecting a bare name says so. A CARET on one does not — see
    /// `a_caret_on_a_name_means_the_expression_around_it`; the two are different gestures.
    #[test]
    fn a_selected_bare_name_is_refused_because_it_has_a_name() {
        let source = "class A {\n    void f(int a) {\n        g(a);\n    }\n    void g(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("g(a)").unwrap() + 2;
        match extract_variable(tree.root_node(), source, at, at + 1) {
            Some(Err(refusal)) => assert!(refusal.reason.contains("already a name")),
            other => panic!("{other:?}"),
        }
    }

    /// Regression: a caret inside an operand used to answer "this is already a name", which made
    /// extract variable useless at the commonest caret there is — inside an identifier.
    #[test]
    fn a_caret_on_a_name_means_the_expression_around_it() {
        let source = "class A {\n    int f(int count) {\n        return count * 2;\n    }\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("count * 2").unwrap() + 2;
        let Some(Ok(plan)) = extract_variable(tree.root_node(), source, at, at) else {
            panic!("expected a plan")
        };
        assert!(plan.apply(source).contains("var value = count * 2;"), "{}", plan.apply(source));
        assert!(plan.apply(source).contains("return value;"), "{}", plan.apply(source));
    }

    /// …and a name with no expression around it stays silent rather than filling the menu with a
    /// row about what the user is not doing.
    #[test]
    fn a_caret_on_a_name_with_nothing_around_it_is_silent() {
        let source = "class A {\n    void f() {\n        int value = 1;\n        take(value);\n    }\n    void take(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("int value").unwrap() + 5;
        assert!(extract_variable(tree.root_node(), source, at, at).is_none());
    }

    /// The same regression, end to end: the extracted expression IS the start of its statement.
    #[test]
    fn an_expression_that_starts_its_statement_keeps_its_declaration() {
        let source = "class A {\n    void f(Object param) {\n        this.items.add(param);\n    }\n    java.util.List<Object> items = new java.util.ArrayList<>();\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("this.items").unwrap() + 6;
        let Some(Ok(plan)) = extract_variable(tree.root_node(), source, at, at) else {
            panic!("expected a plan")
        };
        let applied = plan.apply(source);
        assert!(applied.contains("var items = this.items;"), "{applied}");
        assert!(applied.contains("items.add(param);"), "{applied}");
    }

    /// Shadowing does not fail to compile — it quietly re-points every later mention of the name.
    #[test]
    fn a_name_the_method_already_uses_gets_a_digit() {
        let source = "class A {\n    void f() {\n        int value = 1;\n        take(value + size());\n    }\n    int size() { return 2; }\n    void take(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("value + size()").unwrap();
        let Some(Ok(plan)) =
            extract_variable(tree.root_node(), source, start, start + "value + size()".len())
        else {
            panic!("expected a plan")
        };
        assert_eq!(plan.name.as_deref(), Some("value2"), "{:?}", plan.name);
    }

    /// …but a method of that name is not a collision: Java keeps the two namespaces apart, and
    /// bumping here would be a suggestion nobody asked for.
    #[test]
    fn a_method_of_the_same_name_is_not_a_collision() {
        let source = "class A {\n    void f() {\n        take(compute() + 1);\n    }\n    int compute() { return 2; }\n    void take(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("compute()").unwrap() + 2;
        let Some(Ok(plan)) = extract_variable(tree.root_node(), source, at, at) else {
            panic!("expected a plan")
        };
        assert_eq!(plan.name.as_deref(), Some("compute"), "{:?}", plan.name);
    }

    /// A lambda has no type of its own, and no resolver changes that — the answer is in the target.
    #[test]
    fn a_lambda_is_refused_because_its_type_comes_from_the_target() {
        let source = "class A {\n    void f() {\n        run(() -> g());\n    }\n    void run(Runnable r) {}\n    void g() {}\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("() -> g()").unwrap();
        match extract_variable(tree.root_node(), source, start, start + "() -> g()".len()) {
            Some(Err(refusal)) => assert!(refusal.reason.contains("no type of its own"), "{}", refusal.reason),
            other => panic!("{other:?}"),
        }
    }

    /// Regression: a constructor's body is a `constructor_body`, not a `block`, so every extract
    /// inside a constructor used to answer "the statement is not inside a block".
    #[test]
    fn a_constructor_body_is_a_block_like_any_other() {
        let source = "class A {\n    int n;\n    A(int k) {\n        take(k * 2);\n    }\n    void take(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("k * 2").unwrap() + 1;
        let Some(Ok(plan)) = extract_variable(tree.root_node(), source, at, at) else {
            panic!("expected a plan")
        };
        assert!(plan.apply(source).contains("var value = k * 2;"), "{}", plan.apply(source));
    }

    /// Regression: `list.add(x);` — the expression is the whole statement, so replacing "the use"
    /// leaves a bare `add;` behind, which is not a statement at all.
    #[test]
    fn an_expression_that_is_the_whole_statement_is_named_in_place() {
        let source = "class A {\n    void f(java.util.List<String> l, String x) {\n        l.add(x);\n    }\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("l.add(x)").unwrap() + 3;
        let Some(Ok(plan)) = extract_variable(tree.root_node(), source, at, at) else {
            panic!("expected a plan")
        };
        assert_eq!(plan.apply(source).matches("add").count(), 2, "{}", plan.apply(source));
        assert!(plan.apply(source).contains("var add = l.add(x);"), "{}", plan.apply(source));
    }

    /// Regression: a constant pulled out of a field's initialiser has to be declared above that
    /// field — below it is an "illegal forward reference".
    #[test]
    fn a_constant_used_by_a_field_is_declared_above_it() {
        let source = "class A {\n    private static final String A = \"x\" + \"y\";\n    private static final String B = \"z\";\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("\"y\"").unwrap();
        let Some(Ok(mut plan)) = extract_constant(tree.root_node(), source, start, start + 3) else {
            panic!("expected a plan")
        };
        plan.fill_type("String");
        let applied = plan.apply(source);
        let declared = applied.find("String TEXT =").expect("the constant");
        assert!(declared < applied.find("String A =").unwrap(), "{applied}");
    }

    /// Regression: a `for` condition. Hoisted above the loop it is computed once instead of every
    /// iteration — and it reads the `i` the loop has not declared yet.
    #[test]
    fn a_loop_condition_is_refused_because_it_runs_every_iteration() {
        let source = "class A {\n    void f(int len) {\n        for (int i = 0; i < len; i++) {\n            g(i);\n        }\n    }\n    void g(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("i < len").unwrap();
        match extract_variable(tree.root_node(), source, start, start + "i < len".len()) {
            Some(Err(refusal)) => assert!(refusal.reason.contains("every iteration"), "{}", refusal.reason),
            other => panic!("{other:?}"),
        }
    }

    /// …but the body is ordinary code, and so is the `init`, which already runs once.
    #[test]
    fn a_loops_body_is_not_its_header() {
        let source = "class A {\n    void f(int len) {\n        for (int i = 0; i < len; i++) {\n            g(i * 2);\n        }\n    }\n    void g(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("i * 2").unwrap();
        assert!(matches!(
            extract_variable(tree.root_node(), source, start, start + 5),
            Some(Ok(_))
        ));
    }

    /// Regression: a static initialiser reads fields declared before it, exactly like a field does.
    #[test]
    fn a_constant_used_by_a_static_block_is_declared_above_it() {
        let source = "class A {\n    static String v;\n    static {\n        v = \"seed\";\n    }\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("\"seed\"").unwrap();
        let Some(Ok(mut plan)) = extract_constant(tree.root_node(), source, start, start + 6) else {
            panic!("expected a plan")
        };
        plan.fill_type("String");
        let applied = plan.apply(source);
        assert!(
            applied.find("String TEXT =").unwrap() < applied.find("static {").unwrap(),
            "{applied}"
        );
    }

    /// Regression: no placement works inside an enum constant's arguments, so it is a refusal and
    /// not a layout problem — above the constant is not a member section, below it is a forward
    /// reference, and the constant is built before either way.
    #[test]
    fn a_constant_inside_an_enum_constants_arguments_is_refused() {
        let source = "enum E {\n    A(\"one\"),\n    B(\"two\");\n    private final String s;\n    E(String s) { this.s = s; }\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("\"one\"").unwrap();
        match extract_constant(tree.root_node(), source, start, start + 5) {
            Some(Err(refusal)) => assert!(refusal.reason.contains("before the class's static fields"), "{}", refusal.reason),
            other => panic!("{other:?}"),
        }
    }

    /// Regression: an expression inside a lambda. Hoisted above the statement it lands where the
    /// lambda's parameter does not exist — and where it is evaluated once instead of per call.
    #[test]
    fn an_expression_inside_a_lambda_is_not_hoisted_out_of_it() {
        let source = "import java.util.*;\nclass A {\n    void f(List<Integer> rows) {\n        rows.removeIf(i -> i > 10);\n    }\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("i > 10").unwrap();
        match extract_variable(tree.root_node(), source, start, start + "i > 10".len()) {
            Some(Err(refusal)) => assert!(refusal.reason.contains("out of that scope"), "{}", refusal.reason),
            other => panic!("{other:?}"),
        }
    }

    /// …while a lambda with a BLOCK body has statements of its own, and those are ordinary ground.
    #[test]
    fn a_statement_inside_a_lambda_block_is_ordinary_ground() {
        let source = "import java.util.*;\nclass A {\n    void f(List<Integer> rows) {\n        rows.forEach(i -> {\n            take(i * 2);\n        });\n    }\n    void take(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("i * 2").unwrap();
        assert!(matches!(
            extract_variable(tree.root_node(), source, start, start + 5),
            Some(Ok(_))
        ));
    }

    /// Regression: `Processor.Arch` is a nested TYPE. Naming it produced `var Arch = Processor.Arch;`
    /// — a variable of a name that is not a value.
    #[test]
    fn a_qualified_type_name_is_not_a_value_to_name() {
        let source = "class A {\n    void f() {\n        take(Processor.Arch.BIT_32);\n    }\n    void take(Object o) {}\n}\nclass Processor { enum Arch { BIT_32 } }";
        let tree = parse_java(source).unwrap();
        let start = source.find("Processor.Arch").unwrap();
        assert!(extract_variable(tree.root_node(), source, start, start + "Processor.Arch".len()).is_none());
    }

    /// …while a SCREAMING_CASE member is a constant, and naming one is the ordinary case.
    #[test]
    fn a_screaming_case_member_is_still_a_value() {
        let source = "class A {\n    void f() {\n        take(Integer.MAX_VALUE);\n    }\n    void take(int n) {}\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("Integer.MAX_VALUE").unwrap();
        assert!(matches!(
            extract_variable(tree.root_node(), source, start, start + "Integer.MAX_VALUE".len()),
            Some(Ok(_))
        ));
    }

    /// A statement with no block around it has nowhere to put the declaration, and manufacturing
    /// braces is a different refactoring.
    #[test]
    fn a_braceless_body_is_refused_rather_than_braced() {
        let source = "class A {\n    void f(int n) {\n        if (n > 0) g(n + 1);\n    }\n    void g(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("n + 1").unwrap() + 1;
        match extract_variable(tree.root_node(), source, at, at) {
            Some(Err(refusal)) => assert!(refusal.reason.contains("not inside a block")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_literal_becomes_a_screaming_constant_beside_the_other_fields() {
        let source = "class A {\n    private int a = 1;\n\n    void f() {\n        take(30_000);\n    }\n    void take(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("30_000").unwrap() + 1;
        let plan = match extract_constant(tree.root_node(), source, at, at) {
            Some(Ok(p)) => p,
            other => panic!("{other:?}"),
        };
        let applied = plan.apply(source);
        assert!(applied.contains("private static final var VALUE = 30_000;"), "{applied}");
        assert!(applied.contains("take(VALUE);"), "{applied}");
        // Beside the existing field, not above it.
        assert!(applied.find("private int a").unwrap() < applied.find("VALUE").unwrap(), "{applied}");
    }

    /// A call is not a constant: lifting it to a `static final` moves it to class-initialisation
    /// time, which is a behavioural change wearing a tidy-up's clothes.
    /// A field of an interface or an annotation type is already public static final; writing
    /// `private` there does not compile.
    #[test]
    fn a_constant_in_an_annotation_type_carries_no_modifiers() {
        let source = "@interface Column {\n    String value() default \"\";\n}";
        let tree = parse_java(source).unwrap();
        let at = source.find("default \"\"").unwrap() + 8;
        let Some(Ok(mut plan)) = extract_constant(tree.root_node(), source, at, at + 2) else {
            panic!("expected a plan")
        };
        plan.fill_type("String");
        let applied = plan.apply(source);
        assert!(!applied.contains("private"), "{applied}");
        assert!(applied.contains("String TEXT = \"\";"), "{applied}");
    }

    /// The worst kind of wrong: `var field = this.field; field = x;` compiles, and the assignment
    /// to the field silently stops happening.
    #[test]
    fn the_target_of_an_assignment_is_not_a_value_to_name() {
        let source = "class A {\n    Object field;\n    void f(Object x) {\n        this.field = x;\n    }\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("this.field = x").unwrap();
        match extract_variable(tree.root_node(), source, start, start + "this.field".len()) {
            Some(Err(refusal)) => assert!(refusal.reason.contains("where a value is written"), "{}", refusal.reason),
            other => panic!("{other:?}"),
        }
    }

    /// …while the index of an array write is an ordinary value.
    #[test]
    fn the_index_of_an_array_write_is_still_a_value() {
        let source = "class A {\n    void f(int[] a, int i) {\n        a[i + 1] = 0;\n    }\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("i + 1").unwrap();
        assert!(matches!(
            extract_variable(tree.root_node(), source, start, start + "i + 1".len()),
            Some(Ok(_))
        ));
    }

    /// An enum's constants come first; a field written before them is not a field, it is a syntax
    /// error that reads like one of the constants.
    #[test]
    fn a_constant_in_an_enum_lands_in_the_member_section() {
        let source = "enum E {\n    A,\n    B;\n\n    String label() {\n        return \"x\";\n    }\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("return \"x\"").unwrap() + "return ".len();
        let Some(Ok(mut plan)) = extract_constant(tree.root_node(), source, start, start + 3) else {
            panic!("expected a plan")
        };
        plan.fill_type("String");
        let applied = plan.apply(source);
        let field = applied.find("private static final").expect("a field");
        assert!(field > applied.find("B;").unwrap(), "{applied}");
    }

    /// Regression: `case "2":` needs a compile-time constant, and a local is not one — while a
    /// constant is, which is why this is a refusal on one offer and not the other.
    #[test]
    fn a_case_label_refuses_a_local_but_takes_a_constant() {
        let source = "class A {\n    void f(String k) {\n        switch (k) {\n            case \"two\":\n                break;\n        }\n    }\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("\"two\"").unwrap();
        match extract_variable(tree.root_node(), source, start, start + 5) {
            Some(Err(refusal)) => assert!(refusal.reason.contains("must be a constant"), "{}", refusal.reason),
            other => panic!("{other:?}"),
        }
        assert!(matches!(extract_constant(tree.root_node(), source, start, start + 5), Some(Ok(_))));
    }

    /// Regression: `getClass()` strips to `class`, and `var class = …` does not parse.
    #[test]
    fn a_suggested_name_is_never_a_keyword() {
        let source = "class A {\n    boolean f(Object o) {\n        return getClass() == o.getClass();\n    }\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("getClass()").unwrap();
        let Some(Ok(plan)) = extract_variable(tree.root_node(), source, start, start + "getClass()".len())
        else {
            panic!("expected a plan")
        };
        assert_eq!(plan.name.as_deref(), Some("value"), "{:?}", plan.name);
    }

    /// A field is never `var` in any Java version, so the caller must name the type or decline —
    /// unlike a local, where `var` is exactly what javac would have inferred.
    #[test]
    fn a_constants_type_is_not_optional() {
        let source = "class A {\n    void f() {\n        take(30_000);\n    }\n    void take(int x) {}\n}";
        let tree = parse_java(source).unwrap();
        let start = source.find("30_000").unwrap();
        let Some(Ok(constant)) = extract_constant(tree.root_node(), source, start, start + 6) else {
            panic!("expected a plan")
        };
        assert!(constant.type_slot.as_ref().is_some_and(|s| s.required), "{:?}", constant.type_slot);
        let Some(Ok(local)) = extract_variable(tree.root_node(), source, start, start + 6) else {
            panic!("expected a plan")
        };
        assert!(local.type_slot.as_ref().is_some_and(|s| !s.required), "{:?}", local.type_slot);
    }

    #[test]
    fn a_call_is_not_offered_as_a_constant() {
        let tree = parse_java(SRC).unwrap();
        let at = SRC.find("compute(n)").unwrap() + 1;
        assert!(extract_constant(tree.root_node(), SRC, at, at).is_none());
    }

    #[test]
    fn a_constant_name_reads_the_way_constants_are_written() {
        assert_eq!(screaming("pageSize"), "PAGE_SIZE");
        assert_eq!(screaming("x"), "X");
        assert_eq!(screaming(""), "VALUE");
    }
}
