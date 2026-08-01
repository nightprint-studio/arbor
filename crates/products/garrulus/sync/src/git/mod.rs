//! [`GitRemote`] — the real implementation, over `corvus-git`.
//!
//! The vault is a git repository, and that is the whole trick: it is invisible
//! (`docs/garrulus-design.md` §4.2). Garrulus never shows a branch, a commit or
//! a staging area unless the user goes looking.
//!
//! ## What runs when
//!
//! * [`SyncRemote::probe`] fetches and counts. **Read-only** — it never
//!   commits, pushes, pulls or touches the working tree. It is the only method
//!   the background timer calls.
//! * [`SyncRemote::push`] stages the batch, makes **one** commit authored
//!   `Garrulus (<device>)`, and pushes.
//! * [`SyncRemote::pull`] fast-forwards when it can and otherwise merges,
//!   resolving every conflicted note itself so that **no merge marker ever
//!   reaches a `.md`**: the note keeps the local text, the remote text goes in a
//!   side file, and both are reported.
//!
//! ## Why two paths into git
//!
//! Network operations go through `corvus_git::remote::{fetch, push}` because
//! that is where the injected credential resolver lives — the same closure the
//! shell's keyring and `corvus-be`'s reverse channel already implement. Local
//! plumbing (staging, committing, merging, `rev-list`) goes through
//! `corvus_git::prelude::GitCli`, because `corvus-git` has **no commit function
//! at all** and no repo-handle-based staging: the only commit in the workspace
//! is inlined in `corvus-be`'s `stage.rs` handler, and it commits with
//! `repo.signature()` — the user's git identity — which is exactly what an
//! automatic vault commit must not use. `GitCli` also gives us `-c user.name` /
//! `-c user.email` per invocation, so the device authorship never touches the
//! user's global config.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;
// Through the prelude, the workspace's canonical entry point for a library
// crate. `fetch` / `push` are renamed at the import so a bare `push(...)` in
// this file cannot be mistaken for `SyncRemote::push`.
use corvus_git::prelude::{
    fetch as git_fetch, get_file_at_commit, get_status, list_remotes, push as git_push, GitCli,
    GitRepo,
};

use crate::change::{auto_commit_message, commit_identity, parse_name_status, ChangeBatch, RelPath};
use crate::conflict::is_side_file;
use crate::error::{SyncError, SyncResult};
use crate::files::walk_notes;
use crate::remote::{
    PullOutcome, RemoteCapabilities, RemoteDescriptor, RemoteKind, Revision, SyncRemote,
};
use crate::run_blocking;
use crate::state::{classify, StateInputs, SyncState};

// Pulling is its own concern and its own file: it is where the conflict rules
// are actually enforced, and it is the code that gets re-read the most.
mod pull;

/// Maps a remote URL to `(username, password)`.
///
/// Owned and `Send + Sync` so the blocking half of a sync can carry it onto a
/// worker thread; `corvus-git` wants a borrowed `dyn Fn`, which this hands it at
/// the call site. The shell binds it to the OS keyring, `garrulus-be` to the
/// reverse channel — this crate never learns which.
pub type CredentialProvider =
    Arc<dyn Fn(&str) -> Result<Option<(String, String)>, String> + Send + Sync>;

/// A vault whose remote is a git repository.
#[derive(Clone)]
pub struct GitRemote {
    vault: PathBuf,
    remote: String,
    branch: Option<String>,
    device: String,
    git: GitCli,
    creds: CredentialProvider,
    daily_folder: Option<String>,
}

impl fmt::Debug for GitRemote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitRemote")
            .field("vault", &self.vault)
            .field("remote", &self.remote)
            .field("branch", &self.branch)
            .field("device", &self.device)
            .field("daily_folder", &self.daily_folder)
            .finish_non_exhaustive()
    }
}

/// The result of a git invocation that is allowed to fail.
#[derive(Debug)]
struct GitOut {
    ok: bool,
    stdout: String,
    stderr: String,
}

impl GitRemote {
    /// A git-backed remote for `vault`, talking to the git remote named
    /// `remote` (conventionally `origin`), committing as `device`.
    pub fn new(
        vault: impl Into<PathBuf>,
        remote: impl Into<String>,
        device: impl Into<String>,
        git: GitCli,
        creds: CredentialProvider,
    ) -> Self {
        Self {
            vault: vault.into(),
            remote: remote.into(),
            branch: None,
            device: device.into(),
            git,
            creds,
            daily_folder: None,
        }
    }

    /// Pin the branch instead of following HEAD.
    pub fn with_branch(mut self, branch: Option<String>) -> Self {
        self.branch = branch;
        self
    }

    /// Tell the engine which folder holds daily notes, so those append-merge
    /// instead of conflicting (§4.4.5).
    pub fn with_daily_folder(mut self, folder: Option<String>) -> Self {
        self.daily_folder = folder;
        self
    }

    // -- git plumbing --------------------------------------------------------

    fn open(&self) -> SyncResult<GitRepo> {
        GitRepo::open(&self.vault.to_string_lossy())
            .map_err(|e| SyncError::Git(format!("vault is not a git repository: {e}")))
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = self.git.command();
        cmd.current_dir(&self.vault);
        cmd.args(args);
        cmd
    }

    /// Run git, failing on a non-zero exit.
    fn run(&self, args: &[&str]) -> SyncResult<String> {
        let out = self.try_run(args)?;
        if out.ok {
            Ok(out.stdout)
        } else {
            Err(SyncError::from_git_message(format!(
                "git {}: {}",
                args.join(" "),
                out.stderr.trim()
            )))
        }
    }

    /// Run git, letting the caller decide what a non-zero exit means (a merge
    /// that conflicted is not a failure, it is the interesting case).
    fn try_run(&self, args: &[&str]) -> SyncResult<GitOut> {
        let out = self.command(args).output()?;
        Ok(GitOut {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        })
    }

    /// Full hex oid of a revision, or `None` when it does not resolve.
    fn rev(&self, refname: &str) -> Option<String> {
        let out = self.try_run(&["rev-parse", "--verify", "--quiet", refname]).ok()?;
        let id = out.stdout.trim().to_string();
        (out.ok && !id.is_empty()).then_some(id)
    }

    /// Which branch this vault syncs.
    fn branch_name(&self, repo: &GitRepo) -> SyncResult<String> {
        if let Some(b) = &self.branch {
            return Ok(b.clone());
        }
        repo.current_branch()
            .filter(|b| b.as_str() != "(detached)")
            .ok_or_else(|| SyncError::Git("the vault has no checked-out branch".into()))
    }

    fn upstream_ref(&self, branch: &str) -> String {
        format!("refs/remotes/{}/{}", self.remote, branch)
    }

    fn has_remote(&self, repo: &GitRepo) -> bool {
        list_remotes(repo.inner())
            .map(|remotes| remotes.iter().any(|r| r.name == self.remote))
            .unwrap_or(false)
    }

    /// Fetch — the one network operation the background is allowed to do.
    fn fetch(&self, repo: &GitRepo) -> SyncResult<()> {
        let creds = self.creds.clone();
        let resolver = move |url: &str| (*creds)(url);
        git_fetch(repo.inner(), &self.remote, &resolver)
            .map(|_| ())
            .map_err(|e| SyncError::from_git_message(e.to_string()))
    }

    fn merge_base(&self, a: &str, b: &str) -> Option<String> {
        let out = self.try_run(&["merge-base", a, b]).ok()?;
        let id = out.stdout.trim().to_string();
        (out.ok && !id.is_empty()).then_some(id)
    }

    fn changed_paths(&self, from: &str, to: &str) -> SyncResult<Vec<RelPath>> {
        let out = self.run(&["diff", "--name-only", from, to])?;
        Ok(path_lines(&out))
    }

    fn conflicted_paths(&self) -> SyncResult<Vec<RelPath>> {
        let out = self.run(&["diff", "--name-only", "--diff-filter=U"])?;
        Ok(path_lines(&out))
    }

    fn in_merge(&self) -> bool {
        self.rev("MERGE_HEAD").is_some()
    }

    fn file_at(&self, repo: &GitRepo, oid: &str, rel: &RelPath) -> Option<String> {
        get_file_at_commit(repo.inner(), oid, rel.as_str(), None).ok()
    }

    /// Commit what is staged, authored as the device.
    ///
    /// `-c user.name` / `-c user.email` per invocation: the user's global git
    /// identity is never read and never written (`corvus-git` offers no
    /// commit-with-explicit-author, and mutating global config to get one would
    /// be unacceptable).
    fn commit_as_device(&self, message: &str) -> SyncResult<()> {
        let (name, email) = commit_identity(&self.device);
        let name_arg = format!("user.name={name}");
        let email_arg = format!("user.email={email}");
        self.run(&[
            "-c",
            name_arg.as_str(),
            "-c",
            email_arg.as_str(),
            "commit",
            "--no-edit",
            "-m",
            message,
        ])?;
        Ok(())
    }

    /// Conflict side files still sitting in the vault, unresolved.
    fn open_conflicts(&self) -> u32 {
        walk_notes(&self.vault)
            .map(|notes| notes.iter().filter(|p| is_side_file(p)).count() as u32)
            .unwrap_or(0)
    }

    // -- the operations ------------------------------------------------------

    fn probe_blocking(&self) -> SyncResult<SyncState> {
        let repo = self.open()?;
        if !self.has_remote(&repo) {
            return Ok(SyncState::NoRemote);
        }
        let branch = self.branch_name(&repo)?;
        let reachable = match self.fetch(&repo) {
            Ok(()) => true,
            Err(SyncError::Offline(_)) => false,
            Err(e) => return Err(e),
        };
        let status = get_status(repo.inner())
            .map_err(|e| SyncError::Git(e.to_string()))?;
        let dirty = (status.staged.len() + status.unstaged.len() + status.untracked.len()) as u32;
        let (ahead, behind) = if reachable {
            self.ahead_behind(&branch)?
        } else {
            (0, 0)
        };
        Ok(classify(StateInputs {
            has_remote: true,
            reachable,
            dirty_notes: dirty,
            conflicts: self.open_conflicts() + status.conflicted.len() as u32,
            ahead_commits: ahead,
            behind_commits: behind,
        }))
    }

    /// Commits ahead of / behind the upstream.
    ///
    /// Computed here rather than read from `corvus_git::status::get_status`:
    /// that one hardcodes `refs/remotes/origin/<branch>` and silently reports
    /// `(0, 0)` for any other remote — see the crate's own divergence with
    /// `branch::list_local_branches`.
    fn ahead_behind(&self, branch: &str) -> SyncResult<(u32, u32)> {
        let upstream = self.upstream_ref(branch);
        match (self.rev("HEAD"), self.rev(&upstream)) {
            (None, _) => Ok((0, 0)), // empty repository
            (Some(_), None) => {
                // Never pushed: everything local is outgoing.
                let out = self.run(&["rev-list", "--count", "HEAD"])?;
                Ok((out.trim().parse().unwrap_or(0), 0))
            }
            (Some(local), Some(up)) => {
                let range = format!("{up}...{local}");
                let out = self.run(&["rev-list", "--left-right", "--count", range.as_str()])?;
                Ok(parse_left_right(&out).unwrap_or((0, 0)))
            }
        }
    }

    fn push_blocking(&self, batch: &ChangeBatch) -> SyncResult<()> {
        let repo = self.open()?;
        if !self.has_remote(&repo) {
            return Err(SyncError::NotConfigured(format!("no remote named '{}'", self.remote)));
        }
        let branch = self.branch_name(&repo)?;

        // An EMPTY batch means "everything the user has changed", not "whatever
        // happens to be staged" — that is the meaning `FolderRemote` already gives
        // it, and the only one that makes the sync button work: nothing else in
        // this product ever runs `git add`, so reading it the other way made the
        // main action pull, commit nothing, and push an unchanged HEAD.
        if batch.is_empty() {
            self.run(&["add", "-A", "--", "."])?;
        } else {
            let mut args: Vec<&str> = vec!["add", "--"];
            args.extend(batch.notes.iter().map(|p| p.as_str()));
            self.run(&args)?;
        }
        let staged = self.run(&["diff", "--cached", "--name-status"])?;
        let changes = parse_name_status(&staged);
        if !changes.is_empty() {
            let message = batch
                .message
                .clone()
                .unwrap_or_else(|| auto_commit_message(&changes));
            self.commit_as_device(&message)?;
        }

        let creds = self.creds.clone();
        let resolver = move |url: &str| (*creds)(url);
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        git_push(repo.inner(), &self.remote, &refspec, false, &resolver)
            .map_err(|e| SyncError::from_git_message(e.to_string()))
    }

    fn history_blocking(&self, note: &RelPath) -> SyncResult<Vec<Revision>> {
        self.open()?;
        let out = self.run(&[
            "log",
            "--follow",
            "--format=%H%x1f%an%x1f%at%x1f%s",
            "--",
            note.as_str(),
        ])?;
        Ok(parse_log(&out))
    }

    fn revision_blocking(&self, note: &RelPath, rev: &str) -> SyncResult<String> {
        let repo = self.open()?;
        let oid = self
            .rev(rev)
            .ok_or_else(|| SyncError::Git(format!("unknown revision '{rev}'")))?;
        get_file_at_commit(repo.inner(), &oid, note.as_str(), None)
            .map_err(|e| SyncError::Git(e.to_string()))
    }
}

#[async_trait]
impl SyncRemote for GitRemote {
    fn descriptor(&self) -> RemoteDescriptor {
        RemoteDescriptor {
            id: self.remote.clone(),
            kind: RemoteKind::Git,
            display: self.remote.clone(),
            capabilities: RemoteCapabilities {
                history: true,
                atomic_batch: true,
                conflicts: true,
            },
        }
    }

    async fn probe(&self) -> SyncResult<SyncState> {
        let me = self.clone();
        run_blocking(move || me.probe_blocking()).await
    }

    async fn pull(&self, _vault: &Path) -> SyncResult<PullOutcome> {
        let me = self.clone();
        run_blocking(move || me.pull_blocking()).await
    }

    async fn push(&self, _vault: &Path, batch: &ChangeBatch) -> SyncResult<()> {
        let me = self.clone();
        let batch = batch.clone();
        run_blocking(move || me.push_blocking(&batch)).await
    }

    async fn history(&self, _vault: &Path, note: &RelPath) -> SyncResult<Vec<Revision>> {
        let me = self.clone();
        let note = note.clone();
        run_blocking(move || me.history_blocking(&note)).await
    }

    async fn revision(&self, _vault: &Path, note: &RelPath, rev: &str) -> SyncResult<String> {
        let me = self.clone();
        let note = note.clone();
        let rev = rev.to_string();
        run_blocking(move || me.revision_blocking(&note, &rev)).await
    }
}

/// One path per line, blanks dropped.
fn path_lines(out: &str) -> Vec<RelPath> {
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(RelPath::new)
        .collect()
}

/// Parse `git rev-list --left-right --count <up>...<local>`, which prints
/// `behind<TAB>ahead` — left is the upstream side.
pub fn parse_left_right(out: &str) -> Option<(u32, u32)> {
    let mut parts = out.split_whitespace();
    let behind: u32 = parts.next()?.parse().ok()?;
    let ahead: u32 = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

/// Parse the `%H%x1f%an%x1f%at%x1f%s` log format into revisions.
///
/// Unit separators rather than a delimiter a commit subject could contain: a
/// note titled `bug: crash | avvio` must not shear the parse.
pub fn parse_log(out: &str) -> Vec<Revision> {
    let mut revisions = Vec::new();
    for line in out.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split('\u{1f}');
        let (Some(id), Some(author), Some(ts), Some(summary)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        revisions.push(Revision {
            id: id.trim().to_string(),
            author: author.to_string(),
            timestamp: ts.trim().parse().unwrap_or(0),
            summary: summary.to_string(),
        });
    }
    revisions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_right_is_behind_then_ahead() {
        assert_eq!(parse_left_right("2\t3\n"), Some((3, 2)));
        assert_eq!(parse_left_right("0\t0"), Some((0, 0)));
        assert_eq!(parse_left_right(""), None);
        assert_eq!(parse_left_right("boh"), None);
    }

    #[test]
    fn log_lines_survive_separators_in_the_subject() {
        let out = "abc123\u{1f}Garrulus (casa)\u{1f}1767225600\u{1f}Nuova nota: bug | avvio\n";
        let revs = parse_log(out);
        assert_eq!(revs.len(), 1);
        assert_eq!(revs[0].id, "abc123");
        assert_eq!(revs[0].author, "Garrulus (casa)");
        assert_eq!(revs[0].timestamp, 1_767_225_600);
        assert_eq!(revs[0].summary, "Nuova nota: bug | avvio");
    }

    #[test]
    fn malformed_log_lines_are_dropped_not_guessed() {
        assert!(parse_log("solo un pezzo\n").is_empty());
    }

    #[test]
    fn path_lines_normalise_and_drop_blanks() {
        let paths = path_lines("bugs/crash.md\n\n diario/2026-07-31.md \n");
        assert_eq!(
            paths,
            vec![RelPath::new("bugs/crash.md"), RelPath::new("diario/2026-07-31.md")]
        );
    }
}
