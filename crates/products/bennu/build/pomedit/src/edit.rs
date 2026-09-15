//! The mechanics every write here is made of: a byte-range replacement, and the two ways of
//! putting a new element into a document that already has its own shape.
//!
//! ## Why insert rather than re-serialise
//!
//! The obvious implementation of "add a module" is: parse the pom, add to the model, write the
//! model back. It is also the one that makes the feature unusable. A pom carries comments nobody
//! wants to lose, an element order the team chose, an indentation the repository is consistent
//! about, and licence headers older than the project — none of which survives a round trip through
//! a model that was built to answer questions. The diff of a re-serialised pom is the whole file,
//! and a whole-file diff is one nobody reviews, so the one line that matters is the one line
//! nobody sees.
//!
//! So every write is a **byte-range edit** against the text as it stands, and the file it produces
//! differs from the file it was given only where the change is.

use bennu_xml::prelude::Doc;

/// One byte-range replacement. An insertion is the degenerate case where `start == end`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl Edit {
    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self { start: at, end: at, text: text.into() }
    }

    pub fn replace(start: usize, end: usize, text: impl Into<String>) -> Self {
        Self { start, end, text: text.into() }
    }
}

/// Apply `edits` to `source`, last first so earlier offsets stay valid.
///
/// Overlapping edits are not something any producer here creates, and rather than defining a
/// resolution rule nobody would remember, a later edit simply lands on the text as the earlier
/// ones left it — which for non-overlapping ranges is exactly the obvious result.
pub fn apply(source: &str, edits: &[Edit]) -> String {
    let mut ordered: Vec<&Edit> = edits.iter().collect();
    ordered.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = source.to_string();
    for edit in ordered {
        let (start, end) = (edit.start.min(out.len()), edit.end.min(out.len()));
        if start <= end {
            out.replace_range(start..end, &edit.text);
        }
    }
    out
}

/// The indentation one nesting level is worth in this document — read off the file rather than
/// assumed, because a pom written with tabs and a pom written with four spaces are both ordinary
/// and an insertion that guesses wrong is visible on the first line of the diff.
pub fn indent_unit(doc: &Doc<'_>) -> String {
    let Some(root) = doc.root() else { return "  ".to_string() };
    let root_indent = doc.indent_at(doc.scan().tags[root].start).len();
    for child in doc.children(root) {
        let indent = doc.indent_at(doc.scan().tags[child].start);
        // Only a child that actually starts its own line says anything about the unit.
        if indent.len() > root_indent && !indent.is_empty() {
            return indent[root_indent..].to_string();
        }
    }
    "  ".to_string()
}

/// Add `snippet` as the **last child** of the element opened at `parent`, indented to match the
/// children it joins.
///
/// Anchored to the end of the last child (or to the open tag, for an empty element) rather than to
/// the close tag: writing just before `</modules>` puts the new line where the close tag's own
/// indentation is, and the result is a child indented like its parent.
pub fn append_child(doc: &Doc<'_>, parent: usize, snippet: &str) -> Option<Edit> {
    let unit = indent_unit(doc);
    let parent_indent = doc.indent_at(doc.scan().tags[parent].start).to_string();
    let child_indent = format!("{parent_indent}{unit}");

    match doc.children(parent).last().copied() {
        Some(last) => {
            let (_, end) = doc.element_span(last)?;
            Some(Edit::insert(end, format!("\n{child_indent}{snippet}")))
        }
        // An empty container is written on one line as often as on two, and only the two-line form
        // reads as a list — so the close tag is pushed down with the insertion.
        None => {
            let (start, end) = doc.inner_span(parent)?;
            Some(Edit::replace(start, end, format!("\n{child_indent}{snippet}\n{parent_indent}")))
        }
    }
}

/// Add `snippet` as a child of `parent`, placed **before** the first child named in `before` — the
/// conventional position for an element Maven's own documentation orders.
///
/// Falls back to appending when none of them is there, which is the case for a pom that only
/// declares coordinates.
pub fn insert_child_before(
    doc: &Doc<'_>,
    parent: usize,
    before: &[&str],
    snippet: &str,
) -> Option<Edit> {
    let unit = indent_unit(doc);
    let parent_indent = doc.indent_at(doc.scan().tags[parent].start).to_string();
    let child_indent = format!("{parent_indent}{unit}");

    let anchor = doc
        .children(parent)
        .into_iter()
        .find(|c| before.contains(&doc.name(*c)));
    match anchor {
        Some(anchor) => {
            let (start, _) = doc.element_span(anchor)?;
            // Back up over the indentation already written for the anchor, so the inserted element
            // gets its own line and the anchor keeps the one it had.
            let line_start = start - doc.indent_at(start).len();
            Some(Edit::insert(line_start, format!("{child_indent}{snippet}\n")))
        }
        None => append_child(doc, parent, snippet),
    }
}

/// The XML for an element holding `lines`, each already a complete element, indented one level in.
pub fn block(name: &str, lines: &[String], unit: &str) -> String {
    let inner: String =
        lines.iter().map(|l| format!("\n{unit}{l}")).collect::<Vec<_>>().join("");
    format!("<{name}>{inner}\n</{name}>")
}

/// Re-indent `snippet`'s continuation lines by `indent` — what turns a `block` written flat into
/// one that sits at the right depth.
pub fn shift(snippet: &str, indent: &str) -> String {
    snippet.replace('\n', &format!("\n{indent}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_apply_back_to_front() {
        let out = apply("abcdef", &[Edit::replace(0, 1, "X"), Edit::replace(4, 6, "YZ!")]);
        assert_eq!(out, "XbcdYZ!");
    }

    #[test]
    fn the_indent_unit_comes_off_the_file() {
        let four = Doc::new("<project>\n    <artifactId>a</artifactId>\n</project>");
        assert_eq!(indent_unit(&four), "    ");
        let tabs = Doc::new("<project>\n\t<artifactId>a</artifactId>\n</project>");
        assert_eq!(indent_unit(&tabs), "\t");
    }

    #[test]
    fn a_child_joins_the_list_at_its_siblings_indent() {
        let src = "<project>\n  <modules>\n    <module>core</module>\n  </modules>\n</project>";
        let doc = Doc::new(src);
        let modules = doc.child(doc.root().unwrap(), "modules").unwrap();
        let edit = append_child(&doc, modules, "<module>api</module>").unwrap();
        assert_eq!(
            apply(src, &[edit]),
            "<project>\n  <modules>\n    <module>core</module>\n    <module>api</module>\n  </modules>\n</project>"
        );
    }

    #[test]
    fn an_empty_container_gains_a_line_rather_than_a_crowded_one() {
        let src = "<project>\n  <modules></modules>\n</project>";
        let doc = Doc::new(src);
        let modules = doc.child(doc.root().unwrap(), "modules").unwrap();
        let edit = append_child(&doc, modules, "<module>api</module>").unwrap();
        assert_eq!(
            apply(src, &[edit]),
            "<project>\n  <modules>\n    <module>api</module>\n  </modules>\n</project>"
        );
    }

    #[test]
    fn a_new_element_takes_its_conventional_place() {
        let src = "<project>\n  <artifactId>a</artifactId>\n  <build>x</build>\n</project>";
        let doc = Doc::new(src);
        let root = doc.root().unwrap();
        let edit = insert_child_before(&doc, root, &["build"], "<properties/>").unwrap();
        assert_eq!(
            apply(src, &[edit]),
            "<project>\n  <artifactId>a</artifactId>\n  <properties/>\n  <build>x</build>\n</project>"
        );
    }

    #[test]
    fn with_no_anchor_it_lands_at_the_end() {
        let src = "<project>\n  <artifactId>a</artifactId>\n</project>";
        let doc = Doc::new(src);
        let root = doc.root().unwrap();
        let edit = insert_child_before(&doc, root, &["build"], "<properties/>").unwrap();
        assert_eq!(
            apply(src, &[edit]),
            "<project>\n  <artifactId>a</artifactId>\n  <properties/>\n</project>"
        );
    }
}
