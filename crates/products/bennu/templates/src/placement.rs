//! Adding generated members to a class that already exists — nested or not — at the indentation the
//! class's own members have.

use bennu_java::prelude::parse_java;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

/// A class a file declares, for choosing where members go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeInFile {
    pub name: String,
    /// `Outer.Inner`.
    pub path: String,
    /// `0` for a top-level class.
    pub depth: usize,
}

pub fn types_in(source: &str) -> Vec<TypeInFile> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    collect(tree.root_node(), source, &mut Vec::new(), &mut out);
    out
}

fn is_class(node: &Node<'_>) -> bool {
    matches!(node.kind(), "class_declaration" | "record_declaration" | "enum_declaration")
}

fn collect(node: Node<'_>, source: &str, chain: &mut Vec<String>, out: &mut Vec<TypeInFile>) {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
    for child in children {
        if is_class(&child) {
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .unwrap_or_default()
                .to_string();
            chain.push(name.clone());
            out.push(TypeInFile { name, path: chain.join("."), depth: chain.len() - 1 });
            collect(child, source, chain, out);
            chain.pop();
        } else {
            collect(child, source, chain, out);
        }
    }
}

fn find_path<'t>(node: Node<'t>, source: &str, chain: &mut Vec<String>, path: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let children: Vec<Node<'t>> = node.named_children(&mut cursor).collect();
    for child in children {
        if is_class(&child) {
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .unwrap_or_default()
                .to_string();
            chain.push(name);
            if chain.join(".") == path {
                return Some(child);
            }
            let found = find_path(child, source, chain, path);
            chain.pop();
            if found.is_some() {
                return found;
            }
        } else if let Some(found) = find_path(child, source, chain, path) {
            return Some(found);
        }
    }
    None
}

/// Text to insert at a byte offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Insertion {
    pub offset: usize,
    pub text: String,
}

impl Insertion {
    pub fn apply(&self, source: &str) -> String {
        let mut out = String::with_capacity(source.len() + self.text.len());
        out.push_str(&source[..self.offset]);
        out.push_str(&self.text);
        out.push_str(&source[self.offset..]);
        out
    }
}

/// Where `members` go in the class at `class_path` (`Outer.Inner`), or in the first class of the file
/// when `None`: just before its closing brace, re-indented to the class's own member indentation.
///
/// `members` may be written at any indentation; their common leading whitespace is replaced.
pub fn insert_members(source: &str, class_path: Option<&str>, members: &str) -> Result<Insertion, String> {
    let tree = parse_java(source).ok_or("The test file does not parse")?;
    let root = tree.root_node();
    let decl = match class_path {
        Some(path) => find_path(root, source, &mut Vec::new(), path)
            .ok_or_else(|| format!("The test file declares no class `{path}`"))?,
        None => {
            let mut cursor = root.walk();
            let first = root.named_children(&mut cursor).find(is_class);
            first.ok_or("The test file declares no class")?
        }
    };
    let body = decl.child_by_field_name("body").ok_or("The class has no body")?;
    let closing = body.end_byte().saturating_sub(1);
    if source.as_bytes().get(closing) != Some(&b'}') {
        return Err("The class body is not closed".to_string());
    }
    let brace_line = source[..closing].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let before_brace = &source[brace_line..closing];
    let alone = before_brace.trim().is_empty();
    let closing_indent = match alone {
        true => before_brace.to_string(),
        false => indentation_of_line(source, decl.start_byte()),
    };
    let unit = member_indent_unit(source, body, &closing_indent);
    let block = reindent(members, &format!("{closing_indent}{unit}"));
    Ok(match alone {
        true => Insertion { offset: brace_line, text: format!("\n{block}\n") },
        false => Insertion { offset: closing, text: format!("\n{block}\n{closing_indent}") },
    })
}

fn indentation_of_line(source: &str, at: usize) -> String {
    let start = source[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    source[start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// How far a member sits inside its class: the first member's indentation beyond the class's, or four
/// spaces for a class with no members to learn from.
fn member_indent_unit(source: &str, body: Node<'_>, closing_indent: &str) -> String {
    let mut cursor = body.walk();
    let first = body.named_children(&mut cursor).find(|n| !n.kind().ends_with("comment"));
    first
        .map(|member| indentation_of_line(source, member.start_byte()))
        .and_then(|indent| indent.strip_prefix(closing_indent).map(str::to_string))
        .filter(|unit| !unit.is_empty())
        .unwrap_or_else(|| "    ".to_string())
}

fn reindent(text: &str, indent: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let common = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = lines
        .iter()
        .map(|l| match l.trim().is_empty() {
            true => String::new(),
            false => format!("{indent}{}", &l[common..]),
        })
        .collect();
    while out.first().is_some_and(String::is_empty) {
        out.remove(0);
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn members_land_before_the_closing_brace_at_the_class_indentation() {
        let src = "class A {\n    void existing() {}\n}\n";
        let insertion = insert_members(src, None, "void added() {}\n").unwrap();
        assert_eq!(insertion.apply(src), "class A {\n    void existing() {}\n\n    void added() {}\n}\n");
    }

    #[test]
    fn a_nested_class_is_found_by_its_path_and_indented_one_level_deeper() {
        let src = "class A {\n    static class B {\n        void existing() {}\n    }\n}\n";
        let insertion = insert_members(src, Some("A.B"), "    @Test\n    void added() {\n        run();\n    }").unwrap();
        assert_eq!(
            insertion.apply(src),
            "class A {\n    static class B {\n        void existing() {}\n\n        @Test\n        void added() {\n            run();\n        }\n    }\n}\n"
        );
    }

    #[test]
    fn a_class_closed_on_its_own_line_still_gets_a_line_of_its_own() {
        let src = "class A {}\n";
        let insertion = insert_members(src, None, "void added() {}").unwrap();
        assert_eq!(insertion.apply(src), "class A {\n    void added() {}\n}\n");
    }

    #[test]
    fn a_missing_class_is_refused_by_name() {
        let error = insert_members("class A {}", Some("A.Missing"), "x").unwrap_err();
        assert!(error.contains("A.Missing"), "{error}");
    }

    #[test]
    fn the_classes_of_a_file_are_listed_with_their_paths() {
        let found = types_in("class A { class B {} } class C {}");
        let paths: Vec<&str> = found.iter().map(|t| t.path.as_str()).collect();
        assert_eq!(paths, ["A", "A.B", "C"]);
        assert_eq!(found[1].depth, 1);
    }
}
