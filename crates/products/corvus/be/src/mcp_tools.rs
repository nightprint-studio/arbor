//! The git tools Corvus offers an AI client (`#[handler(mcp(...))]`).
//!
//! Deliberately a handful, not the 179 handlers behind the UI: a model choosing among
//! everything Corvus can do would be choosing among `reset_hard` and `status`. These are the
//! questions an assistant actually asks of a repository — what changed, who wrote this, what
//! happened here — plus the one write it is trusted with, `corvus_commit`. No push, no amend,
//! no branch surgery: those stay in the UI.
//!
//! Every tool names its repository by **absolute path** and works with no open tab: the path
//! is turned into a headless repo ([`crate::ephemeral`]) that the idle TTL later reclaims.
//! Who may call what, and on which projects, is decided shell-side by the MCP policy — this
//! module only describes the tools honestly (`safety = read | write`).

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{get_file_blame, get_status, BlameLine, RepoStatus};
use git2::{Repository, Sort};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ephemeral::tab_for;
use crate::repo::{open, repo_path};

const DEFAULT_LOG: usize = 20;
const MAX_LOG: usize = 200;
/// Commits examined when filtering the log by path — bounds the walk on a huge history.
const MAX_LOG_SCAN: usize = 20_000;

/// Args for [`corvus_status`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RepoArgs {
    /// Absolute path to the repository root, or to any folder or file inside it.
    pub root: String,
}

/// Branch, ahead/behind, and every changed file split into staged / unstaged / untracked /
/// conflicted. Read this before `corvus_commit` to see exactly what a commit would contain.
#[arbor_rpc::handler(mcp(
    title = "Git status",
    safety = read,
))]
fn corvus_status(state: &CorvusState, args: RepoArgs) -> Result<RepoStatus, String> {
    let tab = tab_for(state, &args.root)?;
    let repo = open(state, &tab)?;
    get_status(&repo).map_err(|e| e.to_string())
}

/// Args for [`corvus_log`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogArgs {
    /// Absolute path to the repository root, or to any folder or file inside it.
    pub root: String,
    /// Only commits that changed this file or folder. Absolute, or relative to the repository
    /// root.
    pub path: Option<String>,
    /// How many commits to return, newest first. Default 20, at most 200.
    pub limit: Option<usize>,
}

/// One commit, as `corvus_log` reports it.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct LogEntry {
    pub commit: String,
    pub short: String,
    pub author: String,
    /// `YYYY-MM-DD`, UTC.
    pub date: String,
    pub summary: String,
}

/// The commit history of the current branch, newest first, optionally narrowed to one file or
/// folder. For *who wrote which line* use `corvus_blame`; this answers "what happened here,
/// and when".
#[arbor_rpc::handler(mcp(
    title = "Git log",
    safety = read,
))]
fn corvus_log(state: &CorvusState, args: LogArgs) -> Result<Vec<LogEntry>, String> {
    let tab = tab_for(state, &args.root)?;
    let workdir = repo_path(state, &tab)?;
    let repo = open(state, &tab)?;
    let limit = args.limit.unwrap_or(DEFAULT_LOG).clamp(1, MAX_LOG);
    let rel = args.path.as_deref().map(|p| relative_to(&workdir, p));
    walk_log(&repo, rel.as_deref(), limit)
}

pub(crate) fn walk_log(repo: &Repository, rel: Option<&str>, limit: usize) -> Result<Vec<LogEntry>, String> {
    let mut walk = repo.revwalk().map_err(|e| e.to_string())?;
    walk.push_head().map_err(|e| format!("no commits yet: {e}"))?;
    walk.set_sorting(Sort::TIME).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for (scanned, oid) in walk.enumerate() {
        if scanned >= MAX_LOG_SCAN || out.len() >= limit {
            break;
        }
        let commit = repo.find_commit(oid.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if let Some(rel) = rel {
            if !commit_touches(&commit, rel) {
                continue;
            }
        }
        let id = commit.id().to_string();
        out.push(LogEntry {
            short: id.chars().take(8).collect(),
            commit: id,
            author: commit.author().name().unwrap_or("Unknown").to_string(),
            date: ymd(commit.author().when().seconds()),
            summary: commit.summary().unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

/// Whether `commit` changed `rel` relative to its first parent (a root commit changes
/// whatever it contains).
fn commit_touches(commit: &git2::Commit<'_>, rel: &str) -> bool {
    let entry_id = |c: &git2::Commit<'_>| {
        c.tree().ok().and_then(|t| t.get_path(std::path::Path::new(rel)).ok()).map(|e| e.id())
    };
    let now = entry_id(commit);
    match commit.parent(0) {
        Ok(parent) => now != entry_id(&parent),
        Err(_) => now.is_some(),
    }
}

/// Args for [`corvus_blame`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BlameArgs {
    /// Absolute path to a file inside a git repository.
    pub file: String,
}

/// A run of consecutive lines last changed by the same commit.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BlameRange {
    pub start_line: usize,
    pub end_line: usize,
    pub commit: String,
    pub short: String,
    pub author: String,
    /// `YYYY-MM-DD`, UTC.
    pub date: String,
    pub summary: String,
}

/// Who last changed each line of a file, as of HEAD, folded into runs of lines that share a
/// commit (far smaller than line-by-line). Uncommitted edits are not reflected — a line you
/// just wrote is attributed to whoever last committed that position.
#[arbor_rpc::handler(mcp(
    title = "Git blame",
    safety = read,
))]
fn corvus_blame(state: &CorvusState, args: BlameArgs) -> Result<Vec<BlameRange>, String> {
    let tab = tab_for(state, &args.file)?;
    let workdir = repo_path(state, &tab)?;
    let repo = open(state, &tab)?;
    let rel = relative_to(&workdir, &args.file);
    let lines = get_file_blame(&repo, &rel).map_err(|e| e.to_string())?;
    Ok(fold_blame(&lines))
}

/// Collapse per-line blame into runs of one commit.
fn fold_blame(lines: &[BlameLine]) -> Vec<BlameRange> {
    let mut out: Vec<BlameRange> = Vec::new();
    for l in lines {
        match out.last_mut() {
            Some(last) if last.commit == l.commit_oid && last.end_line + 1 == l.line_no => {
                last.end_line = l.line_no;
            }
            _ => out.push(BlameRange {
                start_line: l.line_no,
                end_line: l.line_no,
                commit: l.commit_oid.clone(),
                short: l.short_oid.clone(),
                author: l.author_name.clone(),
                date: ymd(l.timestamp),
                summary: l.summary.clone(),
            }),
        }
    }
    out
}

/// Args for [`corvus_commit`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CommitArgs {
    /// Absolute path to the repository root, or to any folder or file inside it.
    pub root: String,
    /// The commit message. First line is the subject.
    pub message: String,
    /// Files or folders to stage before committing — absolute, or relative to the repository
    /// root. Omit to commit exactly what is already staged (see `corvus_status`).
    pub paths: Option<Vec<String>>,
}

/// What `corvus_commit` made.
#[derive(Debug, Serialize)]
pub struct CommitResult {
    pub commit: String,
    pub short: String,
    pub summary: String,
    /// How many files the commit contains.
    pub files: usize,
}

/// Create a commit on the current branch from the staged changes, staging `paths` first when
/// given. Never amends, never pushes. Fails if nothing is staged, and plugin pre-commit hooks
/// can still veto it — the reason comes back as the error. The commit is local and undoable
/// (`git reset --soft HEAD~1`), but it does move the branch: call `corvus_status` first.
#[arbor_rpc::handler(mcp(
    title = "Commit staged changes",
    safety = write,
))]
fn corvus_commit(state: &CorvusState, args: CommitArgs) -> Result<CommitResult, String> {
    let message = args.message.trim().to_string();
    if message.is_empty() {
        return Err("The commit message is empty.".to_string());
    }
    let tab = tab_for(state, &args.root)?;
    let workdir = repo_path(state, &tab)?;

    if let Some(paths) = args.paths.filter(|p| !p.is_empty()) {
        let rels: Vec<String> = paths.iter().map(|p| relative_to(&workdir, p)).collect();
        // The MCP policy only sees *absolute* paths in the arguments, so a relative one is the
        // one way to name a file outside the project it was checked against. Refuse it here.
        if let Some(bad) = rels.iter().find(|p| p.split('/').any(|seg| seg == "..")) {
            return Err(format!("'{bad}' leaves the repository; pass a path inside it."));
        }
        let repo = open(state, &tab)?;
        corvus_git::stage::stage_paths(&repo, &rels).map_err(|e| e.to_string())?;
    }

    let staged = {
        let repo = open(state, &tab)?;
        get_status(&repo).map_err(|e| e.to_string())?.staged.len()
    };
    if staged == 0 {
        return Err("Nothing is staged, so there is nothing to commit. Pass `paths`, or check \
                    `corvus_status`."
            .to_string());
    }

    let oid = crate::stage::commit(state, tab, message.clone(), false)?;
    Ok(CommitResult {
        short: oid.chars().take(8).collect(),
        commit: oid,
        summary: message.lines().next().unwrap_or("").to_string(),
        files: staged,
    })
}

/// `path` as a repo-relative, forward-slash path: an absolute path under `workdir` is cut down
/// to its tail, anything else is taken as already relative.
pub(crate) fn relative_to(workdir: &str, path: &str) -> String {
    let norm = path.replace('\\', "/");
    let base = workdir.replace('\\', "/");
    let base = base.trim_end_matches('/');
    let inside = norm
        .get(..base.len())
        .filter(|head| head.eq_ignore_ascii_case(base))
        .and_then(|_| norm.get(base.len()..))
        // The boundary matters: `/w/repo2/x` shares a prefix with `/w/repo` and is not in it.
        .filter(|tail| tail.is_empty() || tail.starts_with('/'));
    match inside {
        Some(tail) => tail.trim_start_matches('/').to_string(),
        None => norm,
    }
}

/// `YYYY-MM-DD` (UTC) from a unix timestamp — civil-from-days, so no date crate is needed for
/// a label.
pub(crate) fn ymd(ts: i64) -> String {
    let days = ts.div_euclid(86_400) + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(no: usize, oid: &str, author: &str) -> BlameLine {
        BlameLine {
            line_no: no,
            content: String::new(),
            commit_oid: oid.into(),
            short_oid: oid.chars().take(8).collect(),
            author_name: author.into(),
            author_email: String::new(),
            timestamp: 0,
            summary: "s".into(),
            is_group_start: false,
        }
    }

    #[test]
    fn ymd_formats_known_instants() {
        assert_eq!(ymd(0), "1970-01-01");
        assert_eq!(ymd(951_782_400), "2000-02-29");
        assert_eq!(ymd(1_700_000_000), "2023-11-14");
    }

    #[test]
    fn relative_to_cuts_the_workdir_prefix() {
        assert_eq!(relative_to("C:/w/repo", r"C:\w\repo\src\a.rs"), "src/a.rs");
        assert_eq!(relative_to("/w/repo", "/w/repo/src/a.rs"), "src/a.rs");
        assert_eq!(relative_to("/w/repo", "src/a.rs"), "src/a.rs");
        // A sibling that merely shares the prefix is not inside the repo.
        assert_eq!(relative_to("/w/repo", "/w/repo2/x"), "/w/repo2/x");
    }

    #[test]
    fn blame_folds_consecutive_lines_of_one_commit() {
        let lines = [line(1, "aaaaaaaaaa", "A"), line(2, "aaaaaaaaaa", "A"), line(3, "bbbbbbbbbb", "B"), line(4, "aaaaaaaaaa", "A")];
        let ranges = fold_blame(&lines);
        assert_eq!(ranges.len(), 3);
        assert_eq!((ranges[0].start_line, ranges[0].end_line), (1, 2));
        assert_eq!((ranges[1].start_line, ranges[1].end_line), (3, 3));
        // The same commit coming back later is a new run, not merged across the gap.
        assert_eq!(ranges[2].commit, "aaaaaaaaaa");
    }
}
