//! Where a member lives, and where a new one goes.
//!
//! ## Why this module exists
//!
//! Four refactorings now write a *member* — a field, a method, a whole nested type — into a type's
//! body: [`extract_var::extract_constant`], [`field::introduce_field`],
//! [`move_member`], [`move_class`]. Each of them has to answer the same four questions, and each
//! of them answered a slightly different way while the answers lived in whichever module needed
//! them first:
//!
//! - **where** does a new member go in this body (and an `enum` is not a class);
//! - **what indentation** do members of this body carry;
//! - **which names** are already taken here;
//! - **which member** is the caret standing on.
//!
//! One copy each, so a fix to the enum case is a fix everywhere rather than in the one module that
//! was measured.
//!
//! [`extract_var::extract_constant`]: crate::extract_var::extract_constant
//! [`field::introduce_field`]: crate::field::introduce_field
//! [`move_member`]: crate::move_member
//! [`move_class`]: crate::move_class

use tree_sitter::Node;

use crate::selection::{descendants, indent_at, text, TYPE_DECLS};

/// The member declarations a type body holds, in source order.
pub const MEMBERS: &[&str] = &[
    "field_declaration",
    "method_declaration",
    "constructor_declaration",
    "compact_constructor_declaration",
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
    "static_initializer",
    "block",
];

/// The start of the line `offset` sits on — where a declaration written "before this one" goes.
pub fn line_start(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// The offset just past the end of the line `offset` sits on, newline included.
pub fn line_after(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    source[offset..].find('\n').map(|i| offset + i + 1).unwrap_or(source.len())
}

/// The body of a type declaration.
pub fn body_of<'t>(type_decl: &Node<'t>) -> Option<Node<'t>> {
    type_decl.child_by_field_name("body")
}

/// The direct members of a type body, in source order.
pub fn members_of<'t>(body: &Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = body.walk();
    // An enum keeps its members in an `enum_body_declarations` child, after the constants.
    let mut out = Vec::new();
    for child in body.named_children(&mut cursor) {
        if child.kind() == "enum_body_declarations" {
            let mut inner = child.walk();
            out.extend(child.named_children(&mut inner).filter(|n| MEMBERS.contains(&n.kind())));
        } else if MEMBERS.contains(&child.kind()) {
            out.push(child);
        }
    }
    out
}

/// The name a member declares, when it declares one it can be found by.
///
/// A field can declare several (`int a, b;`) and answers with the first: every caller here already
/// refuses a multi-declarator field, and answering `None` instead would make them refuse it for the
/// wrong reason.
pub fn member_name<'a>(member: &Node<'_>, source: &'a str) -> Option<&'a str> {
    match member.kind() {
        "field_declaration" => descendants(*member, "variable_declarator")
            .first()
            .and_then(|d| d.child_by_field_name("name"))
            .map(|n| text(&n, source)),
        _ => member.child_by_field_name("name").map(|n| text(&n, source)),
    }
}

/// Where a new member goes in `body`.
///
/// After the last field when there is one, right after the `{` otherwise — which reads the way a
/// person writes a class, and keeps a new field above the methods that use it. An `enum` is the
/// exception and gets its own answer: its constants must come first, so "right after the `{`" puts
/// a field where the grammar wants `INTEGER,`.
pub fn insertion_point(body: &Node<'_>, source: &str) -> Option<usize> {
    if body.kind() == "enum_body" {
        return enum_insertion_point(body, source);
    }
    let last_field = members_of(body).into_iter().filter(|c| c.kind() == "field_declaration").last();
    match last_field {
        Some(field) => Some(line_after(source, field.end_byte())),
        None => Some(line_after(source, body.start_byte())),
    }
}

/// Where a new member goes at the **end** of a body — just before its closing brace.
///
/// What a *moved* member wants, and not what a new field wants: a member that came from somewhere
/// else keeps the company it had, and appending it is the only placement that never reorders
/// anything the reader was relying on.
pub fn append_point(body: &Node<'_>, source: &str) -> Option<usize> {
    // The `}` itself, not one past it: comparing against the range that *includes* the brace is
    // how this landed **inside** the closing line for two days. The result compiled — javac has no
    // opinion about where a brace sits — so nothing but a test could have said so.
    let brace = body.end_byte().checked_sub(1)?;
    let start = line_start(source, brace);
    let alone = source.get(start..brace).is_some_and(|before| before.trim().is_empty());
    // On its own line in nearly every file: land at the START of that line, so the inserted text
    // ends with a newline and the brace keeps the indentation it was written with.
    Some(if alone { start } else { brace })
}

/// Where a moved member goes at the end of a body, and what has to be written before it.
///
/// The `enum` is why this is not just [`append_point`]. An enum body whose constants are not
/// terminated by a `;` has no member section at all, and a method written straight after
/// `INCLUDE, EXCLUDE` is read as another constant — `',', '}', or ';' expected`, which is a
/// **syntax error**, the worst thing a refactoring can produce. So the `;` comes with it.
pub fn append_into(body: &Node<'_>, source: &str) -> Option<(usize, &'static str)> {
    if body.kind() != "enum_body" {
        return Some((append_point(body, source)?, ""));
    }
    let mut cursor = body.walk();
    let children: Vec<Node<'_>> = body.named_children(&mut cursor).collect();
    if children.iter().any(|c| c.kind() == "enum_body_declarations") {
        return Some((append_point(body, source)?, ""));
    }
    // Right after the last constant, so the `;` reads as the terminator it is — appended down by
    // the closing brace it would be a line of its own saying nothing.
    match children.iter().filter(|c| c.kind() == "enum_constant").last() {
        Some(last) => Some((last.end_byte(), ";")),
        None => Some((append_point(body, source)?, "")),
    }
}

/// Where a field goes in an enum: in the member section after the constants, never before them.
fn enum_insertion_point(body: &Node<'_>, source: &str) -> Option<usize> {
    let mut cursor = body.walk();
    let members = body.named_children(&mut cursor).find(|c| c.kind() == "enum_body_declarations")?;
    let mut inner = members.walk();
    let last_field =
        members.named_children(&mut inner).filter(|n| n.kind() == "field_declaration").last();
    let end = last_field.map(|f| f.end_byte()).unwrap_or_else(|| members.start_byte());
    Some(line_after(source, end))
}

/// The indentation members of this body are written with — read off the first one rather than
/// assumed, so a file indented with tabs or with two spaces keeps its own.
pub fn member_indent(source: &str, body: &Node<'_>) -> String {
    for child in members_of(body) {
        let indent = indent_at(source, child.start_byte());
        if !indent.is_empty() {
            return indent;
        }
    }
    // An **empty** body has nothing to read, and four spaces is a guess that is wrong in every file
    // written with two or with tabs — which is where the first member ever written into a class
    // lands crooked. So the step comes off the file itself.
    format!("{}{}", indent_at(source, body.start_byte()), indent_step(source))
}

/// The indentation step this file is written with — one level, whatever that is here.
///
/// Read off the first place the file indents: the first non-blank line more indented than the one
/// before it, and the difference between them. Tabs, two spaces and four all answer for themselves.
/// Four spaces when the file never indents at all, which is a file with nothing to copy.
pub fn indent_step(source: &str) -> String {
    let mut previous = "";
    for line in source.lines().filter(|l| !l.trim().is_empty()) {
        let indent = &line[..line.len() - line.trim_start().len()];
        if indent.len() > previous.len() && indent.starts_with(previous) {
            return indent[previous.len()..].to_string();
        }
        previous = indent;
    }
    "    ".to_string()
}

/// The member of a type body that contains `expr`, when the expression is NOT inside a method or
/// constructor — a field's initialiser, an initialiser block, an enum constant's arguments.
///
/// Inside a method there is nothing to sit above: a body may read a field declared anywhere in the
/// class, so what is lifted out can go with the other fields.
pub fn member_containing<'t>(body: &Node<'t>, expr: &Node<'t>) -> Option<Node<'t>> {
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

/// Re-indent a member's text for a new home: every line loses `from` and gains `to`.
///
/// Lines that do not start with `from` are left alone rather than mangled — a continuation line
/// aligned by hand, or a text block, whose relative shape is the only thing that survives a move
/// intact.
pub fn reindent(member: &str, from: &str, to: &str) -> String {
    if from == to {
        return member.to_string();
    }
    member
        .lines()
        .map(|line| match line.strip_prefix(from) {
            Some(rest) => format!("{to}{rest}"),
            None if line.trim().is_empty() => String::new(),
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every VARIABLE name declared inside `scope` — parameters, locals, fields.
///
/// Variables only, and not every identifier in sight: Java keeps methods and variables in separate
/// namespaces, so a local called `compute` beside a method `compute()` is legal and renaming it
/// away would be a suggestion nobody asked for.
pub fn declared_names(scope: Node<'_>, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    // `enum_constant` is in the list because an enum's constants ARE fields of it: an enum of SQL
    // types with a constant `TEXT` is where a constant named `TEXT` collides.
    for kind in [
        "variable_declarator",
        "formal_parameter",
        "catch_formal_parameter",
        "enhanced_for_statement",
        "enum_constant",
    ] {
        for node in descendants(scope, kind) {
            if let Some(name) = node.child_by_field_name("name") {
                out.push(text(&name, source).to_string());
            }
        }
    }
    out
}

/// `base`, or `base2`, `base3`… — the first spelling nothing else in scope has taken.
pub fn unique_name(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base}{i}"))
        .find(|candidate| !taken.iter().any(|t| t == candidate))
        .unwrap_or_else(|| base.to_string())
}

/// Every type declaration in the file, nested ones included, in source order.
pub fn types_in<'t>(root: Node<'t>) -> Vec<Node<'t>> {
    crate::selection::descendants_any(root, TYPE_DECLS)
}

/// The names a type declaration extends or implements, as the source writes them.
///
/// The written spelling and not a resolved one: this crate has no resolver, and the caller that
/// does needs the source's own words to look the type up with anyway.
pub fn supertypes_of(type_decl: &Node<'_>, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = type_decl.walk();
    for child in type_decl.named_children(&mut cursor) {
        match child.kind() {
            // `class B extends A` — the superclass is the `superclass` field, one type.
            "superclass" | "super_interfaces" | "extends_interfaces" | "interfaces" => {
                push_written_types(&child, source, &mut out);
            }
            _ => {}
        }
    }
    out
}

/// The type names under a clause, in source order — `extends` and `implements` nest them one level
/// deeper (`type_list`) and a generic supertype hides its name inside a `generic_type`.
fn push_written_types(node: &Node<'_>, source: &str, out: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "scoped_type_identifier" => {
                out.push(text(&child, source).to_string())
            }
            "generic_type" => {
                if let Some(base) = child.named_child(0) {
                    out.push(text(&base, source).to_string());
                }
            }
            _ => push_written_types(&child, source, out),
        }
    }
}

/// The bodies a member can be a direct child of — what tells an instance-initialiser `block` from
/// the `block` that is a method's body.
pub const TYPE_BODIES: &[&str] = &[
    "class_body",
    "interface_body",
    "enum_body",
    "enum_body_declarations",
    "annotation_type_body",
];

/// The simple name of a written type, with its type arguments and package dropped: `List<String>`
/// and `java.util.List` are both `List`.
pub fn simple_name(written: &str) -> &str {
    let base = written.split('<').next().unwrap_or(written).trim();
    base.rsplit('.').next().unwrap_or(base)
}

/// The type declaration in this file that goes by `name`, if any.
pub fn type_named<'t>(root: Node<'t>, source: &str, name: &str) -> Option<Node<'t>> {
    types_in(root).into_iter().find(|t| {
        t.child_by_field_name("name").map(|n| text(&n, source)) == Some(simple_name(name))
    })
}

/// The type declarations in this file that name `name` as a supertype.
pub fn subtypes_of<'t>(root: Node<'t>, source: &str, name: &str) -> Vec<Node<'t>> {
    types_in(root)
        .into_iter()
        .filter(|t| supertypes_of(t, source).iter().any(|s| simple_name(s) == name))
        .collect()
}

/// The type parameters a declaration introduces — `<T, K extends Comparable<K>>` gives `T` and `K`.
///
/// The names only. A member that mentions one of these cannot leave the thing that declared it: a
/// field written `private T value;` on a class with no `T` does not compile, and neither does a
/// method carrying its own `T` into a type that never heard of it.
pub fn type_parameters(node: &Node<'_>, source: &str) -> Vec<String> {
    let Some(list) = node.child_by_field_name("type_parameters") else { return Vec::new() };
    let mut cursor = list.walk();
    let parameters: Vec<Node<'_>> =
        list.named_children(&mut cursor).filter(|c| c.kind() == "type_parameter").collect();
    parameters
        .into_iter()
        .filter_map(|c| {
            let mut inner = c.walk();
            // Bound rather than returned: the iterator borrows `inner`, which dies with the closure.
            let found = c
                .named_children(&mut inner)
                .find(|n| n.kind() == "type_identifier")
                .map(|n| text(&n, source).to_string());
            found
        })
        .collect()
}

/// Whether any of `names` is written as a whole type name inside `node`.
pub fn mentions_type(node: &Node<'_>, source: &str, names: &[String]) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    crate::selection::descendants_any(*node, &["type_identifier"])
        .iter()
        .map(|n| text(n, source).to_string())
        .find(|written| names.contains(written))
}

/// Whether `node` carries `keyword` among its modifiers.
pub fn has_modifier(node: &Node<'_>, source: &str, keyword: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .any(|c| c.kind() == "modifiers" && text(&c, source).split_whitespace().any(|w| w == keyword));
    found
}

/// The `package` a source declares, as it declares it.
pub fn package_of(root: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = root.walk();
    let decl = root.named_children(&mut cursor).find(|c| c.kind() == "package_declaration")?;
    let name = decl.named_child(0)?;
    Some(text(&name, source).to_string())
}

/// Every `import` line of a file, as `(what it imports, the whole line)`.
pub fn imports_of(root: Node<'_>, source: &str) -> Vec<(String, String)> {
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .filter(|c| c.kind() == "import_declaration")
        .map(|c| {
            let whole = text(&c, source).to_string();
            let what = whole
                .trim_start_matches("import")
                .trim()
                .trim_start_matches("static")
                .trim()
                .trim_end_matches(';')
                .trim()
                .to_string();
            (what, whole)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::parse_java;

    #[test]
    fn a_new_member_goes_after_the_last_field() {
        let src = "class A {\n    int a;\n    int b;\n    void f() {}\n}";
        let tree = parse_java(src).unwrap();
        let ty = types_in(tree.root_node()).remove(0);
        let body = body_of(&ty).unwrap();
        assert_eq!(insertion_point(&body, src), Some(src.find("    void f").unwrap()));
        assert_eq!(member_indent(src, &body), "    ");
    }

    /// An enum's constants come first, so a field written "after the `{`" would land among them.
    #[test]
    fn a_field_in_an_enum_goes_after_the_constants() {
        let src = "enum E {\n    A, B;\n    int n;\n    void f() {}\n}";
        let tree = parse_java(src).unwrap();
        let ty = types_in(tree.root_node()).remove(0);
        let body = body_of(&ty).unwrap();
        let at = insertion_point(&body, src).unwrap();
        assert!(at > src.find("A, B;").unwrap(), "a field must not land among the constants");
    }

    #[test]
    fn a_moved_member_is_appended_before_the_closing_brace() {
        let src = "class A {\n    int a;\n}";
        let tree = parse_java(src).unwrap();
        let ty = types_in(tree.root_node()).remove(0);
        let body = body_of(&ty).unwrap();
        assert_eq!(append_point(&body, src), Some(src.rfind('}').unwrap()));
    }

    /// The brace keeps the indentation it was written with — a nested body's `}` sits at its own
    /// level, and landing after it put that brace at column zero. It still compiled, which is why
    /// only a test could say so.
    #[test]
    fn an_indented_closing_brace_keeps_its_indentation() {
        let src = "class Host {\n  static class Inner {\n  }\n}";
        let tree = parse_java(src).unwrap();
        let inner = types_in(tree.root_node()).remove(1);
        let body = body_of(&inner).unwrap();
        let at = append_point(&body, src).unwrap();
        assert_eq!(&src[at..at + 3], "  }");
    }

    /// A body written on one line has nothing to land at the start of, so it lands at the brace.
    /// The step is the file's own, not four spaces — a class written with two got its first
    /// member at six.
    #[test]
    fn the_indent_step_is_read_off_the_file() {
        assert_eq!(indent_step("class A {\n  int a;\n}"), "  ");
        assert_eq!(indent_step("class A {\n\tint a;\n}"), "\t");
        assert_eq!(indent_step("class A {\n    int a;\n}"), "    ");
        assert_eq!(indent_step("class A {}"), "    ");
    }

    /// An empty body reads the step from the file rather than assuming one.
    #[test]
    fn an_empty_body_indents_its_first_member_the_way_the_file_does() {
        let src = "class Host {\n  static class Inner {\n  }\n}";
        let tree = parse_java(src).unwrap();
        let inner = types_in(tree.root_node()).remove(1);
        let body = body_of(&inner).unwrap();
        assert_eq!(member_indent(src, &body), "    ");
    }

    #[test]
    fn a_one_line_body_appends_at_the_brace() {
        let src = "class A { int a; }";
        let tree = parse_java(src).unwrap();
        let ty = types_in(tree.root_node()).remove(0);
        let body = body_of(&ty).unwrap();
        assert_eq!(append_point(&body, src), Some(src.rfind('}').unwrap()));
    }

    #[test]
    fn supertypes_are_read_as_the_source_writes_them() {
        let src = "class B extends a.A implements Runnable, Cloneable {}";
        let tree = parse_java(src).unwrap();
        let ty = types_in(tree.root_node()).remove(0);
        assert_eq!(supertypes_of(&ty, src), vec!["a.A", "Runnable", "Cloneable"]);
        assert_eq!(simple_name("java.util.List<String>"), "List");
    }

    #[test]
    fn a_member_is_re_indented_for_its_new_home() {
        let member = "    void f() {\n        g();\n    }";
        assert_eq!(reindent(member, "    ", "        "), "        void f() {\n            g();\n        }");
    }

    #[test]
    fn the_types_of_a_file_include_the_nested_ones() {
        let src = "class A {\n    static class Inner {}\n}\nclass B extends A {}";
        let tree = parse_java(src).unwrap();
        let names: Vec<String> = types_in(tree.root_node())
            .iter()
            .filter_map(|t| t.child_by_field_name("name").map(|n| text(&n, src).to_string()))
            .collect();
        assert_eq!(names, vec!["A", "Inner", "B"]);
        assert_eq!(subtypes_of(tree.root_node(), src, "A").len(), 1);
        assert!(type_named(tree.root_node(), src, "Inner").is_some());
    }
}
