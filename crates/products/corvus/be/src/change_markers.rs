//! Where an editor buffer differs from HEAD — the backend of Bennu's change gutter.
//!
//! The editor sends the text it holds (saved or not) and gets back the line ranges that were
//! added, modified or removed relative to the committed file, each with the lines as they were.
//! No tab is involved: the repository is discovered from the file path ([`crate::ephemeral`]).
//!
//! Pure diffing lives in [`change_hunks`] so it is testable without a repository; the handler is
//! only the glue that reads the HEAD blob. Both sides are normalised to LF first — a checkout
//! with CRLF against a blob with LF would otherwise mark every line as changed.

use corvus_core::prelude::CorvusState;
use serde::Serialize;
use similar::{DiffOp, TextDiff};

use crate::ephemeral::tab_for;
use crate::mcp_tools::relative_to;
use crate::repo::{open, repo_path};

/// Past this many bytes on either side the diff is skipped: the gutter is a convenience and a
/// multi-megabyte file recomputed after every pause in typing is not worth it.
const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    /// Lines that existed in HEAD and are gone. `line_count` is 0; `start_line` is the line that
    /// now follows the gap (clamped to the last line).
    Removed,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ChangeHunk {
    pub kind: ChangeKind,
    /// 1-based first line in the buffer.
    pub start_line: usize,
    /// Buffer lines covered (0 for a removal).
    pub line_count: usize,
    /// The block as HEAD has it (empty for an addition), newline-terminated per line.
    pub old_text: String,
}

#[derive(Debug, Serialize, Default, PartialEq, Eq)]
pub struct ChangeMarkers {
    /// The file exists in HEAD. False for an untracked file, a new repo, or no repository: no
    /// markers are meaningful then, so the gutter stays empty rather than painting every line.
    pub tracked: bool,
    /// The diff was not run because a side exceeded the size limit.
    pub skipped: bool,
    pub hunks: Vec<ChangeHunk>,
}

fn normalise(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Line ranges of `text` that differ from `head`.
pub fn change_hunks(head: &str, text: &str) -> Vec<ChangeHunk> {
    let (head, text) = (normalise(head), normalise(text));
    let diff = TextDiff::from_lines(&head, &text);
    let old_lines: Vec<&str> = head.split_inclusive('\n').collect();
    let new_len = text.split_inclusive('\n').count();
    let old_block = |index: usize, len: usize| old_lines[index..index + len].concat();

    diff.ops()
        .iter()
        .filter_map(|op| match *op {
            DiffOp::Equal { .. } => None,
            DiffOp::Insert { new_index, new_len, .. } => Some(ChangeHunk {
                kind: ChangeKind::Added,
                start_line: new_index + 1,
                line_count: new_len,
                old_text: String::new(),
            }),
            DiffOp::Replace { old_index, old_len, new_index, new_len } => Some(ChangeHunk {
                kind: ChangeKind::Modified,
                start_line: new_index + 1,
                line_count: new_len,
                old_text: old_block(old_index, old_len),
            }),
            DiffOp::Delete { old_index, old_len, new_index } => Some(ChangeHunk {
                kind: ChangeKind::Removed,
                start_line: (new_index + 1).min(new_len.max(1)),
                line_count: 0,
                old_text: old_block(old_index, old_len),
            }),
        })
        .collect()
}

/// How `text` — the buffer of `file` — differs from the committed version of that file.
#[arbor_rpc::handler]
fn change_markers_by_path(state: &CorvusState, file: String, text: String) -> Result<ChangeMarkers, String> {
    let Ok(tab) = tab_for(state, &file) else {
        return Ok(ChangeMarkers::default());
    };
    let workdir = repo_path(state, &tab)?;
    let repo = open(state, &tab)?;
    let rel = relative_to(&workdir, &file);

    // An unborn HEAD (no commits yet) or a path HEAD does not have are both "untracked".
    let head_blob = repo
        .head()
        .and_then(|h| h.peel_to_tree())
        .and_then(|tree| tree.get_path(std::path::Path::new(&rel)))
        .and_then(|entry| repo.find_blob(entry.id()));
    let Ok(blob) = head_blob else {
        return Ok(ChangeMarkers::default());
    };
    if blob.is_binary() || blob.content().len() > MAX_BYTES || text.len() > MAX_BYTES {
        return Ok(ChangeMarkers { tracked: true, skipped: true, hunks: Vec::new() });
    }
    let head = String::from_utf8_lossy(blob.content()).into_owned();
    Ok(ChangeMarkers { tracked: true, skipped: false, hunks: change_hunks(&head, &text) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_no_hunks() {
        assert!(change_hunks("a\nb\n", "a\nb\n").is_empty());
    }

    #[test]
    fn crlf_against_lf_is_not_a_change() {
        assert!(change_hunks("a\nb\n", "a\r\nb\r\n").is_empty());
    }

    #[test]
    fn an_inserted_line_is_added() {
        let h = change_hunks("a\nc\n", "a\nb\nc\n");
        assert_eq!(h, vec![ChangeHunk { kind: ChangeKind::Added, start_line: 2, line_count: 1, old_text: String::new() }]);
    }

    #[test]
    fn an_edited_line_is_modified_and_keeps_the_old_text() {
        let h = change_hunks("a\nb\nc\n", "a\nX\nc\n");
        assert_eq!(h, vec![ChangeHunk { kind: ChangeKind::Modified, start_line: 2, line_count: 1, old_text: "b\n".into() }]);
    }

    #[test]
    fn a_deleted_line_is_removed_at_the_line_that_follows() {
        let h = change_hunks("a\nb\nc\n", "a\nc\n");
        assert_eq!(h, vec![ChangeHunk { kind: ChangeKind::Removed, start_line: 2, line_count: 0, old_text: "b\n".into() }]);
    }

    #[test]
    fn a_deletion_at_the_end_is_clamped_to_the_last_line() {
        let h = change_hunks("a\nb\n", "a\n");
        assert_eq!(h[0].kind, ChangeKind::Removed);
        assert_eq!(h[0].start_line, 1);
    }
}
