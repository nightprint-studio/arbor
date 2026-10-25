//! The git tools Corvus offers an AI client (`#[handler(mcp(...))]`).
//!
//! Deliberately a handful, not the 179 handlers behind the UI: a model choosing among
//! everything Corvus can do would be choosing among `reset_hard` and `status`. These are the
//! questions an assistant actually asks of a repository — what changed, what does the change
//! say, who wrote this, what happened here — plus the writes that make up one ordinary
//! contribution: stage, unstage, commit, push.
//!
//! **Where the line is drawn, and why there.** Every write here is either reversible on this
//! machine or additive to the remote. `corvus_stage` and `corvus_unstage` move the index and
//! never the working tree, so no edit can be lost by either; `corvus_commit` is undone with
//! `git reset --soft HEAD~1`; `corvus_push` is a fast-forward of one branch and nothing else.
//! What stays in the UI is everything that destroys or rewrites: no amend, no force push, no
//! arbitrary refspec, no tags, no discard, no reset, no branch surgery. The asymmetry is the
//! point — undoing a bad commit is a keystroke, while recalling a force push is a conversation
//! with whoever already pulled it.
//!
//! Every tool names its repository by **absolute path** and works with no open tab: the path
//! is turned into a headless repo ([`crate::ephemeral`]) that the idle TTL later reclaims.
//! Who may call what, and on which projects, is decided shell-side by the MCP policy — this
//! module only describes the tools honestly (`safety = read | write`).

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{
    get_file_blame, get_status, get_workdir_diff, BlameLine, DiffFile, RepoStatus,
};
use git2::{Repository, Sort};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ephemeral::tab_for;
use crate::repo::{open, repo_path};

const DEFAULT_LOG: usize = 20;
const MAX_LOG: usize = 200;
/// Commits examined when filtering the log by path — bounds the walk on a huge history.
const MAX_LOG_SCAN: usize = 20_000;
const DEFAULT_DIFF_CONTEXT: u32 = 3;
const MAX_DIFF_CONTEXT: u32 = 20;
/// Files a single diff answers with. A 400-file patch is not an answer anyone reads, and it
/// would push everything else out of the reply — `path` narrows it instead.
const MAX_DIFF_FILES: usize = 50;

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
    /// root. Added to whatever is already staged, never replacing it. Omit to commit exactly
    /// what is in the index (see `corvus_status` or `corvus_diff`).
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
/// (`git reset --soft HEAD~1`), but it does move the branch: read `corvus_diff` first to know
/// what the commit will say.
///
/// `paths` **adds** to the index, it does not replace it: anything already staged is committed
/// too. To commit only certain files, `corvus_unstage` the rest first.
#[arbor_rpc::handler(mcp(
    title = "Commit staged changes",
    safety = write,
))]
fn corvus_commit(state: &CorvusState, args: CommitArgs) -> Result<CommitResult, String> {
    let message = args.message.trim().to_string();
    if message.is_empty() {
        return Err("The commit message is empty.".to_string());
    }
    let (tab, extra) = resolve_paths(state, &args.root, args.paths.as_deref().unwrap_or(&[]))?;

    if !extra.is_empty() {
        let repo = open(state, &tab)?;
        corvus_git::stage::stage_paths(&repo, &extra).map_err(|e| e.to_string())?;
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

// ---------------------------------------------------------------------------
// Index — what the next commit would contain
// ---------------------------------------------------------------------------

/// Args for [`corvus_stage`] and [`corvus_unstage`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StageArgs {
    /// Absolute path to the repository root, or to any folder or file inside it.
    pub root: String,
    /// The files or folders to move — absolute, or relative to the repository root. A folder
    /// takes everything under it. There is no "everything" shortcut on purpose: name what you
    /// have actually looked at.
    pub paths: Vec<String>,
}

/// The index, as `corvus_stage` and `corvus_unstage` leave it.
#[derive(Debug, Serialize)]
pub struct StageResult {
    /// Repo-relative paths in the index now — exactly what a commit would contain.
    pub staged: Vec<String>,
    /// How many changed files are still outside the index.
    pub unstaged: usize,
}

/// Move files or folders into the index, as the Stage panel does. The working tree is never
/// touched: this only decides what the next commit would contain, and `corvus_unstage` takes
/// it straight back. Returns the whole index afterwards, so what a commit would contain is
/// visible without a second call.
#[arbor_rpc::handler(mcp(
    title = "Stage files",
    safety = write,
))]
fn corvus_stage(state: &CorvusState, args: StageArgs) -> Result<StageResult, String> {
    let (tab, rels) = resolve_paths(state, &args.root, &args.paths)?;
    if rels.is_empty() {
        return Err("Name at least one file or folder to stage.".to_string());
    }
    let repo = open(state, &tab)?;
    corvus_git::stage::stage_paths(&repo, &rels).map_err(|e| e.to_string())?;
    index_summary(&repo)
}

/// Take files or folders back out of the index, as the Stage panel does. The edits themselves
/// survive untouched in the working tree — this undoes a staging decision, it does not discard
/// work, and there is no tool here that does.
#[arbor_rpc::handler(mcp(
    title = "Unstage files",
    safety = write,
))]
fn corvus_unstage(state: &CorvusState, args: StageArgs) -> Result<StageResult, String> {
    let (tab, rels) = resolve_paths(state, &args.root, &args.paths)?;
    if rels.is_empty() {
        return Err("Name at least one file or folder to unstage.".to_string());
    }
    let repo = open(state, &tab)?;
    corvus_git::stage::unstage_paths(&repo, &rels).map_err(|e| e.to_string())?;
    index_summary(&repo)
}

/// The index and what is left outside it.
fn index_summary(repo: &Repository) -> Result<StageResult, String> {
    let status = get_status(repo).map_err(|e| e.to_string())?;
    Ok(StageResult {
        staged: status.staged.iter().map(|e| e.path.clone()).collect(),
        unstaged: status.unstaged.len() + status.untracked.len(),
    })
}

// ---------------------------------------------------------------------------
// Diff — the content of a change, not just its name
// ---------------------------------------------------------------------------

/// Args for [`corvus_diff`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiffArgs {
    /// Absolute path to the repository root, or to any folder or file inside it.
    pub root: String,
    /// `true` reads the index against HEAD — exactly what a commit would contain. `false`,
    /// the default, reads the working tree against the index: the edits not staged yet.
    pub staged: Option<bool>,
    /// One file or folder to narrow to — absolute, or relative to the repository root. Omit
    /// for every changed file.
    pub path: Option<String>,
    /// Unchanged lines kept around each change. Default 3, at most 20.
    pub context_lines: Option<u32>,
}

/// The patch, as `corvus_diff` reports it.
#[derive(Debug, Serialize)]
pub struct DiffResult {
    /// Which side was read: `true` = index vs HEAD, `false` = working tree vs index.
    pub staged: bool,
    pub files: Vec<DiffFile>,
    /// Changed files on that side, before any truncation.
    pub total_files: usize,
    /// `true` when `files` holds only the first `MAX_DIFF_FILES` of them — narrow with `path`
    /// to see the rest.
    pub truncated: bool,
}

/// The actual patch of the uncommitted work: hunks and lines, not just file names. Read this
/// before `corvus_commit` to know what the commit says, and before `corvus_stage` to know
/// which files belong in it.
#[arbor_rpc::handler(mcp(
    title = "Diff the uncommitted work",
    safety = read,
))]
fn corvus_diff(state: &CorvusState, args: DiffArgs) -> Result<DiffResult, String> {
    let staged = args.staged.unwrap_or(false);
    let tab = tab_for(state, &args.root)?;
    let workdir = repo_path(state, &tab)?;
    let narrow = args.path.as_deref().map(|p| relative_to(&workdir, p));
    if let Some(bad) = narrow.as_deref().filter(|p| p.split('/').any(|seg| seg == "..")) {
        return Err(format!("'{bad}' leaves the repository; pass a path inside it."));
    }
    let context = args.context_lines.unwrap_or(DEFAULT_DIFF_CONTEXT).min(MAX_DIFF_CONTEXT);

    let repo = open(state, &tab)?;
    let mut files =
        get_workdir_diff(&repo, staged, context, None, None).map_err(|e| e.to_string())?;
    if let Some(prefix) = narrow.as_deref().filter(|p| !p.is_empty()) {
        files.retain(|f| is_under(&f.path, prefix));
    }
    let total_files = files.len();
    files.truncate(MAX_DIFF_FILES);
    // A base64 PNG is not a patch anyone can read, and two of them would crowd every text
    // change out of the answer. `is_binary` already says the file changed.
    for file in &mut files {
        file.image_old = None;
        file.image_new = None;
    }
    Ok(DiffResult { staged, files, truncated: total_files > MAX_DIFF_FILES, total_files })
}

/// Whether `path` is `prefix` or lives under it — on the repo-relative, forward-slash form,
/// so the boundary is a `/` and `src/lib2` is not inside `src/lib`.
fn is_under(path: &str, prefix: &str) -> bool {
    path == prefix || path.strip_prefix(prefix).is_some_and(|tail| tail.starts_with('/'))
}

// ---------------------------------------------------------------------------
// Push — the one tool whose effect leaves this machine
// ---------------------------------------------------------------------------

/// Args for [`corvus_push`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PushArgs {
    /// Absolute path to the repository root, or to any folder or file inside it.
    pub root: String,
    /// The remote to push to. Defaults to `origin`.
    pub remote: Option<String>,
}

/// What `corvus_push` sent.
#[derive(Debug, Serialize)]
pub struct PushResult {
    pub remote: String,
    pub branch: String,
    /// Commits the remote did not have. `None` on a first push, where there was no
    /// remote-tracking branch to count against.
    pub commits: Option<usize>,
}

/// Push the current branch to a remote: a plain fast-forward push of `refs/heads/<branch>`,
/// and nothing else. Never forces, never pushes another ref, never pushes a tag — a force
/// push rewrites history other people already have, and that decision stays in the UI. The
/// branch must not be behind the remote; when it is, the pull or rebase happens in Arbor.
///
/// This is the one tool here whose effect leaves the machine: a commit is undoable
/// (`git reset --soft HEAD~1`), a push is public.
#[arbor_rpc::handler(mcp(
    title = "Push the current branch",
    safety = destructive,
))]
async fn corvus_push(state: &CorvusState, args: PushArgs) -> Result<PushResult, String> {
    let remote = args.remote.unwrap_or_else(|| "origin".to_string());
    let tab = tab_for(state, &args.root)?;

    // Counted against *this* remote's tracking ref rather than taken from `get_status`, whose
    // ahead/behind is hardcoded to origin.
    let (branch, counts) = {
        let repo = open(state, &tab)?;
        let head = repo.head().map_err(|e| format!("Git error: {e}"))?;
        if !head.is_branch() {
            return Err("HEAD is detached, so there is no branch to push. Check out a branch \
                        first."
                .to_string());
        }
        let branch = head
            .shorthand()
            .ok_or_else(|| "the current branch has no printable name".to_string())?
            .to_string();
        let counts = repo
            .refname_to_id(&format!("refs/remotes/{remote}/{branch}"))
            .ok()
            .zip(head.target())
            .and_then(|(upstream, local)| repo.graph_ahead_behind(local, upstream).ok());
        (branch, counts)
    };

    match counts {
        Some((_, behind)) if behind > 0 => {
            return Err(format!(
                "'{branch}' is {behind} commit(s) behind '{remote}/{branch}', so the remote \
                 would refuse this push. Pulling, merging or rebasing is a decision taken in \
                 Arbor, not here."
            ));
        }
        Some((0, _)) => {
            return Err(format!(
                "'{branch}' has nothing '{remote}' does not already have."
            ));
        }
        _ => {}
    }

    crate::remote::push_branch(state, tab, remote.clone(), format!("refs/heads/{branch}"), false)
        .await?;
    Ok(PushResult { remote, branch, commits: counts.map(|(ahead, _)| ahead) })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// `root` to an ephemeral tab, and `paths` to repo-relative paths, refusing any that leaves
/// the repository.
///
/// The refusal lives here rather than in the shell's policy because only this side knows the
/// repository root: the policy sees **absolute** paths in the arguments and checks those, so a
/// relative one is the single way to name a file outside the project it was checked against.
fn resolve_paths(
    state: &CorvusState,
    root: &str,
    paths: &[String],
) -> Result<(String, Vec<String>), String> {
    let tab = tab_for(state, root)?;
    let workdir = repo_path(state, &tab)?;
    let rels: Vec<String> = paths.iter().map(|p| relative_to(&workdir, p)).collect();
    if let Some(bad) = rels.iter().find(|p| p.split('/').any(|seg| seg == "..")) {
        return Err(format!("'{bad}' leaves the repository; pass a path inside it."));
    }
    Ok((tab, rels))
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
    fn is_under_respects_the_path_boundary() {
        assert!(is_under("src/lib/a.rs", "src/lib"));
        assert!(is_under("src/lib", "src/lib"), "the folder itself");
        // A sibling that merely shares the prefix is not inside it.
        assert!(!is_under("src/lib2/a.rs", "src/lib"));
        assert!(!is_under("src", "src/lib"));
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
