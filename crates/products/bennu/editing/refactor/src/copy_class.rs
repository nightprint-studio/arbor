//! **Copy class** — the same type, in another package, under another name.
//!
//! ## Why a copy is so much safer than a move, and where it still is not
//!
//! Moving a type breaks every reference to it, which is why a move needs the reference index of the
//! whole project and refuses more often than it succeeds. A copy breaks nothing: the original stays
//! where it was, everything that mentioned it still mentions it, and the new file is a leaf nothing
//! points at yet. There is no project-wide question to ask.
//!
//! The one thing that *does* change is what the copy itself can see. A type in `com.acme.order`
//! refers to its neighbours by simple name, because they share a package; the copy in
//! `com.acme.report` does not share it any more, and those names stop resolving. That is the whole
//! of the difficulty, and it is not answerable from the file — it needs to know which types the old
//! package declares. So this returns the names the copy **mentions** and lets the caller, which has
//! the index, decide which of them now need an import.
//!
//! ## What it rewrites
//!
//! The `package` declaration, and the type's own name — declaration, constructors, and every
//! mention of it as a type inside the file (a `Builder` returning `Order`, a `new Order()`, a
//! `Order.class`). A local variable that happens to be called `order` is not touched: renaming is
//! done over the nodes the parser calls type names, not over the text.

use tree_sitter::Node;

use crate::body::{package_of, types_in};
use crate::selection::{descendants, descendants_any, text, TYPE_DECLS};

/// A copy, before it is written anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyPlan {
    /// The new file's complete text.
    pub source: String,
    /// The type the copy declares — what the new file has to be named after.
    pub type_name: String,
    /// The package it now declares, empty for the default package.
    pub package: String,
    /// The package it came **from**. The caller needs both: what the copy can no longer see by
    /// simple name is exactly what this package declares and the new one does not.
    pub from_package: String,
    /// Every **other** type mentioned by simple name, de-duplicated. The caller resolves these
    /// against the index to decide which the copy can no longer see.
    pub mentions: Vec<String>,
}

/// Plan a copy of the primary type in `source` into `new_package`, named `new_name`.
///
/// `new_name` equal to the current name is an ordinary case — copying into a different package
/// without renaming — and so is `new_package` equal to the current one, which is "duplicate this
/// class here". Both at once is a copy that would overwrite its own original, and the caller is the
/// one that knows the file names, so it is not refused here.
///
/// `None` when the file declares no type at all: there is nothing to name the copy after, and a
/// file like that is copied by copying its bytes.
pub fn copy_class(
    root: Node<'_>,
    source: &str,
    new_name: &str,
    new_package: &str,
) -> Option<CopyPlan> {
    let primary = primary_type(root, source)?;
    let old_name = primary.child_by_field_name("name").map(|n| text(&n, source))?.to_string();

    let mut edits: Vec<(usize, usize, String)> = Vec::new();

    // ── the package line ─────────────────────────────────────────────────────
    let current = package_of(root, source).unwrap_or_default();
    if current != new_package {
        match descendants(root, "package_declaration").first() {
            Some(decl) => {
                if new_package.is_empty() {
                    // Into the default package: the declaration goes, and the blank lines under it
                    // with it, or the file starts with a gap where a statement used to be.
                    let end = blank_lines_after(source, crate::body::line_after(source, decl.end_byte()));
                    edits.push((decl.start_byte(), end, String::new()));
                } else if let Some(name) = decl.child_by_field_name("name").or_else(|| {
                    // tree-sitter-java names the child `name` on some grammar versions and leaves
                    // it unnamed on others; the identifier chain is the only child either way.
                    decl.named_children(&mut decl.walk()).next()
                }) {
                    edits.push((name.start_byte(), name.end_byte(), new_package.to_string()));
                }
            }
            // No declaration at all — the file was in the default package.
            None if !new_package.is_empty() => {
                edits.push((0, 0, format!("package {new_package};\n\n")));
            }
            None => {}
        }
    }

    // ── the type's own name ──────────────────────────────────────────────────
    if new_name != old_name {
        for (start, end) in own_name_spans(root, source, &old_name) {
            edits.push((start, end, new_name.to_string()));
        }
    }

    let mut mentions: Vec<String> = descendants(root, "type_identifier")
        .iter()
        .map(|n| text(n, source).to_string())
        .filter(|name| name != &old_name && name != new_name)
        .collect();
    mentions.sort();
    mentions.dedup();

    Some(CopyPlan {
        source: applied(source, &mut edits),
        type_name: new_name.to_string(),
        package: new_package.to_string(),
        from_package: current,
        mentions,
    })
}

/// The type a file is named after: the `public` one, else the first top-level declaration.
///
/// Java requires the public type to match the file name, so on any file that compiles the two rules
/// agree; the fallback is for the package-private classes that are legal and common in old code.
fn primary_type<'t>(root: Node<'t>, source: &str) -> Option<Node<'t>> {
    let tops: Vec<Node<'t>> = types_in(root)
        .into_iter()
        .filter(|t| t.parent().map(|p| p.kind()) == Some("program"))
        .collect();
    tops.iter()
        .find(|t| crate::body::has_modifier(t, source, "public"))
        .or_else(|| tops.first())
        .copied()
}

/// Every span naming *this* type: its declaration, its constructors, and every mention of it as a
/// type. Not a text search — a field called `order` and a class called `Order` are different words
/// to the parser and the same word to a regular expression.
fn own_name_spans(root: Node<'_>, source: &str, name: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    // The declarations that carry a name field: the type itself, its constructors, and any nested
    // type that shadows the name (which cannot happen in valid Java, but costs nothing to allow).
    for decl in descendants_any(root, &[TYPE_DECLS, &["constructor_declaration"]].concat()) {
        if let Some(id) = decl.child_by_field_name("name") {
            if text(&id, source) == name {
                out.push((id.start_byte(), id.end_byte()));
            }
        }
    }
    for id in descendants(root, "type_identifier") {
        if text(&id, source) == name {
            out.push((id.start_byte(), id.end_byte()));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Past every line from `from` that holds nothing but whitespace.
fn blank_lines_after(source: &str, from: usize) -> usize {
    let mut at = from;
    while at < source.len() {
        let line_end = source[at..].find('\n').map(|i| at + i + 1).unwrap_or(source.len());
        if !source[at..line_end].trim().is_empty() {
            break;
        }
        at = line_end;
    }
    at
}

/// Apply the edits back to front, so the offsets ahead of each one stay valid.
fn applied(source: &str, edits: &mut [(usize, usize, String)]) -> String {
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut out = source.to_string();
    for (start, end, text) in edits.iter() {
        if *start <= *end && *end <= out.len() {
            out.replace_range(*start..*end, text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"package com.acme.order;

import java.util.List;

public class Order {
    private final List<OrderLine> lines;
    private String order;

    public Order(List<OrderLine> lines) {
        this.lines = lines;
    }

    public static Order empty() {
        return new Order(List.of());
    }
}
"#;

    fn plan(new_name: &str, new_package: &str) -> CopyPlan {
        let tree = bennu_java::prelude::parse_java(SRC).expect("parses");
        copy_class(tree.root_node(), SRC, new_name, new_package).expect("a plan")
    }

    #[test]
    fn a_copy_into_another_package_says_so_in_the_package_line() {
        let out = plan("Order", "com.acme.report");
        assert!(out.source.starts_with("package com.acme.report;"));
        assert!(out.source.contains("public class Order {"), "and is not renamed");
        assert_eq!(out.package, "com.acme.report");
    }

    #[test]
    fn a_rename_follows_the_type_everywhere_it_is_a_type() {
        let out = plan("Invoice", "com.acme.order");
        assert!(out.source.contains("public class Invoice {"));
        assert!(out.source.contains("public Invoice(List<OrderLine> lines)"), "the constructor");
        assert!(out.source.contains("public static Invoice empty()"), "the return type");
        assert!(out.source.contains("return new Invoice(List.of());"), "the instantiation");
    }

    /// The reason this walks nodes instead of replacing text: a **field** called `order` is not the
    /// class called `Order`, and a project full of them is exactly where a text rename does damage.
    #[test]
    fn a_name_that_is_not_the_type_is_left_alone() {
        let out = plan("Invoice", "com.acme.order");
        assert!(out.source.contains("private String order;"));
        assert!(out.source.contains("package com.acme.order;"), "including in the package name");
    }

    /// What the caller needs to fix the imports: the neighbours the copy refers to by simple name.
    #[test]
    fn the_types_it_mentions_come_back_for_the_caller_to_resolve() {
        let out = plan("Invoice", "com.acme.report");
        assert!(out.mentions.contains(&"OrderLine".to_string()));
        assert!(out.mentions.contains(&"List".to_string()));
        assert!(!out.mentions.contains(&"Order".to_string()), "not its own name");
        assert!(!out.mentions.contains(&"Invoice".to_string()));
    }

    #[test]
    fn a_file_with_no_package_gains_one() {
        let src = "public class Bare {}\n";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let out = copy_class(tree.root_node(), src, "Bare", "com.acme").unwrap();
        assert!(out.source.starts_with("package com.acme;\n\npublic class Bare"));
    }

    #[test]
    fn a_copy_into_the_default_package_loses_the_declaration() {
        let out = plan("Order", "");
        assert!(!out.source.contains("package "));
        assert!(out.source.starts_with("import java.util.List;"));
    }

    #[test]
    fn an_interface_is_copied_like_a_class() {
        let src = "package a;\npublic interface Repo {\n    Repo self();\n}\n";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        let out = copy_class(tree.root_node(), src, "Store", "b").unwrap();
        assert!(out.source.contains("public interface Store {"));
        assert!(out.source.contains("Store self();"));
        assert!(out.source.starts_with("package b;"));
    }

    #[test]
    fn a_file_declaring_nothing_is_not_a_class_to_copy() {
        let src = "// just a comment\n";
        let tree = bennu_java::prelude::parse_java(src).unwrap();
        assert_eq!(copy_class(tree.root_node(), src, "X", "a"), None);
    }
}
