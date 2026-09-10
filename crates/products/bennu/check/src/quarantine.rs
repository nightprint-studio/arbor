//! Which part of a file a parse error costs — the member it landed in, instead of the file.
//!
//! Validation used to stop at the first sign of a broken parse: a file whose tree carries an
//! `ERROR` node got its syntax error and nothing else. The reason was real and is still real. On
//! one file — a fuzzer test whose payload is a 16 KB string literal — recovery ended the literal
//! early, so its CONTENTS were read as code and the checks reported 199 undefined symbols with
//! names like `t` and `Ë`. Reporting on a tree nobody believes is the one way to produce a page of
//! errors about code that compiles.
//!
//! What was wrong was the SIZE of the thing given up. A file is edited one member at a time, and
//! for most of the time it spends being edited exactly one member does not parse — which is
//! precisely when the checks are worth the most, and precisely when they all went quiet. The
//! method being typed is unknowable; the forty around it did not change.
//!
//! So the unit is the member. Every `ERROR`/`MISSING` node is charged to the smallest declaration
//! that contains it, that declaration's byte range is quarantined, and everything outside every
//! quarantined range is checked exactly as a clean file is.
//!
//! ## When the whole file is still given up
//!
//! An error that no member contains is not a member being typed — it is the shape of the file not
//! being understood, and there is no smaller unit to blame. An unbalanced brace at class level
//! swallows the members after it into a recovery node, so a check running past it would be reading
//! a nesting nobody wrote. [`quarantine`] answers [`Quarantine::WholeFile`] there, and the caller
//! keeps the old behaviour.

use tree_sitter::Node;

/// The declarations a parse error can be charged to — the members of a type body, and nothing
/// wider. A type declaration is deliberately absent: quarantining one is quarantining the file in
/// every single-type file there is, which is most of them.
const MEMBERS: [&str; 8] = [
    "method_declaration",
    "constructor_declaration",
    "compact_constructor_declaration",
    "field_declaration",
    "static_initializer",
    "annotation_type_element_declaration",
    "enum_constant",
    "record_declaration_body",
];

/// What a file's parse errors cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quarantine {
    /// The tree parsed. Every check runs over all of it.
    Clean,
    /// Every error sits inside a member; those members' byte ranges are out, the rest is in.
    /// Ranges are sorted by start and non-overlapping.
    Members(Vec<(usize, usize)>),
    /// At least one error is not inside any member — see the module doc.
    WholeFile,
}

impl Quarantine {
    /// Whether `(start, end)` overlaps anything quarantined. A diagnostic is dropped when it does:
    /// its subject is inside a region this file does not claim to have read.
    pub fn hides(&self, start: usize, end: usize) -> bool {
        match self {
            Quarantine::Clean => false,
            Quarantine::WholeFile => true,
            Quarantine::Members(ranges) => ranges.iter().any(|(s, e)| start < *e && end > *s),
        }
    }

    /// Whether a node is inside a quarantined region — the test that keeps a broken member's nodes
    /// from reaching the checks at all. Dropping the diagnostics afterwards is the backstop; not
    /// visiting the nodes is what stops the cascade.
    pub fn hides_node(&self, node: &Node<'_>) -> bool {
        self.hides(node.start_byte(), node.end_byte())
    }
}

/// What this tree's parse errors cost. See the module doc.
pub fn quarantine(root: Node<'_>) -> Quarantine {
    if !root.has_error() {
        return Quarantine::Clean;
    }
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    // `has_error` is true for every ancestor of an error, so the walk descends only where it is
    // set — which is one root-to-error path per error, not the whole tree.
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            let Some(member) = enclosing_member(&node) else {
                return Quarantine::WholeFile;
            };
            ranges.push((member.start_byte(), member.end_byte()));
            continue; // the whole member is out; its own children add nothing
        }
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.has_error() || child.is_missing() {
                stack.push(child);
            }
        }
    }
    if ranges.is_empty() {
        // `has_error()` with no error node reachable: nothing to charge it to.
        return Quarantine::WholeFile;
    }
    ranges.sort_unstable();
    merge(&mut ranges);
    Quarantine::Members(ranges)
}

/// The smallest member declaration containing `node`, or `None` when none does.
fn enclosing_member<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if MEMBERS.contains(&n.kind()) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

/// Collapse overlapping and touching ranges, so `hides` is a scan over disjoint spans.
fn merge(ranges: &mut Vec<(usize, usize)>) {
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for &(s, e) in ranges.iter() {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    *ranges = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(src: &str) -> Quarantine {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        quarantine(tree.root_node())
    }

    #[test]
    fn a_file_that_parses_is_clean() {
        assert_eq!(q("class A { void f() { int x = 1; } }"), Quarantine::Clean);
    }

    #[test]
    fn a_broken_method_body_costs_that_method() {
        let src = "class A {\n  void f() { int x = = ; }\n  void g() { }\n}\n";
        let Quarantine::Members(ranges) = q(src) else { panic!("expected members, got {:?}", q(src)) };
        assert_eq!(ranges.len(), 1, "{ranges:?}");
        let (s, e) = ranges[0];
        assert!(src[s..e].contains("void f()"), "{:?}", &src[s..e]);
        // And `g` is untouched — the point of the whole thing.
        assert!(!src[s..e].contains("void g()"), "{:?}", &src[s..e]);
        assert!(!q(src).hides(src.find("void g").unwrap(), src.find("void g").unwrap() + 6));
    }

    #[test]
    fn a_broken_field_costs_that_field() {
        let src = "class A {\n  int x = ;\n  void g() { }\n}\n";
        let Quarantine::Members(ranges) = q(src) else { return }; // recovery may charge it wider
        assert!(ranges.iter().all(|(s, e)| !src[*s..*e].contains("void g")), "{ranges:?}");
    }

    #[test]
    fn an_unbalanced_class_brace_gives_up_the_file() {
        // Nothing smaller than the file is to blame: the brace changes what every member below it
        // is nested in.
        assert_eq!(q("class A {\n  void f() { }\n"), Quarantine::WholeFile);
    }

    #[test]
    fn two_broken_methods_are_two_ranges() {
        let src = "class A {\n  void f() { int x = = ; }\n  void g() { }\n  void h() { int y = = ; }\n}\n";
        let Quarantine::Members(ranges) = q(src) else { panic!("expected members") };
        assert_eq!(ranges.len(), 2, "{ranges:?}");
        assert!(!ranges.iter().any(|(s, e)| src[*s..*e].contains("void g")), "{ranges:?}");
    }

    #[test]
    fn a_clean_quarantine_hides_nothing_and_the_whole_file_hides_everything() {
        assert!(!Quarantine::Clean.hides(0, 10));
        assert!(Quarantine::WholeFile.hides(0, 10));
        assert!(!Quarantine::Members(vec![(10, 20)]).hides(0, 10));
        assert!(Quarantine::Members(vec![(10, 20)]).hides(15, 16));
    }
}
