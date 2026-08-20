//! Diffing two revisions.
//!
//! Here rather than in the caller for one reason: "what changed" has to mean the same
//! thing in the Local History dialog as it does everywhere else in the app, and it can
//! only do that if it is the same implementation. The output is a plain line model —
//! no HTML, no styling, no assumptions about who draws it.

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

/// What one line of a diff is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

/// One rendered line. Both numbers are 1-based; the one that does not apply is absent
/// (an added line has no old number), which is exactly what a gutter needs to draw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<u32>,
    pub text: String,
}

/// A run of changed lines with its context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

/// The whole comparison, plus the two counts a header wants to show.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDelta {
    pub hunks: Vec<DiffHunk>,
    pub added: usize,
    pub removed: usize,
    /// `true` when the two sides are identical — which a caller should say out loud
    /// rather than render as an empty panel that looks like a failure to load.
    pub identical: bool,
}

/// Compare two texts, keeping `context` unchanged lines around each run of changes.
pub fn compare(old: &str, new: &str, context: usize) -> TextDelta {
    if old == new {
        return TextDelta { identical: true, ..Default::default() };
    }
    let diff = TextDiff::from_lines(old, new);
    let mut out = TextDelta::default();

    for group in diff.grouped_ops(context) {
        let mut lines = Vec::new();
        let mut old_start = 0;
        let mut new_start = 0;
        let mut first = true;
        for op in &group {
            for change in diff.iter_changes(op) {
                let o = change.old_index().map(|i| i as u32 + 1);
                let n = change.new_index().map(|i| i as u32 + 1);
                if first {
                    old_start = o.unwrap_or(0);
                    new_start = n.unwrap_or(0);
                    first = false;
                }
                let kind = match change.tag() {
                    ChangeTag::Equal => DiffLineKind::Context,
                    ChangeTag::Insert => {
                        out.added += 1;
                        DiffLineKind::Add
                    }
                    ChangeTag::Delete => {
                        out.removed += 1;
                        DiffLineKind::Del
                    }
                };
                lines.push(DiffLine {
                    kind,
                    old: o,
                    new: n,
                    // The newline belongs to the layout, not to the content: a renderer
                    // that gets it back draws a blank line after every row.
                    text: change.value().trim_end_matches(['\n', '\r']).to_string(),
                });
            }
        }
        out.hunks.push(DiffHunk { old_start, new_start, lines });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_say_so_instead_of_producing_nothing() {
        let d = compare("a\nb\n", "a\nb\n", 3);
        assert!(d.identical);
        assert!(d.hunks.is_empty());
    }

    #[test]
    fn counts_and_numbers_line_up() {
        let d = compare("one\ntwo\nthree\n", "one\nTWO\nthree\nfour\n", 1);
        assert_eq!((d.added, d.removed), (2, 1));
        let lines = &d.hunks[0].lines;
        let del = lines.iter().find(|l| l.kind == DiffLineKind::Del).unwrap();
        assert_eq!((del.old, del.text.as_str()), (Some(2), "two"));
        let add = lines.iter().find(|l| l.kind == DiffLineKind::Add).unwrap();
        assert_eq!((add.new, add.text.as_str()), (Some(2), "TWO"));
    }

    #[test]
    fn a_line_carries_no_newline() {
        let d = compare("a\n", "b\n", 3);
        assert!(d.hunks[0].lines.iter().all(|l| !l.text.contains('\n')));
    }
}
