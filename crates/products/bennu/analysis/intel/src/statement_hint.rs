//! Ghost text for the continuations of a statement that are **facts** — the next `case` of a
//! `switch` over an enum.
//!
//! The same standard as every ghost proposal in Arbor (see `accessor_completion::generated_hint` and
//! the SQL ghost text): derived, never predicted, and absent is the fallback. Ghost text is drawn
//! inline where it reads like text that is already there, so being wrong costs trust rather than a
//! keystroke. Everything plausible-but-not-certain belongs in the popup, which is why each refusal
//! below is explicit.
//!
//! ## The next `case`
//!
//! ```java
//! switch (color) {
//!     case RED:
//!         paint();
//!         break;
//!     |                 ← ghost: case GREEN:
//! }
//! ```
//!
//! The constants of an enum are a closed list, in an order the declaration fixes, and the arms
//! already written say which are covered. The proposal is the first constant no arm names. It is
//! made only when all of this holds:
//!
//! * the caret is on a **blank line** directly inside a `switch` body and **after every arm**;
//! * there is at least one arm, so the style (`case X:` or `case X ->`) is copied from the code and
//!   not chosen — an empty switch is silent;
//! * the last arm is **finished**: an arrow arm always is, a colon arm only when it ends in
//!   `break`, `return`, `throw`, `continue` or `yield`. A bare `case RED:` is the first half of a
//!   grouped `case RED: case GREEN:`, and a body still being written is not an invitation;
//! * every label is a plain constant name — no `default` (a `default` ends the discussion), no
//!   pattern, no `null`, no qualified name, no label the enum does not declare;
//! * the selector's type resolves to an enum whose constants are fully known, and one remains.
//!
//! The enum's constants are the static final fields typed as the enum itself — how the member model
//! represents them, and how the exhaustiveness check in `bennu-check` reads them too. A project enum
//! that also declares a `public static final` alias of its own type reads as one more constant; the
//! same limitation that check lives with.

use bennu_java::prelude::{extract_symbols, infer_node_type_cached, parse_java, InferCache, TypeResolver};
use tree_sitter::Node;

use crate::accessor_completion::AccessorHint;

/// How the arms of a switch are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    /// `case RED:` followed by statements.
    Colon,
    /// `case RED -> …`.
    Arrow,
}

/// What the arms already written say.
struct Arms {
    style: Style,
    /// The constants some label names.
    covered: Vec<String>,
    /// The indentation the last arm is written at — where the next one goes.
    indent: String,
}

/// The `case` the caret is certainly about to write, or `None` — the ordinary answer. See the module
/// docs for every condition.
pub fn next_case_hint(source: &str, offset: usize, resolver: &dyn TypeResolver) -> Option<AccessorHint> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    let line_start = blank_line_start(source, offset)?;
    let tree = parse_java(source)?;
    let root = tree.root_node();
    let body = switch_body_at(root, offset)?;
    let switch = body.parent().filter(|p| p.kind() == "switch_expression" && !p.has_error())?;
    let arms = read_arms(body, offset, source)?;

    let condition = switch.child_by_field_name("condition")?;
    let symbols = extract_symbols(source);
    let selector = infer_node_type_cached(&root, source, &symbols, &condition, resolver, &InferCache::new())?;
    let constants = enum_constants(resolver, &selector)?;
    if arms.covered.iter().any(|name| !constants.contains(name)) {
        return None;
    }
    let next = constants.iter().find(|name| !arms.covered.contains(name))?;
    let text = match arms.style {
        Style::Colon => format!("case {next}:"),
        Style::Arrow => format!("case {next} ->"),
    };
    Some(placed(source, line_start, offset, &arms.indent, text))
}

/// The start of the caret's line, when the line is blank apart from the caret: nothing but spaces
/// and tabs before it, nothing at all after it.
fn blank_line_start(source: &str, offset: usize) -> Option<usize> {
    let line_start = source[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[offset..].find('\n').map_or(source.len(), |i| offset + i);
    let before_is_blank = source[line_start..offset].bytes().all(|b| b == b' ' || b == b'\t');
    (before_is_blank && source[offset..line_end].trim().is_empty()).then_some(line_start)
}

/// The `switch_block` the caret is directly inside — not a block nested in one of its arms.
///
/// The smallest node spanning a caret in whitespace is the node whose children the whitespace sits
/// between, so a caret between two arms answers the switch body, and one inside an arm's own block
/// answers that block (and is refused).
fn switch_body_at(root: Node<'_>, offset: usize) -> Option<Node<'_>> {
    let node = root.named_descendant_for_byte_range(offset, offset)?;
    (node.kind() == "switch_block" && node.start_byte() < offset && offset < node.end_byte()).then_some(node)
}

/// Read every arm of the switch body, or refuse — see the module docs for what is refused.
fn read_arms(body: Node<'_>, offset: usize, source: &str) -> Option<Arms> {
    let mut cursor = body.walk();
    let items: Vec<Node<'_>> = body
        .named_children(&mut cursor)
        .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
        .collect();
    let last = *items.last()?;
    // Caret after every arm: a blank line between two arms is not where the next constant goes.
    if last.end_byte() > offset {
        return None;
    }
    let style = match last.kind() {
        "switch_rule" => Style::Arrow,
        "switch_block_statement_group" => Style::Colon,
        _ => return None,
    };
    let mut covered = Vec::new();
    for item in &items {
        if item.kind() != last.kind() {
            return None;
        }
        read_labels(*item, source, &mut covered)?;
    }
    if style == Style::Colon && !terminates_last_statement(last) {
        return None;
    }
    Some(Arms { style, covered, indent: indent_of_line(source, last.start_byte()) })
}

/// Collect the constant names the labels of one arm name; `None` for anything that is not a plain
/// constant — `default` has no named child at all, so it lands here too.
fn read_labels(item: Node<'_>, source: &str, covered: &mut Vec<String>) -> Option<()> {
    let mut cursor = item.walk();
    for label in item.named_children(&mut cursor).filter(|n| n.kind() == "switch_label") {
        let mut inner = label.walk();
        let mut named = 0;
        for part in label.named_children(&mut inner) {
            if matches!(part.kind(), "line_comment" | "block_comment") {
                continue;
            }
            if part.kind() != "identifier" {
                return None;
            }
            covered.push(part.utf8_text(source.as_bytes()).ok()?.to_string());
            named += 1;
        }
        if named == 0 {
            return None;
        }
    }
    Some(())
}

/// Whether a colon arm ends in a jump, so nothing falls through into what comes next.
fn terminates_last_statement(group: Node<'_>) -> bool {
    fn last_statement(node: Node<'_>) -> Option<Node<'_>> {
        let mut cursor = node.walk();
        let found = node
            .named_children(&mut cursor)
            .filter(|n| !matches!(n.kind(), "switch_label" | "line_comment" | "block_comment"))
            .last();
        found
    }
    let mut current = last_statement(group);
    while let Some(statement) = current {
        match statement.kind() {
            "break_statement" | "return_statement" | "throw_statement" | "continue_statement"
            | "yield_statement" => return true,
            // `case RED: { …; break; }` — judged by what ends the block.
            "block" => current = last_statement(statement),
            _ => return false,
        }
    }
    false
}

/// The constants of `selector`'s type if it is an enum, in declaration order — `None` for anything
/// else, and for an enum whose constants are not visible (an empty list would fabricate a "next").
fn enum_constants(resolver: &dyn TypeResolver, selector: &bennu_java::prelude::TypeRef) -> Option<Vec<String>> {
    if selector.dims != 0 {
        return None;
    }
    let members = resolver.members_of(&selector.binary_name)?;
    if !members.flags.is_enum {
        return None;
    }
    let constants: Vec<String> = members
        .fields
        .iter()
        .filter(|f| f.is_static && f.is_final && f.return_type.dims == 0 && f.return_type.binary_name == selector.binary_name)
        .map(|f| f.name.clone())
        .collect();
    (!constants.is_empty()).then_some(constants)
}

/// The whitespace the line holding `pos` starts with.
fn indent_of_line(source: &str, pos: usize) -> String {
    let line_start = source[..pos].rfind('\n').map_or(0, |i| i + 1);
    source[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// The hint for `text` on the blank line `line_start..offset`, drawn where the arm will stand.
///
/// When the line already holds (part of) the arm's indentation, the missing part is drawn and
/// written with the text, so a caret at column 0 on an empty line still produces an aligned arm.
/// When it holds something else — a different indent — the line is left alone and only the text is
/// written.
fn placed(source: &str, line_start: usize, offset: usize, wanted_indent: &str, text: String) -> AccessorHint {
    let present = &source[line_start..offset];
    match wanted_indent.strip_prefix(present) {
        Some(missing) if !missing.is_empty() => AccessorHint {
            preview: format!("{missing}{text}"),
            insert: format!("{wanted_indent}{text}"),
            replace_start: line_start,
            replace_end: offset,
        },
        _ => AccessorHint { preview: text.clone(), insert: text, replace_start: offset, replace_end: offset },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};

    use super::*;

    /// A resolver that knows one enum, `shop.Color { RED, GREEN, BLUE }`.
    struct Colors;

    impl TypeResolver for Colors {
        fn members_of(&self, binary_name: &str) -> Option<Arc<ClassMembers>> {
            if binary_name != "shop/Color" {
                return None;
            }
            Some(Arc::new(ClassMembers {
                superclass: None,
                interfaces: Vec::new(),
                methods: Vec::new(),
                fields: ["RED", "GREEN", "BLUE"]
                    .iter()
                    .map(|name| Member::field(*name, TypeRef::simple("shop/Color")).stat().final_())
                    .collect(),
                flags: ClassFlags { is_enum: true, ..Default::default() },
                type_params: Vec::new(),
            }))
        }

        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            (name == "Color").then(|| "shop/Color".to_string())
        }
    }

    /// The hint at the `¶` marker in `marked`, which is removed from the source first.
    fn hint(marked: &str) -> Option<AccessorHint> {
        let offset = marked.find('¶').expect("a caret marker");
        let source = marked.replacen('¶', "", 1);
        next_case_hint(&source, offset, &Colors)
    }

    /// A switch in a method, the arms and the caret line supplied.
    fn switch(arms: &str) -> String {
        format!(
            "class A {{\n    void m(Color c) {{\n        switch (c) {{\n{arms}\n        }}\n    }}\n}}\n"
        )
    }

    const RED_COLON: &str = "            case RED:\n                run();\n                break;\n";
    const RED_ARROW: &str = "            case RED -> run();\n";

    #[test]
    fn a_finished_colon_arm_is_followed_by_the_next_constant() {
        let h = hint(&switch(&format!("{RED_COLON}            ¶"))).expect("a hint");
        assert_eq!(h.preview, "case GREEN:");
        assert_eq!(h.insert, "case GREEN:");
        assert_eq!(h.replace_start, h.replace_end, "an insertion at the caret");
    }

    #[test]
    fn the_arrow_style_is_copied_from_the_arms_already_written() {
        let h = hint(&switch(&format!("{RED_ARROW}            ¶"))).expect("a hint");
        assert_eq!(h.insert, "case GREEN ->");
    }

    #[test]
    fn the_first_constant_no_arm_names_is_the_one_proposed() {
        let red_blue = "            case RED -> a();\n            case BLUE -> b();\n            ¶";
        assert_eq!(hint(&switch(red_blue)).expect("a hint").insert, "case GREEN ->");
        let red_green = "            case RED -> a();\n            case GREEN -> b();\n            ¶";
        assert_eq!(hint(&switch(red_green)).expect("a hint").insert, "case BLUE ->");
    }

    /// `case RED, GREEN ->` covers both.
    #[test]
    fn a_label_listing_several_constants_covers_each() {
        let arms = "            case RED, GREEN -> a();\n            ¶";
        assert_eq!(hint(&switch(arms)).expect("a hint").insert, "case BLUE ->");
    }

    #[test]
    fn a_block_arm_that_ends_in_a_jump_is_finished() {
        let arms = "            case RED: {\n                run();\n                break;\n            }\n            ¶";
        assert_eq!(hint(&switch(arms)).expect("a hint").insert, "case GREEN:");
    }

    /// A caret at column 0 on an empty line still produces an aligned arm: the indentation is
    /// drawn and written with it, replacing the (empty) start of the line.
    #[test]
    fn a_missing_indentation_is_written_with_the_arm() {
        let marked = switch(&format!("{RED_COLON}¶"));
        let offset = marked.find('¶').unwrap();
        let h = hint(&marked).expect("a hint");
        assert_eq!(h.preview, "            case GREEN:");
        assert_eq!(h.insert, "            case GREEN:");
        assert_eq!((h.replace_start, h.replace_end), (offset, offset));
    }

    #[test]
    fn a_partly_indented_line_is_completed() {
        let marked = switch(&format!("{RED_COLON}    ¶"));
        let offset = marked.find('¶').unwrap();
        let h = hint(&marked).expect("a hint");
        assert_eq!(h.preview, "        case GREEN:");
        assert_eq!(h.insert, "            case GREEN:");
        assert_eq!((h.replace_start, h.replace_end), (offset - 4, offset));
    }

    /// Indented deeper than the arms: left alone rather than rewritten.
    #[test]
    fn a_line_indented_differently_is_not_rewritten() {
        let marked = switch(&format!("{RED_COLON}                    ¶"));
        let offset = marked.find('¶').unwrap();
        let h = hint(&marked).expect("a hint");
        assert_eq!(h.insert, "case GREEN:");
        assert_eq!((h.replace_start, h.replace_end), (offset, offset));
    }

    #[test]
    fn nothing_when_every_constant_is_covered() {
        let arms = "            case RED -> a();\n            case GREEN -> b();\n            case BLUE -> c();\n            ¶";
        assert_eq!(hint(&switch(arms)).map(|h| h.insert), None);
    }

    #[test]
    fn nothing_when_a_default_is_there() {
        let arms = "            case RED -> a();\n            default -> b();\n            ¶";
        assert_eq!(hint(&switch(arms)).map(|h| h.insert), None);
    }

    /// `case RED: case GREEN:` grouping is common; a bare label says nothing about what follows.
    #[test]
    fn nothing_after_an_arm_that_is_not_finished() {
        assert!(hint(&switch("            case RED:\n            ¶")).is_none(), "a bare label");
        assert!(hint(&switch("            case RED:\n                run();\n            ¶")).is_none(), "falls through");
    }

    #[test]
    fn nothing_in_an_empty_switch() {
        assert!(hint(&switch("            ¶")).is_none());
    }

    #[test]
    fn nothing_unless_the_line_is_blank() {
        assert!(hint(&switch(&format!("{RED_ARROW}            x¶"))).is_none(), "text typed");
        assert!(hint(&switch(&format!("{RED_ARROW}            ¶x"))).is_none(), "text after the caret");
        assert!(hint(&switch(&format!("{RED_ARROW}            case G¶"))).is_none(), "a case being written");
    }

    #[test]
    fn nothing_when_arms_follow_the_caret() {
        let arms = "            case RED -> a();\n            ¶\n            case BLUE -> b();";
        assert!(hint(&switch(arms)).is_none());
    }

    #[test]
    fn nothing_inside_a_block_nested_in_an_arm() {
        let arms = "            case RED -> {\n                ¶\n            }";
        assert!(hint(&switch(arms)).is_none());
    }

    #[test]
    fn nothing_for_labels_that_are_not_plain_constants() {
        let qualified = "            case Color.RED -> a();\n            ¶";
        assert!(hint(&switch(qualified)).is_none(), "qualified");
        let foreign = "            case PURPLE -> a();\n            ¶";
        assert!(hint(&switch(foreign)).is_none(), "not a constant of the enum");
        let null = "            case null -> a();\n            ¶";
        assert!(hint(&switch(null)).is_none(), "null");
    }

    #[test]
    fn nothing_when_the_selector_is_not_a_known_enum() {
        let unknown = "class A {\n    void m(Mystery c) {\n        switch (c) {\n            case RED -> a();\n            ¶\n        }\n    }\n}\n";
        assert!(hint(unknown).is_none(), "unresolved type");
        let number = "class A {\n    void m(int c) {\n        switch (c) {\n            case 1 -> a();\n            ¶\n        }\n    }\n}\n";
        assert!(hint(number).is_none(), "an int");
    }

    #[test]
    fn nothing_when_the_switch_does_not_parse() {
        let arms = "            case RED -> run(\n            ¶";
        assert!(hint(&switch(arms)).is_none());
    }

    #[test]
    fn a_caret_past_the_end_or_inside_a_character_is_refused() {
        assert!(next_case_hint("switch", 99, &Colors).is_none());
        assert!(next_case_hint("é", 1, &Colors).is_none());
    }

    #[test]
    fn the_blank_line_test_reads_only_the_carets_own_line() {
        assert_eq!(blank_line_start("a\n    \nb", 6), Some(2));
        assert_eq!(blank_line_start("a\n  x \nb", 5), None);
        assert_eq!(blank_line_start("a\n    x\nb", 6), None);
        // A Windows line ending after the caret is whitespace, not text.
        assert_eq!(blank_line_start("a\r\n    \r\nb", 7), Some(3));
    }
}
