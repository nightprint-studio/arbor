//! The repairs for a blank `final` field nothing initialises — IntelliJ's four, as byte-range edits.
//!
//! ```java
//! public class CheckAssignedUser {
//!     private final PaMsRestClientApi client;   // never assigned: does not compile
//! }
//! ```
//!
//! - **Add constructor parameter** — every constructor that does its own initialising takes the
//!   field as a parameter and assigns it; with no constructor at all, one is written.
//! - **Initialize in constructor** — the same assignment, from a placeholder value.
//! - **Initialize variable** — `= null`, or the type's zero.
//! - **Make not final.**
//!
//! Plus the edit behind *Add `@RequiredArgsConstructor`* — the annotation only. Whether Lombok is on
//! the classpath, the import, and whether this project writes it that way are the caller's, which has
//! the project; this crate has one buffer.
//!
//! ## Which fields
//!
//! Not decided here. The caller passes the name spans the blank-final **check** reported
//! (`bennu_check::prelude::uninitialized_final_fields`), so a fix is never offered for a field the
//! check does not flag — a field assigned in one `if` branch, a `@Data` class — and never missed for
//! one it does. A second opinion about definite assignment would be a second set of bugs.
//!
//! ## Why a delegating constructor is left alone
//!
//! `A() { this(1); }` initialises nothing itself: the constructor it calls does. Giving it the
//! parameter and the assignment as well would assign a `final` twice — a new compile error in place
//! of the old one.

use tree_sitter::Node;

use crate::body::{
    body_of, declared_names, has_modifier, indent_step, insertion_point, line_start, member_indent,
    members_of,
};
use crate::plan::{EditSelection, RefactorEdit};
use crate::selection::{enclosing, indent_at, newline, text};

/// One repair, ready to apply: its edits are in **descending** start order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalFieldFix {
    pub id: String,
    pub label: String,
    pub edits: Vec<RefactorEdit>,
    /// The placeholder value the fix wrote — `null`, `0`, `false` — indexed into `edits` as they
    /// are ordered here, so the editor can select it and the next keystroke replaces it. `None` for
    /// a fix that writes no placeholder.
    pub select: Option<EditSelection>,
}

/// What can be done about the blank final under the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalFieldFixes {
    /// The field's name.
    pub field: String,
    /// The repairs, in the order a menu shows them.
    pub fixes: Vec<FinalFieldFix>,
    /// The `@RequiredArgsConstructor` line above the class, when Lombok *could* write the
    /// constructor instead: an instance field of a class that declares no constructor of its own
    /// (Lombok refuses to generate one beside an explicit constructor). Without the import, which the
    /// caller adds.
    pub required_args_constructor: Option<RefactorEdit>,
}

/// A blank final, as the edits need it.
struct BlankField {
    name: String,
    /// The type as written, with any `[]` the declarator carries (`int x[]`).
    ty: String,
}

/// The repairs for the blank final field whose declaration holds `offset` — on its modifiers, its
/// type or its name, the way Alt+Enter is pressed on it.
///
/// `blanks` are the name spans of the fields the blank-final check reports. `None` when the caret is
/// on no such declaration.
pub fn final_field_fixes(
    root: Node<'_>,
    source: &str,
    offset: usize,
    blanks: &[(usize, usize)],
) -> Option<FinalFieldFixes> {
    if blanks.is_empty() || offset > source.len() {
        return None;
    }
    // The caret just past the `;` is still "on" the declaration it ends.
    let field = [offset, offset.saturating_sub(1)].into_iter().find_map(|at| {
        let node = root.named_descendant_for_byte_range(at, at)?;
        enclosing(node, &["field_declaration"])
    })?;
    let declarators = declarators_of(&field);
    let is_blank = |d: &Node<'_>| {
        d.child_by_field_name("name")
            .is_some_and(|n| blanks.contains(&(n.start_byte(), n.end_byte())))
    };
    // `final int a, b;` — the declarator under the caret when it is one, else the first blank one.
    let declarator = declarators
        .iter()
        .find(|d| is_blank(*d) && d.start_byte() <= offset && offset <= d.end_byte())
        .or_else(|| declarators.iter().find(|d| is_blank(*d)))
        .copied()?;
    let type_decl = enclosing(field, &["class_declaration", "enum_declaration"])?;
    let body = body_of(&type_decl)?;
    let chosen = blank_field(&field, &declarator, source)?;
    let is_static = has_modifier(&field, source, "static");
    let is_class = type_decl.kind() == "class_declaration";

    let mut fixes = Vec::new();
    if !is_static {
        // An enum's constants are its constructor's callers: a new parameter breaks every one of them.
        if is_class {
            let edits = add_constructor_parameters(&type_decl, &body, source, std::slice::from_ref(&chosen));
            push_fix(&mut fixes, "add-constructor-parameter", "Add constructor parameter", edits, None);
            let all = instance_blanks(&body, source, blanks);
            if all.len() > 1 {
                let edits = add_constructor_parameters(&type_decl, &body, source, &all);
                push_fix(
                    &mut fixes,
                    "add-constructor-parameters",
                    "Add constructor parameters for all final fields",
                    edits,
                    None,
                );
            }
        }
        let value = default_value(&chosen.ty);
        let assignment = format!("this.{} = {value};", chosen.name);
        let edits = initialize_in_constructors(&type_decl, &body, source, &assignment);
        // Several constructors each get the assignment; the value is selected in the first — one
        // selection is what a keystroke can replace, and the first is where the eye already is.
        let select = edits
            .first()
            .and_then(|first| value_in_assignment(&first.text, &assignment, value))
            .map(|(start, end)| EditSelection { edit: 0, start, end });
        push_fix(&mut fixes, "initialize-in-constructor", "Initialize in constructor", edits, select);
    }
    let value = default_value(&chosen.ty);
    let written = format!(" = {value}");
    let select = EditSelection { edit: 0, start: written.len() - value.len(), end: written.len() };
    let initializer = RefactorEdit::new(declarator.end_byte(), declarator.end_byte(), written, "declaration");
    push_fix(
        &mut fixes,
        "initialize-variable",
        &format!("Initialize variable '{}'", chosen.name),
        vec![initializer],
        Some(select),
    );
    if let Some(edit) = remove_final(&field, source) {
        push_fix(&mut fixes, "make-not-final", &format!("Make '{}' not final", chosen.name), vec![edit], None);
    }

    let required_args_constructor = (is_class && !is_static && constructors_of(&body).is_empty())
        .then(|| required_args_constructor_edit(&type_decl, source));

    Some(FinalFieldFixes { field: chosen.name, fixes, required_args_constructor })
}

/// Record a fix, its edits in application order. `select` indexes `edits` as **passed**; the shared
/// ordering carries it so it still names the same edit afterwards.
fn push_fix(
    fixes: &mut Vec<FinalFieldFix>,
    id: &str,
    label: &str,
    edits: Vec<RefactorEdit>,
    select: Option<EditSelection>,
) {
    if edits.is_empty() {
        return;
    }
    let (edits, select) = crate::plan::in_application_order(edits, select);
    fixes.push(FinalFieldFix { id: id.to_string(), label: label.to_string(), edits, select });
}

/// Where `value` sits in `text`, found through the `assignment` (`this.x = value;`) that wrote it —
/// the assignment rather than the bare value, so a `null` elsewhere in the inserted text (a
/// constructor written from scratch holds more than the statement) is never the one picked.
fn value_in_assignment(text: &str, assignment: &str, value: &str) -> Option<(usize, usize)> {
    let at = text.find(assignment)?;
    let end = at + assignment.len() - ";".len();
    Some((end - value.len(), end))
}

fn declarators_of<'t>(field: &Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = field.walk();
    let found: Vec<Node<'t>> =
        field.named_children(&mut cursor).filter(|n| n.kind() == "variable_declarator").collect();
    found
}

fn blank_field(field: &Node<'_>, declarator: &Node<'_>, source: &str) -> Option<BlankField> {
    let name = text(&declarator.child_by_field_name("name")?, source).to_string();
    let written = text(&field.child_by_field_name("type")?, source);
    let dims = declarator.child_by_field_name("dimensions").map(|d| text(&d, source)).unwrap_or_default();
    Some(BlankField { name, ty: format!("{written}{dims}") })
}

/// Every blank **instance** final of the body, in source order — what "for all final fields" covers.
fn instance_blanks(body: &Node<'_>, source: &str, blanks: &[(usize, usize)]) -> Vec<BlankField> {
    members_of(body)
        .into_iter()
        .filter(|m| m.kind() == "field_declaration" && !has_modifier(m, source, "static"))
        .flat_map(|field| {
            declarators_of(&field)
                .into_iter()
                .filter(|d| {
                    d.child_by_field_name("name")
                        .is_some_and(|n| blanks.contains(&(n.start_byte(), n.end_byte())))
                })
                .filter_map(|d| blank_field(&field, &d, source))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The value a placeholder initialiser is written with: the type's zero, `null` for a reference.
fn default_value(ty: &str) -> &'static str {
    match ty.trim() {
        "boolean" => "false",
        "byte" | "short" | "int" => "0",
        "long" => "0L",
        "float" => "0.0f",
        "double" => "0.0",
        "char" => "'\\0'",
        _ => "null",
    }
}

fn constructors_of<'t>(body: &Node<'t>) -> Vec<Node<'t>> {
    members_of(body).into_iter().filter(|m| m.kind() == "constructor_declaration").collect()
}

/// Whether a constructor hands its initialising to another one — `this(…)` as its first statement.
fn delegates_to_this(ctor: &Node<'_>, source: &str) -> bool {
    let Some(body) = ctor.child_by_field_name("body") else { return false };
    let mut cursor = body.walk();
    let first = body
        .named_children(&mut cursor)
        .find(|n| !matches!(n.kind(), "line_comment" | "block_comment"));
    first.is_some_and(|n| {
        n.kind() == "explicit_constructor_invocation" && text(&n, source).trim_start().starts_with("this")
    })
}

/// "Add constructor parameter(s)": each initialising constructor takes the fields it does not already
/// have a parameter for, and assigns all of them; with no constructor, one is written.
fn add_constructor_parameters(
    type_decl: &Node<'_>,
    body: &Node<'_>,
    source: &str,
    fields: &[BlankField],
) -> Vec<RefactorEdit> {
    let assignments: Vec<String> = fields.iter().map(|f| format!("this.{0} = {0};", f.name)).collect();
    let ctors = constructors_of(body);
    if ctors.is_empty() {
        let params = fields.iter().map(|f| format!("{} {}", f.ty, f.name)).collect::<Vec<_>>().join(", ");
        return new_constructor(type_decl, body, source, &params, &assignments).into_iter().collect();
    }
    let mut edits = Vec::new();
    for ctor in ctors.iter().filter(|c| !delegates_to_this(c, source)) {
        edits.extend(parameter_edit(ctor, source, fields));
        edits.extend(append_statements(ctor, source, &assignments));
    }
    edits
}

/// "Initialize in constructor": the placeholder assignment in each initialising constructor, or a
/// no-argument constructor holding it.
fn initialize_in_constructors(
    type_decl: &Node<'_>,
    body: &Node<'_>,
    source: &str,
    assignment: &str,
) -> Vec<RefactorEdit> {
    let statements = [assignment.to_string()];
    let ctors = constructors_of(body);
    if ctors.is_empty() {
        return new_constructor(type_decl, body, source, "", &statements).into_iter().collect();
    }
    ctors
        .iter()
        .filter(|c| !delegates_to_this(c, source))
        .filter_map(|c| append_statements(c, source, &statements))
        .collect()
}

/// The parameters `fields` add to `ctor` — only the ones it has no parameter of that name for, and
/// before a varargs parameter, which has to stay last.
fn parameter_edit(ctor: &Node<'_>, source: &str, fields: &[BlankField]) -> Option<RefactorEdit> {
    let params = ctor.child_by_field_name("parameters")?;
    let taken = declared_names(params, source);
    let missing: Vec<String> = fields
        .iter()
        .filter(|f| !taken.contains(&f.name))
        .map(|f| format!("{} {}", f.ty, f.name))
        .collect();
    if missing.is_empty() {
        return None;
    }
    let list = missing.join(", ");
    let mut cursor = params.walk();
    let existing: Vec<Node<'_>> = params
        .named_children(&mut cursor)
        .filter(|n| matches!(n.kind(), "formal_parameter" | "spread_parameter" | "receiver_parameter"))
        .collect();
    if let Some(spread) = existing.iter().find(|n| n.kind() == "spread_parameter") {
        return Some(RefactorEdit::new(spread.start_byte(), spread.start_byte(), format!("{list}, "), "declaration"));
    }
    match existing.last() {
        Some(last) => Some(RefactorEdit::new(last.end_byte(), last.end_byte(), format!(", {list}"), "declaration")),
        None => {
            let close = params.end_byte().checked_sub(1)?;
            Some(RefactorEdit::new(close, close, list, "declaration"))
        }
    }
}

/// `statements` at the end of a constructor's body, one per line at the body's statement indent. A
/// body written on one line (`A() {}`) is opened up, so the assignment does not land beside a brace.
fn append_statements(ctor: &Node<'_>, source: &str, statements: &[String]) -> Option<RefactorEdit> {
    let body = ctor.child_by_field_name("body")?;
    let brace = body.end_byte().checked_sub(1)?;
    let nl = newline(source);
    let indent = indent_at(source, ctor.start_byte());
    let inner = format!("{indent}{}", indent_step(source));
    let brace_line = line_start(source, brace);
    let brace_alone = brace_line > body.start_byte() && source[brace_line..brace].trim().is_empty();
    if brace_alone {
        let text: String = statements.iter().map(|s| format!("{inner}{s}{nl}")).collect();
        return Some(RefactorEdit::new(brace_line, brace_line, text, "body"));
    }
    let content_end = body.start_byte() + source[body.start_byte()..brace].trim_end().len();
    let mut text: String = statements.iter().map(|s| format!("{nl}{inner}{s}")).collect();
    text.push_str(nl);
    text.push_str(&indent);
    Some(RefactorEdit::new(content_end, brace, text, "body"))
}

/// A constructor written after the last field, with the class's own visibility — a `public` class
/// gets a `public` constructor; an enum's constructor carries none, the only one it may.
fn new_constructor(
    type_decl: &Node<'_>,
    body: &Node<'_>,
    source: &str,
    params: &str,
    statements: &[String],
) -> Option<RefactorEdit> {
    let name = text(&type_decl.child_by_field_name("name")?, source);
    let visibility = match type_decl.kind() {
        "enum_declaration" => "",
        _ => ["public", "protected", "private"]
            .into_iter()
            .find(|v| has_modifier(type_decl, source, v))
            .unwrap_or(""),
    };
    let visibility = if visibility.is_empty() { String::new() } else { format!("{visibility} ") };
    let nl = newline(source);
    let indent = member_indent(source, body);
    let step = indent_step(source);
    let mut ctor = format!("{indent}{visibility}{name}({params}) {{{nl}");
    for statement in statements {
        ctor.push_str(&format!("{indent}{step}{statement}{nl}"));
    }
    ctor.push_str(&format!("{indent}}}"));

    let brace = body.end_byte().checked_sub(1)?;
    let at = insertion_point(body, source)?;
    if at <= brace && source[..at].ends_with('\n') {
        return Some(RefactorEdit::new(at, at, format!("{nl}{ctor}{nl}"), "declaration"));
    }
    // A class written on one line has no line after its last field to land on.
    let content_end = body.start_byte() + source[body.start_byte()..brace].trim_end().len();
    let closing = indent_at(source, type_decl.start_byte());
    Some(RefactorEdit::new(content_end, brace, format!("{nl}{nl}{ctor}{nl}{closing}"), "declaration"))
}

/// The `final` keyword and the whitespace after it.
fn remove_final(field: &Node<'_>, source: &str) -> Option<RefactorEdit> {
    let mut cursor = field.walk();
    let modifiers = field.children(&mut cursor).find(|c| c.kind() == "modifiers")?;
    let mut inner = modifiers.walk();
    let keyword = modifiers.children(&mut inner).find(|m| text(m, source) == "final")?;
    let after = &source[keyword.end_byte()..];
    let end = keyword.end_byte() + (after.len() - after.trim_start().len());
    Some(RefactorEdit::new(keyword.start_byte(), end, "", "declaration"))
}

/// `@RequiredArgsConstructor` on its own line above the class, at the class's indentation.
fn required_args_constructor_edit(type_decl: &Node<'_>, source: &str) -> RefactorEdit {
    let at = line_start(source, type_decl.start_byte());
    let indent = indent_at(source, type_decl.start_byte());
    RefactorEdit::new(at, at, format!("{indent}@RequiredArgsConstructor{}", newline(source)), "declaration")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    /// The name spans of `names`, the way the check reports them.
    fn blanks(src: &str, names: &[&str]) -> Vec<(usize, usize)> {
        names
            .iter()
            .map(|name| {
                let at = src.find(&format!(" {name};")).expect("field present") + 1;
                (at, at + name.len())
            })
            .collect()
    }

    fn fixes_at(src: &str, caret: usize, names: &[&str]) -> Option<FinalFieldFixes> {
        let tree = parse_java(src).unwrap();
        final_field_fixes(tree.root_node(), src, caret, &blanks(src, names))
    }

    fn ids(src: &str, caret: usize, names: &[&str]) -> Vec<String> {
        fixes_at(src, caret, names).map(|f| f.fixes.into_iter().map(|x| x.id).collect()).unwrap_or_default()
    }

    /// The source after the fix `id` offered at `marker`, for the blank finals `names`.
    fn applied(src: &str, marker: &str, names: &[&str], id: &str) -> String {
        let fixes = fixes_at(src, src.find(marker).expect("marker"), names).expect("fixes");
        let fix = fixes.fixes.iter().find(|f| f.id == id).unwrap_or_else(|| panic!("no {id}: {fixes:?}"));
        apply(src, &fix.edits)
    }

    fn apply(src: &str, edits: &[RefactorEdit]) -> String {
        let mut out = src.to_string();
        for e in edits {
            out.replace_range(e.start..e.end, &e.text);
        }
        out
    }

    /// Where the fix `id` puts its selection in the applied source, and the text it covers — found
    /// the way the editor finds it: the chosen edit's landing position, shifted by every edit before
    /// it.
    fn selection(src: &str, marker: &str, names: &[&str], id: &str) -> Option<(usize, String)> {
        let fixes = fixes_at(src, src.find(marker).expect("marker"), names).expect("fixes");
        let fix = fixes.fixes.iter().find(|f| f.id == id).unwrap_or_else(|| panic!("no {id}: {fixes:?}"));
        let select = fix.select?;
        let chosen = &fix.edits[select.edit];
        let shift: isize = fix
            .edits
            .iter()
            .filter(|e| e.start < chosen.start)
            .map(|e| e.text.len() as isize - (e.end - e.start) as isize)
            .sum();
        let landed = (chosen.start as isize + shift) as usize;
        let out = apply(src, &fix.edits);
        Some((landed + select.start, out[landed + select.start..landed + select.end].to_string()))
    }

    const USER: &str = "package p;\n\npublic class CheckAssignedUser implements AttributeValidator {\n    private final PaMsRestClientApi client;\n\n    public boolean validate(Object o) {\n        return true;\n    }\n}\n";

    #[test]
    fn a_class_without_a_constructor_gets_one_after_its_last_field() {
        assert_eq!(
            applied(USER, "client;", &["client"], "add-constructor-parameter"),
            "package p;\n\npublic class CheckAssignedUser implements AttributeValidator {\n    private final PaMsRestClientApi client;\n\n    public CheckAssignedUser(PaMsRestClientApi client) {\n        this.client = client;\n    }\n\n    public boolean validate(Object o) {\n        return true;\n    }\n}\n"
        );
    }

    /// Alt+Enter is pressed anywhere on the declaration: the modifiers, the type, the name, past the `;`.
    #[test]
    fn the_caret_can_be_anywhere_on_the_declaration() {
        for marker in ["private final", "final Pa", "PaMsRestClientApi client", "client;"] {
            let at = USER.find(marker).unwrap();
            assert!(ids(USER, at, &["client"]).contains(&"add-constructor-parameter".to_string()), "{marker}");
        }
        let past_semicolon = USER.find("client;").unwrap() + "client;".len();
        assert!(!ids(USER, past_semicolon, &["client"]).is_empty());
    }

    #[test]
    fn nothing_is_offered_off_the_declaration_or_for_a_field_the_check_did_not_flag() {
        assert!(fixes_at(USER, USER.find("return true").unwrap(), &["client"]).is_none());
        assert!(fixes_at(USER, USER.find("client;").unwrap(), &[]).is_none());
    }

    #[test]
    fn the_offers_come_in_intellij_order() {
        assert_eq!(
            ids(USER, USER.find("client;").unwrap(), &["client"]),
            ["add-constructor-parameter", "initialize-in-constructor", "initialize-variable", "make-not-final"]
        );
    }

    #[test]
    fn a_crlf_file_keeps_its_newlines() {
        let src = USER.replace('\n', "\r\n");
        let out = applied(&src, "client;", &["client"], "add-constructor-parameter");
        assert!(
            out.contains("\r\n    public CheckAssignedUser(PaMsRestClientApi client) {\r\n        this.client = client;\r\n    }\r\n"),
            "{out:?}"
        );
        assert!(!out.replace("\r\n", "").contains('\n'), "a lone LF crept in: {out:?}");
    }

    #[test]
    fn the_indentation_is_the_files_own() {
        let src = "class A {\n  final int n;\n  void f() {}\n}\n";
        assert_eq!(
            applied(src, "n;", &["n"], "add-constructor-parameter"),
            "class A {\n  final int n;\n\n  A(int n) {\n    this.n = n;\n  }\n  void f() {}\n}\n"
        );
    }

    /// `A() { this(1); }` initialises nothing itself — touching it would assign the final twice.
    #[test]
    fn existing_constructors_take_the_parameter_but_a_delegating_one_does_not() {
        let src = "class A {\n    private final int n;\n    A() {\n        this(1);\n    }\n    A(int x) {\n        foo();\n    }\n}\n";
        assert_eq!(
            applied(src, "n;", &["n"], "add-constructor-parameter"),
            "class A {\n    private final int n;\n    A() {\n        this(1);\n    }\n    A(int x, int n) {\n        foo();\n        this.n = n;\n    }\n}\n"
        );
    }

    #[test]
    fn a_new_parameter_goes_before_varargs() {
        let src = "class A {\n    final int n;\n    A(String... xs) {\n    }\n}\n";
        assert_eq!(
            applied(src, "n;", &["n"], "add-constructor-parameter"),
            "class A {\n    final int n;\n    A(int n, String... xs) {\n        this.n = n;\n    }\n}\n"
        );
    }

    #[test]
    fn a_constructor_that_already_has_the_parameter_only_gains_the_assignment() {
        let src = "class A {\n    final int n;\n    A(int n) {\n    }\n}\n";
        assert_eq!(
            applied(src, "n;", &["n"], "add-constructor-parameter"),
            "class A {\n    final int n;\n    A(int n) {\n        this.n = n;\n    }\n}\n"
        );
    }

    #[test]
    fn initialize_in_constructor_opens_up_a_one_line_body() {
        let src = "class A {\n    final String s;\n    A() {}\n}\n";
        assert_eq!(
            applied(src, "s;", &["s"], "initialize-in-constructor"),
            "class A {\n    final String s;\n    A() {\n        this.s = null;\n    }\n}\n"
        );
    }

    #[test]
    fn initialize_in_constructor_writes_a_no_arg_constructor_when_there_is_none() {
        let out = applied(USER, "client;", &["client"], "initialize-in-constructor");
        assert!(out.contains("\n    public CheckAssignedUser() {\n        this.client = null;\n    }\n"), "{out}");
    }

    #[test]
    fn a_placeholder_is_the_types_zero() {
        for (ty, value) in [("int", "0"), ("boolean", "false"), ("long", "0L"), ("double", "0.0"), ("String", "null"), ("int[]", "null")] {
            let src = format!("class A {{\n    private final {ty} v;\n}}\n");
            let out = applied(&src, "v;", &["v"], "initialize-variable");
            assert!(out.contains(&format!("private final {ty} v = {value};")), "{out}");
            let out = applied(&src, "v;", &["v"], "initialize-in-constructor");
            assert!(out.contains(&format!("this.v = {value};")), "{out}");
        }
    }

    /// The placeholder is selected, IntelliJ-style, so typing the real value replaces it.
    #[test]
    fn the_placeholder_is_selected_to_be_typed_over() {
        for (ty, value) in [("Object", "null"), ("int", "0"), ("boolean", "false"), ("long", "0L")] {
            let src = format!("class A {{\n    private final {ty} v;\n}}\n");
            for id in ["initialize-variable", "initialize-in-constructor"] {
                let (_, text) = selection(&src, "v;", &["v"], id).unwrap_or_else(|| panic!("{id} selects nothing"));
                assert_eq!(text, value, "{id} on {ty}");
            }
        }
    }

    #[test]
    fn with_several_constructors_the_value_in_the_first_is_selected() {
        let src = "class A {\n    final String s;\n    A() {\n    }\n    A(int x) {\n    }\n}\n";
        let out = applied(src, "s;", &["s"], "initialize-in-constructor");
        assert_eq!(
            out,
            "class A {\n    final String s;\n    A() {\n        this.s = null;\n    }\n    A(int x) {\n        this.s = null;\n    }\n}\n"
        );
        assert_eq!(selection(src, "s;", &["s"], "initialize-in-constructor"), Some((out.find("null").unwrap(), "null".to_string())));
    }

    #[test]
    fn the_fixes_that_write_no_placeholder_select_nothing() {
        for id in ["add-constructor-parameter", "make-not-final"] {
            assert_eq!(selection(USER, "client;", &["client"], id), None, "{id}");
        }
    }

    #[test]
    fn make_not_final_removes_the_keyword_and_its_space() {
        assert_eq!(
            applied(USER, "client;", &["client"], "make-not-final"),
            USER.replace("private final PaMsRestClientApi", "private PaMsRestClientApi")
        );
    }

    #[test]
    fn a_static_final_is_offered_neither_constructor_fix_nor_lombok() {
        let src = "public class A {\n    private static final int MAX;\n}\n";
        let fixes = fixes_at(src, src.find("MAX;").unwrap(), &["MAX"]).unwrap();
        assert_eq!(fixes.fixes.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), ["initialize-variable", "make-not-final"]);
        assert!(fixes.required_args_constructor.is_none());
    }

    #[test]
    fn several_blank_finals_get_one_constructor_for_all_of_them() {
        let src = "public class A {\n    private final String a;\n    private final int b;\n}\n";
        assert_eq!(
            applied(src, "a;", &["a", "b"], "add-constructor-parameters"),
            "public class A {\n    private final String a;\n    private final int b;\n\n    public A(String a, int b) {\n        this.a = a;\n        this.b = b;\n    }\n}\n"
        );
        assert!(!ids(USER, USER.find("client;").unwrap(), &["client"]).contains(&"add-constructor-parameters".to_string()));
    }

    #[test]
    fn a_one_line_class_still_gets_a_well_formed_constructor() {
        let src = "class A { final int n; }";
        assert_eq!(
            applied(src, "n;", &["n"], "add-constructor-parameter"),
            "class A { final int n;\n\n    A(int n) {\n        this.n = n;\n    }\n}"
        );
    }

    /// An enum's constants call its constructor: a new parameter would break every one of them.
    #[test]
    fn an_enum_is_not_offered_a_constructor_parameter() {
        let src = "enum E {\n    A;\n    private final int n;\n}\n";
        let found = ids(src, src.find("n;").unwrap(), &["n"]);
        assert!(!found.contains(&"add-constructor-parameter".to_string()), "{found:?}");
        let out = applied(src, "n;", &["n"], "initialize-in-constructor");
        assert!(out.contains("\n    E() {\n        this.n = 0;\n    }\n"), "{out}");
    }

    #[test]
    fn the_lombok_annotation_goes_above_the_class_only_without_a_constructor() {
        let src = "import lombok.Getter;\n\n@Getter\npublic class A {\n    private final int n;\n}\n";
        let fixes = fixes_at(src, src.find("n;").unwrap(), &["n"]).unwrap();
        let edit = fixes.required_args_constructor.expect("offered");
        assert_eq!(
            apply(src, &[edit]),
            "import lombok.Getter;\n\n@RequiredArgsConstructor\n@Getter\npublic class A {\n    private final int n;\n}\n"
        );
        let with_ctor = "public class A {\n    private final int n;\n    A(boolean b) {\n    }\n}\n";
        assert!(fixes_at(with_ctor, with_ctor.find("n;").unwrap(), &["n"]).unwrap().required_args_constructor.is_none());
    }
}
