//! File and line history for a file named by **path** — the backend of Bennu's *Git → Show
//! History* and *Show History for Selection*.
//!
//! Same door as [`crate::diff::get_file_blame_by_path`]: no tab, the repository is discovered
//! from the path and kept for its idle TTL ([`crate::ephemeral`]). Three verbs, because the
//! dialog asks three things: which commits touched this file, what one of them changed in it,
//! and — for a selection — which commits touched *those lines*, each with the lines' own diff.
//!
//! The selection history shells out to `git log -L`: libgit2 has no line-range log, and
//! reimplementing git's line-tracking across moves and renames would be a bug farm. Everything
//! interpolated into the command line is validated first (line numbers are integers, the object
//! id is hex), because a path or id that starts with `-` would otherwise be an option.

use corvus_core::prelude::CorvusState;
use serde::Serialize;

use crate::ephemeral::tab_for;
use crate::mcp_tools::{relative_to, walk_log, ymd, LogEntry};
use crate::repo::{git, open, repo_path};

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;

/// Commits that touched `file`, newest first.
#[arbor_rpc::handler]
fn file_history_by_path(
    state: &CorvusState,
    file: String,
    limit: Option<usize>,
) -> Result<Vec<LogEntry>, String> {
    let tab = tab_for(state, &file)?;
    let workdir = repo_path(state, &tab)?;
    let repo = open(state, &tab)?;
    let rel = relative_to(&workdir, &file);
    walk_log(&repo, Some(&rel), limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT))
}

/// One commit that touched a line range, with what it did to those lines.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct LineHistoryEntry {
    pub commit: String,
    pub short: String,
    pub author: String,
    /// `YYYY-MM-DD`, UTC.
    pub date: String,
    pub summary: String,
    /// The unified diff of the tracked lines in this commit.
    pub patch: String,
}

/// Commits that touched lines `start_line..=end_line` of `file` (1-based), newest first, each
/// with the diff of just those lines — `git log -L`, so it follows the lines through moves.
#[arbor_rpc::handler]
fn line_history_by_path(
    state: &CorvusState,
    file: String,
    start_line: usize,
    end_line: usize,
    limit: Option<usize>,
) -> Result<Vec<LineHistoryEntry>, String> {
    if start_line == 0 || end_line < start_line {
        return Err(format!("invalid line range {start_line}..{end_line}"));
    }
    let tab = tab_for(state, &file)?;
    let workdir = repo_path(state, &tab)?;
    let rel = relative_to(&workdir, &file);
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let output = git(state)
        .command()
        .current_dir(&workdir)
        .args(["log", "--no-color", &format!("-n{limit}"), &format!("--format={HEADER_FORMAT}")])
        .arg(format!("-L{start_line},{end_line}:{rel}"))
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(parse_line_log(&String::from_utf8_lossy(&output.stdout)))
}

/// What one file looked like in a commit's diff: `git show` limited to the file.
#[arbor_rpc::handler]
fn file_commit_patch_by_path(state: &CorvusState, file: String, commit: String) -> Result<String, String> {
    // An object id is hex. Anything else — above all a leading `-` — is not one, and must not
    // reach the command line.
    if commit.is_empty() || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("'{commit}' is not a commit id"));
    }
    let tab = tab_for(state, &file)?;
    let workdir = repo_path(state, &tab)?;
    let rel = relative_to(&workdir, &file);

    let output = git(state)
        .command()
        .current_dir(&workdir)
        .args(["show", "--no-color", "--format=", "--patch", &commit, "--", &rel])
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Record separator (U+0001) opens each commit; unit separator (U+001F) splits its fields.
/// Control characters, because a subject line can contain anything printable.
const HEADER_FORMAT: &str = "%x01%H%x1f%an%x1f%at%x1f%s";

/// Parse `git log -L --format=HEADER_FORMAT` output: per commit, one header line then its patch.
fn parse_line_log(out: &str) -> Vec<LineHistoryEntry> {
    out.split('\u{1}')
        .filter(|chunk| !chunk.trim().is_empty())
        .filter_map(|chunk| {
            let (header, patch) = chunk.split_once('\n').unwrap_or((chunk, ""));
            let mut fields = header.split('\u{1f}');
            let commit = fields.next()?.trim().to_string();
            let author = fields.next()?.to_string();
            let at = fields.next()?.trim().parse::<i64>().ok()?;
            let summary = fields.next().unwrap_or("").to_string();
            if commit.is_empty() {
                return None;
            }
            Some(LineHistoryEntry {
                short: commit.chars().take(8).collect(),
                commit,
                author,
                date: ymd(at),
                summary,
                patch: patch.trim_matches('\n').to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_commits_with_their_patches() {
        let out = "\u{1}aaaaaaaaaaaa\u{1f}Ann\u{1f}0\u{1f}first change\n\ndiff --git a/f b/f\n@@ -1 +1 @@\n-a\n+b\n\u{1}bbbbbbbbbbbb\u{1f}Bob\u{1f}86400\u{1f}second\n\ndiff --git a/f b/f\n@@ -0,0 +1 @@\n+a\n";
        let entries = parse_line_log(out);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].author, "Ann");
        assert_eq!(entries[0].date, "1970-01-01");
        assert_eq!(entries[0].short, "aaaaaaaa");
        assert!(entries[0].patch.starts_with("diff --git"));
        assert_eq!(entries[1].date, "1970-01-02");
        assert!(entries[1].patch.contains("+a"));
    }

    #[test]
    fn a_subject_with_odd_characters_survives() {
        let out = "\u{1}abc\u{1f}Zoë\u{1f}0\u{1f}fix: handle \"quotes\" & ünïcode\n\n";
        let entries = parse_line_log(out);
        assert_eq!(entries[0].summary, "fix: handle \"quotes\" & ünïcode");
        assert_eq!(entries[0].patch, "");
    }

    #[test]
    fn empty_output_is_no_history() {
        assert!(parse_line_log("").is_empty());
    }
}
