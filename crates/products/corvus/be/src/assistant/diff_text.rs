//! A diff as text a model can read, within a budget.
//!
//! The Stage panel and the MCP tools hold a diff as structured files and hunks; a prompt wants a
//! unified patch. The budget is what keeps one enormous change from becoming the whole request: files
//! are written in order until it is spent, and what did not fit is *named* ("12 more files") rather
//! than silently dropped, so the model knows the picture is partial and says so.

use corvus_git::prelude::{DiffFile, DiffStatus, LineKind};

/// Render `files` as a unified-style patch of at most about `budget` characters.
pub fn render(files: &[DiffFile], budget: usize) -> String {
    let mut out = String::new();
    let mut omitted: Vec<&str> = Vec::new();
    for file in files {
        let block = render_file(file);
        if out.len() + block.len() > budget && !out.is_empty() {
            omitted.push(&file.path);
            continue;
        }
        if block.len() > budget {
            // One file larger than everything allowed: keep its head, say where it was cut.
            out.push_str(&cut_at_line(&block, budget));
            out.push_str("[file truncated]\n");
            continue;
        }
        out.push_str(&block);
    }
    if !omitted.is_empty() {
        let shown: Vec<&str> = omitted.iter().take(8).copied().collect();
        let more = omitted.len() - shown.len();
        out.push_str(&format!("[{} more file(s) not shown: {}{}]\n", omitted.len(), shown.join(", "), if more > 0 { format!(", and {more} more") } else { String::new() }));
    }
    out
}

fn render_file(file: &DiffFile) -> String {
    let status = match file.status {
        DiffStatus::Added | DiffStatus::Untracked => "new file",
        DiffStatus::Deleted => "deleted",
        DiffStatus::Renamed => "renamed",
        DiffStatus::Copied => "copied",
        DiffStatus::Binary => "binary",
        DiffStatus::Modified => "modified",
    };
    let mut s = match &file.old_path {
        Some(old) if old != &file.path => format!("--- {old}\n+++ {} ({status}, +{} -{})\n", file.path, file.stats.additions, file.stats.deletions),
        _ => format!("--- {0}\n+++ {0} ({status}, +{1} -{2})\n", file.path, file.stats.additions, file.stats.deletions),
    };
    if file.is_binary {
        s.push_str("(binary file)\n");
        return s;
    }
    for hunk in &file.hunks {
        s.push_str(hunk.header.trim_end());
        s.push('\n');
        for line in &hunk.lines {
            let mark = match line.kind {
                LineKind::Context => ' ',
                LineKind::Added => '+',
                LineKind::Removed => '-',
            };
            s.push(mark);
            s.push_str(line.content.trim_end_matches(['\n', '\r']));
            s.push('\n');
        }
    }
    s
}

/// The longest prefix of `text` that is at most `max` bytes and ends on a line break.
fn cut_at_line(text: &str, max: usize) -> String {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    match text[..end].rfind('\n') {
        Some(nl) => text[..=nl].to_string(),
        None => text[..end].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corvus_git::prelude::{DiffHunk, DiffLine, DiffStats};

    fn file(path: &str, status: DiffStatus, lines: &[(LineKind, &str)]) -> DiffFile {
        DiffFile {
            path: path.into(),
            old_path: None,
            status,
            hunks: vec![DiffHunk {
                header: "@@ -1,2 +1,2 @@".into(),
                old_start: 1,
                old_lines: 2,
                new_start: 1,
                new_lines: 2,
                lines: lines.iter().map(|(k, c)| DiffLine { kind: k.clone(), old_lineno: None, new_lineno: None, content: (*c).into() }).collect(),
            }],
            stats: DiffStats { additions: 1, deletions: 1 },
            is_binary: false,
            image_old: None,
            image_new: None,
            encoding: None,
        }
    }

    #[test]
    fn a_change_reads_as_a_patch() {
        let f = file("src/a.rs", DiffStatus::Modified, &[(LineKind::Context, "fn a() {\n"), (LineKind::Removed, "  1\n"), (LineKind::Added, "  2\n")]);
        let text = render(&[f], 10_000);
        assert_eq!(text, "--- src/a.rs\n+++ src/a.rs (modified, +1 -1)\n@@ -1,2 +1,2 @@\n fn a() {\n-  1\n+  2\n");
    }

    #[test]
    fn what_does_not_fit_is_named_not_dropped() {
        let big = |p: &str| file(p, DiffStatus::Modified, &[(LineKind::Added, &"x".repeat(200))]);
        let text = render(&[big("a.rs"), big("b.rs"), big("c.rs")], 400);
        assert!(text.contains("+++ a.rs"));
        assert!(text.contains("[2 more file(s) not shown: b.rs, c.rs]"), "{text}");
    }

    #[test]
    fn a_single_huge_file_is_cut_on_a_line_and_says_so() {
        let lines: Vec<(LineKind, String)> = (0..100).map(|i| (LineKind::Added, format!("line {i}"))).collect();
        let refs: Vec<(LineKind, &str)> = lines.iter().map(|(k, s)| (k.clone(), s.as_str())).collect();
        let text = render(&[file("big.txt", DiffStatus::Added, &refs)], 200);
        assert!(text.ends_with("[file truncated]\n"), "{text}");
        assert!(text.len() < 260);
        assert!(!text.contains("line 99"));
    }

    #[test]
    fn a_binary_file_is_a_line_not_a_blob() {
        let mut f = file("logo.png", DiffStatus::Binary, &[]);
        f.is_binary = true;
        f.hunks.clear();
        assert!(render(&[f], 1000).contains("(binary file)"));
    }

    #[test]
    fn a_rename_shows_both_names() {
        let mut f = file("new.rs", DiffStatus::Renamed, &[]);
        f.old_path = Some("old.rs".into());
        let text = render(&[f], 1000);
        assert!(text.starts_with("--- old.rs\n+++ new.rs (renamed"), "{text}");
    }
}
