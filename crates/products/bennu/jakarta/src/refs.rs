//! Finding constraints in a Java source, and reading their messages.
//!
//! ## Why this parses instead of scanning
//!
//! A constraint's message is a Java string that routinely contains braces and commas — `@Size(min =
//! 2, max = 30, message = "{order.name.length}")` — inside an argument list that may hold arrays,
//! nested annotations and class literals. Finding where that argument list ends is a parser's job.
//! And the second question this has to answer, *what type is the thing being constrained*, is not
//! in the annotation at all: it is a sibling node. Both are cheap with a tree and guesswork
//! without one.
//!
//! ## The one rule everything else follows from
//!
//! In a Bean Validation message, `{name}` is resolved **against the constraint's own attributes
//! first**, and only then against a bundle. `${…}` is not a bundle reference at all — it is an
//! expression evaluated at interpolation time. So:
//!
//! | written | what it is |
//! |---|---|
//! | `{min}` on a `@Size` | the constraint's `min` attribute |
//! | `{order.name.length}` | a bundle key |
//! | `${validatedValue}` | an EL expression |
//! | `\{literal\}` | escaped braces, printed as-is |
//!
//! A check that read every `{…}` as a key would report `min` and `max` missing on every custom
//! `@Size` message in the project. That is why [`crate::constraints::Constraint::attributes`] is
//! part of the table rather than documentation.

use tree_sitter::Node;

use crate::constraints::{constraint, Constraint};

/// One constraint annotation, where it is, and what it is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintUse {
    /// Simple name as written (`"NotNull"`).
    pub name: String,
    /// Byte span of the annotation, `@` included — what a diagnostic underlines.
    pub start: usize,
    pub end: usize,
    /// The `message` attribute, when the annotation writes one.
    pub message: Option<MessageLiteral>,
    /// The written type of what is being constrained (`"String"`, `"List<Order>"`, `"int"`), when
    /// it is on something with one.
    pub target_type: Option<String>,
    /// The name of what is being constrained.
    pub target_name: Option<String>,
    /// The type that declares it.
    pub owner: Option<String>,
}

/// A `message = "…"` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageLiteral {
    /// The text between the quotes, as written.
    pub text: String,
    /// Byte offset of that text — inside the opening quote.
    pub start: usize,
    pub end: usize,
}

/// One bundle key named inside a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRef {
    pub key: String,
    /// Byte offset of the key text, inside the braces.
    pub start: usize,
    pub end: usize,
}

/// Every constraint annotation in the file, in source order.
pub fn constraints_in(root: Node<'_>, source: &str) -> Vec<ConstraintUse> {
    let mut out = Vec::new();
    walk(root, source, &mut out);
    out.sort_by_key(|c| c.start);
    out
}

fn walk(node: Node<'_>, source: &str, out: &mut Vec<ConstraintUse>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if matches!(child.kind(), "annotation" | "marker_annotation") {
            if let Some(found) = read_annotation(child, source) {
                out.push(found);
            }
        }
        walk(child, source, out);
    }
}

fn read_annotation(node: Node<'_>, source: &str) -> Option<ConstraintUse> {
    let name_node = node.child_by_field_name("name")?;
    // `@jakarta.validation.constraints.NotNull` is legal and rare; the simple name is the last
    // segment either way, and it is what the table is keyed by.
    let written = text(&name_node, source);
    let name = written.rsplit('.').next().unwrap_or(written).to_string();
    let c = constraint(&name)?;

    let (target_type, target_name, owner) = target_of(node, source);
    Some(ConstraintUse {
        message: message_of(node, source, c),
        name,
        start: node.start_byte(),
        end: node.end_byte(),
        target_type,
        target_name,
        owner,
    })
}

/// The `message = "…"` of this annotation.
///
/// **Only** that spelling. Java's single-element shorthand (`@Max(5)`) sets the annotation's
/// `value` element, and no constraint declares `message` as its `value` — so a bare literal in a
/// constraint is `@Max(5)`, `@DecimalMin("0.0")` or nothing legal at all, and never a message.
/// Reading one as a message would report a regular expression as a missing bundle key.
fn message_of(node: Node<'_>, source: &str, _c: &Constraint) -> Option<MessageLiteral> {
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if child.kind() != "element_value_pair" {
            continue;
        }
        let Some(key) = child.child_by_field_name("key") else { continue };
        if text(&key, source) == "message" {
            return child.child_by_field_name("value").and_then(|v| literal(v, source));
        }
    }
    None
}

/// The text of a string literal node, without its quotes. `None` for anything that is not a plain
/// one — a text block, a concatenation, a constant reference — because nothing certain can be said
/// about a message this cannot read.
fn literal(node: Node<'_>, source: &str) -> Option<MessageLiteral> {
    if node.kind() != "string_literal" {
        return None;
    }
    let raw = text(&node, source);
    if raw.starts_with("\"\"\"") || !raw.starts_with('"') || raw.len() < 2 || !raw.ends_with('"') {
        return None;
    }
    Some(MessageLiteral {
        text: raw[1..raw.len() - 1].to_string(),
        start: node.start_byte() + 1,
        end: node.end_byte() - 1,
    })
}

/// What the annotation is attached to: its written type, its name, and the type declaring it.
fn target_of(node: Node<'_>, source: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut owner = None;
    let mut member: Option<Node<'_>> = None;
    let mut at = node.parent();
    while let Some(current) = at {
        match current.kind() {
            "field_declaration" | "formal_parameter" | "method_declaration" | "record_component"
            | "local_variable_declaration"
                if member.is_none() =>
            {
                member = Some(current);
            }
            "class_declaration" | "record_declaration" | "interface_declaration"
            | "enum_declaration" => {
                owner = current.child_by_field_name("name").map(|n| text(&n, source).to_string());
                break;
            }
            _ => {}
        }
        at = current.parent();
    }
    let Some(member) = member else { return (None, None, owner) };
    let written_type =
        member.child_by_field_name("type").map(|n| text(&n, source).trim().to_string());
    let member_name = member
        .child_by_field_name("name")
        .or_else(|| {
            // A field is `type declarator`, and the declarator carries the name.
            member.child_by_field_name("declarator").and_then(|d| d.child_by_field_name("name"))
        })
        .map(|n| text(&n, source).to_string());
    (written_type, member_name, owner)
}

/// The bundle keys named inside a message.
///
/// See the module docs for the four things a brace run can be; only one of them is a key.
pub fn keys_in_message(message: &MessageLiteral, c: &Constraint) -> Vec<KeyRef> {
    let bytes = message.text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            // In a Java string literal the escape is written `\\{`, which reaches us as `\{`.
            b'\\' => {
                i += 2;
                continue;
            }
            // `${…}` is an expression, evaluated rather than looked up.
            b'$' if bytes.get(i + 1) == Some(&b'{') => {
                i = close_of(bytes, i + 1).map(|c| c + 1).unwrap_or(bytes.len());
                continue;
            }
            b'{' => {
                let Some(close) = close_of(bytes, i) else { break };
                let inner = &message.text[i + 1..close];
                if !inner.is_empty() && !c.interpolates(inner) {
                    out.push(KeyRef {
                        key: inner.to_string(),
                        start: message.start + i + 1,
                        end: message.start + close,
                    });
                }
                i = close + 1;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// The `}` closing the `{` at `open`. Bean Validation does not nest brace runs, so this is the
/// next unescaped `}`.
fn close_of(bytes: &[u8], open: usize) -> Option<usize> {
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'}' => return Some(i),
            // A newline inside a message parameter means the brace was never a parameter.
            b'\n' => return None,
            _ => {}
        }
        i += 1;
    }
    None
}

/// The constraint use whose **message** the caret sits in, with the key it is on (if any).
pub fn at_offset(
    uses: &[ConstraintUse],
    offset: usize,
) -> Option<(&ConstraintUse, &'static Constraint)> {
    uses.iter().find_map(|u| {
        let m = u.message.as_ref()?;
        if offset < m.start || offset > m.end {
            return None;
        }
        Some((u, constraint(&u.name)?))
    })
}

/// The part of a bundle key already typed at `offset`, and the span a candidate replaces.
///
/// Scans the message text rather than reusing [`keys_in_message`], because the key being completed
/// is half-written: `"{order.` has no closing brace, and `"{}"` — the moment the popup is most
/// wanted — holds no key at all.
pub fn key_prefix_at(
    uses: &[ConstraintUse],
    offset: usize,
) -> Option<(String, usize, usize)> {
    let (used, _) = at_offset(uses, offset)?;
    let m = used.message.as_ref()?;
    let bytes = m.text.as_bytes();
    let local = offset.checked_sub(m.start)?;
    if local > bytes.len() {
        return None;
    }
    // Back to the `{` that opens the run the caret is in — stopping at a `}`, which would mean the
    // caret is after a completed run rather than inside one.
    let mut i = local;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b'}' => return None,
            b'{' => {
                // `${` is an expression, and completing bundle keys into one would be wrong.
                if i > 0 && bytes[i - 1] == b'$' {
                    return None;
                }
                let typed = m.text.get(i + 1..local)?.to_string();
                // The run may already be closed ahead of the caret; a candidate replaces the whole
                // of what is there, not just what is behind the caret.
                let mut end = local;
                while end < bytes.len() && bytes[end] != b'}' && bytes[end] != b'{' {
                    end += 1;
                }
                return Some((typed, m.start + i + 1, m.start + end));
            }
            _ => {}
        }
    }
    None
}

fn text<'a>(node: &Node<'_>, source: &'a str) -> &'a str {
    source.get(node.start_byte()..node.end_byte()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"package com.acme;

import jakarta.validation.constraints.*;

public class Order {
    @NotNull
    private OrderId id;

    @NotBlank(message = "{order.name.required}")
    @Size(min = 2, max = 30, message = "between {min} and {max} characters")
    private String name;

    @Min(value = 1, message = "${validatedValue} is too small")
    private int quantity;

    @NotBlank
    private int brokenlyConstrained;

    public Order(@NotNull String name) {
        this.name = name;
    }
}
"#;

    fn uses() -> Vec<ConstraintUse> {
        let tree = bennu_java::prelude::parse_java(SRC).expect("parses");
        constraints_in(tree.root_node(), SRC)
    }

    #[test]
    fn every_constraint_is_found_with_what_it_is_on() {
        let found = uses();
        let names: Vec<&str> = found.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, ["NotNull", "NotBlank", "Size", "Min", "NotBlank", "NotNull"]);

        let size = found.iter().find(|u| u.name == "Size").unwrap();
        assert_eq!(size.target_type.as_deref(), Some("String"));
        assert_eq!(size.target_name.as_deref(), Some("name"));
        assert_eq!(size.owner.as_deref(), Some("Order"));
    }

    #[test]
    fn a_constraint_on_a_parameter_finds_the_parameters_type() {
        let found = uses();
        let on_param = found.last().unwrap();
        assert_eq!(on_param.name, "NotNull");
        assert_eq!(on_param.target_type.as_deref(), Some("String"));
    }

    /// The rule the whole check rests on: `{min}` is the constraint's attribute, and only the
    /// dotted name is a bundle key.
    #[test]
    fn a_constraints_own_attribute_is_not_a_bundle_key() {
        let found = uses();
        let size = found.iter().find(|u| u.name == "Size").unwrap();
        let c = constraint("Size").unwrap();
        let keys = keys_in_message(size.message.as_ref().unwrap(), c);
        assert!(keys.is_empty(), "min and max are attributes, not keys: {keys:?}");
    }

    #[test]
    fn a_dotted_name_in_braces_is_a_key_and_its_span_is_the_key_itself() {
        let found = uses();
        let blank = found.iter().find(|u| u.message.is_some() && u.name == "NotBlank").unwrap();
        let c = constraint("NotBlank").unwrap();
        let keys = keys_in_message(blank.message.as_ref().unwrap(), c);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key, "order.name.required");
        assert_eq!(&SRC[keys[0].start..keys[0].end], "order.name.required");
    }

    /// `${…}` is evaluated, not looked up — offering it as a missing key would flag the single
    /// most common thing people put in a custom message.
    #[test]
    fn an_expression_is_not_a_key() {
        let found = uses();
        let min = found.iter().find(|u| u.name == "Min").unwrap();
        let c = constraint("Min").unwrap();
        assert!(keys_in_message(min.message.as_ref().unwrap(), c).is_empty());
    }

    #[test]
    fn escaped_braces_are_text() {
        let m = MessageLiteral { text: r"a \{literal\} brace".into(), start: 0, end: 19 };
        let c = constraint("NotNull").unwrap();
        assert!(keys_in_message(&m, c).is_empty());
    }

    /// The single-element shorthand sets `value`, and no constraint declares `message` as its
    /// `value` — so a bare literal is never a message. `@DecimalMin("0.0")` is a bound, and
    /// reading it as a message would put a check on a number.
    #[test]
    fn a_bare_literal_is_never_a_message() {
        let src = "class A { @DecimalMin(\"0.0\") private String a; }";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let found = constraints_in(tree.root_node(), src);
        assert_eq!(found[0].message, None, "that is the bound");
    }

    /// And the spelling that IS a message keeps working beside other attributes.
    #[test]
    fn a_named_message_is_found_among_the_other_attributes() {
        let src = "class A { @Size(min = 1, message = \"{k}\", max = 9) private String a; }";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let found = constraints_in(tree.root_node(), src);
        assert_eq!(found[0].message.as_ref().unwrap().text, "{k}");
    }

    #[test]
    fn a_message_that_is_not_a_plain_literal_is_left_alone() {
        let src = "class A { @NotNull(message = MESSAGES.REQUIRED) private String a; }";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let found = constraints_in(tree.root_node(), src);
        assert_eq!(found[0].message, None);
    }

    // ── completion ───────────────────────────────────────────────────────────

    #[test]
    fn a_half_typed_key_is_offered_its_prefix_and_the_span_to_replace() {
        let src = "class A { @NotBlank(message = \"{order.na}\") private String a; }";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let found = constraints_in(tree.root_node(), src);
        let caret = src.find("order.na").unwrap() + "order.na".len();
        let (typed, start, end) = key_prefix_at(&found, caret).unwrap();
        assert_eq!(typed, "order.na");
        // The whole run is replaced, so a candidate does not land beside the tail already there.
        assert_eq!(&src[start..end], "order.na");
    }

    #[test]
    fn an_empty_run_is_where_the_popup_is_most_wanted() {
        let src = "class A { @NotBlank(message = \"{}\") private String a; }";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let found = constraints_in(tree.root_node(), src);
        let caret = src.find("{}").unwrap() + 1;
        let (typed, _, _) = key_prefix_at(&found, caret).unwrap();
        assert_eq!(typed, "");
    }

    #[test]
    fn nothing_is_offered_outside_a_brace_run_or_inside_an_expression() {
        let src = "class A { @NotBlank(message = \"plain ${x}\") private String a; }";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let found = constraints_in(tree.root_node(), src);
        assert_eq!(key_prefix_at(&found, src.find("plain").unwrap() + 3), None);
        assert_eq!(key_prefix_at(&found, src.find("${x").unwrap() + 3), None);
    }
}
