//! Recovery journal — snapshot-based safety net for destructive git operations.
//!
//! Before running any operation that can destroy uncommitted work (reset --hard,
//! discard, checkout with dirty workdir, stash apply with conflicts, etc.), a
//! snapshot of the working tree + index + HEAD is captured as an unreachable
//! git object and a JSON entry is appended to `.git/arbor-recovery/journal.jsonl`.
//!
//! Snapshots use the `git stash create` mechanism (which produces a commit
//! containing a tree of the working directory + a parent tree for the index)
//! but do NOT update `refs/stash` — the commits are kept alive via a dedicated
//! ref namespace `refs/arbor/recovery/<timestamp>-<kind>` so they survive
//! garbage collection until the TTL expires.
//!
//! Users can browse these snapshots in the ReflogPanel (Recovery tab) and
//! restore them with a single click.  Expired snapshots are removed lazily on
//! every successful journal read.
//!
//! ## Tauri-free, no global state, no config coupling
//!
//! Git invocation goes through the explicit [`GitCli`] (the shell and
//! `corvus-be` each pass their own) and the [`SnapshotPolicy`] / retention is
//! passed in by the caller — the shell loads it from its config and forwards
//! it, so `recovery` here drags in neither `git_cli` globals nor the app config.
//!
//! Design choices:
//! - The journal is append-only JSONL so partial writes never corrupt older
//!   entries, and parsing can tolerate individual line failures.
//! - Snapshots are keyed by unique monotonically-increasing IDs, not array
//!   indices, so rewriting the journal on eviction does not break outstanding
//!   UI references.
//! - Failure to write the journal is logged but never propagated — a snapshot
//!   attempt must never prevent the user-requested operation from running.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::cli::GitCli;
use crate::error::{GitError, Result};

// ---------------------------------------------------------------------------
// Snapshot policy — dictates which files get their content preserved vs. only
// a journal log entry.
// ---------------------------------------------------------------------------

/// Default per-file size limit for content preservation (2 MB).
pub const DEFAULT_MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

/// Default retention window — snapshots older than this are pruned at the
/// next journal read.  Matches git's own `gc.reflogExpireUnreachable`.
pub const DEFAULT_RETENTION_DAYS: u32 = 30;

/// Extensions that are never preserved (only logged) — large binaries and
/// build artifacts that would bloat the snapshot store without adding value.
/// The user can still see in the journal that they were there at snapshot
/// time — they just cannot be restored via the Recovery tab.
pub const DEFAULT_DENY_EXTENSIONS: &[&str] = &[
    // Archives
    "zip", "tar", "tgz", "gz", "bz2", "xz", "7z", "rar",
    // Video / audio
    "mp4", "mov", "mkv", "avi", "webm", "mp3", "wav", "flac",
    // Large images
    "psd", "ai", "tiff",
    // Binaries / artifacts
    "exe", "dll", "so", "dylib", "a", "lib", "obj", "o",
    "jar", "war", "class", "pdb", "wasm",
    // Build outputs
    "iso", "dmg", "pkg", "deb", "rpm",
    // ML / data blobs
    "onnx", "pt", "pth", "ckpt", "h5", "parquet", "safetensors",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPolicy {
    /// Files larger than this (in bytes) are logged but their content is NOT
    /// preserved — the entry carries `restorable=false` for that path.
    pub max_file_size: u64,
    /// Case-insensitive extension deny-list (no leading dot).  Files whose
    /// extension matches are logged but not preserved.
    pub deny_extensions: Vec<String>,
    /// How many days of history to keep.  Zero means "never auto-expire" —
    /// only the MAX_ENTRIES cap applies in that case.
    pub retention_days: u32,
}

impl Default for SnapshotPolicy {
    fn default() -> Self {
        Self {
            max_file_size:   DEFAULT_MAX_FILE_SIZE,
            deny_extensions: DEFAULT_DENY_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
            retention_days:  DEFAULT_RETENTION_DAYS,
        }
    }
}

impl SnapshotPolicy {
    /// Decide whether a single file should have its content preserved.  Returns
    /// `None` when preservation is allowed; `Some(reason)` with a short
    /// user-facing reason when the file is excluded.
    pub fn exclude_reason(&self, path: &str, size: u64) -> Option<String> {
        if size > self.max_file_size {
            return Some(format!(
                "size {} > limit {}", human_size(size), human_size(self.max_file_size)
            ));
        }
        if let Some(ext) = path.rsplit('.').next() {
            if path.contains('.') {
                let low = ext.to_ascii_lowercase();
                if self.deny_extensions.iter().any(|d| d.eq_ignore_ascii_case(&low)) {
                    return Some(format!("extension .{low} is in deny-list"));
                }
            }
        }
        None
    }
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB { format!("{:.1} GB", bytes as f64 / GB as f64) }
    else if bytes >= MB { format!("{:.1} MB", bytes as f64 / MB as f64) }
    else if bytes >= KB { format!("{:.1} KB", bytes as f64 / KB as f64) }
    else { format!("{bytes} B") }
}

/// Log-only record for a file that was seen at snapshot time but excluded
/// from content preservation (too large or denied extension).  Present in
/// the journal so the user can still tell *what* was lost if they need to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedFile {
    pub path: String,
    /// Size in bytes at snapshot time.
    pub size: u64,
    /// Short reason shown in the UI (e.g. "size 12.4 MB > limit 2.0 MB").
    pub reason: String,
    /// True for files under version control; false for untracked.
    pub tracked: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind {
    /// Before `reset --hard`.
    ResetHard,
    /// Before a branch checkout that would overwrite dirty workdir.
    Checkout,
    /// Before `discard_file` / `discard_all`.
    Discard,
    /// Before `force_stash_apply` overwrites workdir files with the stash's
    /// untracked-tree blobs to satisfy the user's "use stash" decisions.
    StashForceApply,
    /// Before `stash_drop` on a non-empty stash entry.
    StashDrop,
    /// Before `pull_branch` — covers the stash/merge/fast-forward trio.
    /// Gives us a full-tree rollback point even if an untracked file is
    /// clobbered by the checkout or the stash re-apply produces conflicts
    /// that the user ends up aborting.
    Pull,
    /// Generic fallback used by operations that wrap a snapshot as a safety net.
    Other,
}

impl RecoveryKind {
    fn slug(&self) -> &'static str {
        match self {
            RecoveryKind::ResetHard        => "reset-hard",
            RecoveryKind::Checkout         => "checkout",
            RecoveryKind::Discard          => "discard",
            RecoveryKind::StashForceApply  => "stash-force-apply",
            RecoveryKind::StashDrop        => "stash-drop",
            RecoveryKind::Pull             => "pull",
            RecoveryKind::Other            => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryEntry {
    /// Monotonically-increasing ID (persisted in the journal).
    pub id: u64,
    /// Unix timestamp (seconds) at which the snapshot was taken.
    pub created_at: i64,
    /// Operation that triggered the snapshot.
    pub kind: RecoveryKind,
    /// Short human-readable description (e.g. "reset --hard to abc1234").
    pub summary: String,
    /// OID of the stash-style snapshot commit.
    pub snapshot_oid: String,
    /// HEAD at the time of the snapshot (so restore can optionally reset back).
    pub head_oid: Option<String>,
    /// Canonical branch name at snapshot time, if HEAD was on a branch.
    pub head_branch: Option<String>,
    /// Full ref holding the snapshot alive (`refs/arbor/recovery/<slug>`).
    pub ref_name: String,
    /// True once the user has restored or explicitly discarded this entry.
    #[serde(default)]
    pub consumed: bool,
    /// Files observed at snapshot time that were NOT preserved (too large or
    /// matched a denied extension).  They are logged here so the user can
    /// still tell what would have been lost, even though the Recovery tab
    /// cannot bring them back.
    #[serde(default)]
    pub skipped_files: Vec<SkippedFile>,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Max entries retained regardless of age — keeps the journal file bounded
/// even under pathological usage.  Applies even when `retention_days == 0`.
const MAX_ENTRIES: usize = 500;

fn journal_path(git_dir: &Path) -> PathBuf {
    git_dir.join("arbor-recovery").join("journal.jsonl")
}

fn ref_name(kind: RecoveryKind, id: u64) -> String {
    // Keep the ref path stable and grep-friendly.
    format!("refs/arbor/recovery/{}-{}", id, kind.slug())
}

// ---------------------------------------------------------------------------
// Journal I/O
// ---------------------------------------------------------------------------

fn read_all_entries(git_dir: &Path) -> Vec<RecoveryEntry> {
    let path = journal_path(git_dir);
    let file = match File::open(&path) {
        Ok(f)  => f,
        Err(_) => return Vec::new(),
    };
    BufReader::new(file)
        .lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<RecoveryEntry>(&line).ok())
        .collect()
}

fn write_all_entries(git_dir: &Path, entries: &[RecoveryEntry]) -> std::io::Result<()> {
    let path   = journal_path(git_dir);
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;

    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut f = File::create(&tmp)?;
        for e in entries {
            let line = serde_json::to_string(e).unwrap_or_default();
            f.write_all(line.as_bytes())?;
            f.write_all(b"\n")?;
        }
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &path)
}

fn append_entry(git_dir: &Path, entry: &RecoveryEntry) -> std::io::Result<()> {
    let path   = journal_path(git_dir);
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;

    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    let line  = serde_json::to_string(entry).unwrap_or_default();
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// Next monotonic ID — max existing + 1.  Unused IDs from pruned entries are
/// not reclaimed so UI caches in the frontend never see an ID collision.
fn next_id(git_dir: &Path) -> u64 {
    read_all_entries(git_dir)
        .iter()
        .map(|e| e.id)
        .max()
        .unwrap_or(0) + 1
}

// ---------------------------------------------------------------------------
// Snapshot creation
// ---------------------------------------------------------------------------

/// Capture a snapshot of the current working tree + index, pin it under a
/// dedicated ref, and record the operation in the journal.
///
/// Returns `Ok(None)` when there is nothing worth snapshotting (clean tree
/// and no untracked files) — the caller can skip the confirmation prompt.
///
/// Files that exceed `policy.max_file_size` or match `policy.deny_extensions`
/// are NOT preserved in the snapshot but ARE recorded in the journal so the
/// user can see what was there at the time.  Excluded files are identified
/// via a pathspec passed to `git stash create -- :(exclude)...` so the
/// resulting snapshot commit never contains their bytes.
///
/// Failures are propagated only if they would leave the repository in an
/// unexpected state; purely cosmetic errors (journal write) are logged and
/// swallowed since a failed snapshot must never block the user's operation.
pub fn snapshot_with_policy(
    git:     &GitCli,
    repo:    &Repository,
    kind:    RecoveryKind,
    summary: impl Into<String>,
    policy:  &SnapshotPolicy,
) -> Result<Option<RecoveryEntry>> {
    let workdir = repo.workdir()
        .ok_or_else(|| GitError::Other("bare repository: no workdir to snapshot".into()))?
        .to_path_buf();
    let git_dir = repo.path().to_path_buf();

    // Walk the status list and split files into (dirty-to-keep, dirty-to-skip).
    // Skipped files are pathspec-excluded from `git stash create` and logged
    // separately so the user can audit what was not preserved.
    let mut any_dirty = false;
    let mut skipped: Vec<SkippedFile> = Vec::new();
    let mut exclude_paths: Vec<String> = Vec::new();

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        for st in statuses.iter() {
            let bits = st.status();
            if bits.is_empty() || bits.contains(git2::Status::CURRENT) || bits.contains(git2::Status::IGNORED) {
                continue;
            }
            any_dirty = true;

            let Some(path) = st.path() else { continue };

            // Untracked directories report a single entry for the dir itself
            // when recurse=false, but we asked for recurse=true so each file
            // gets its own entry.  Still, skip if the entry refers to a
            // deleted file that no longer exists on disk.
            let abs = workdir.join(path);
            let size = abs.metadata().map(|m| m.len()).unwrap_or(0);
            let tracked = !bits.contains(git2::Status::WT_NEW);

            if let Some(reason) = policy.exclude_reason(path, size) {
                skipped.push(SkippedFile {
                    path: path.to_string(),
                    size,
                    reason,
                    tracked,
                });
                exclude_paths.push(path.to_string());
            }
        }
    } else {
        // Fail open — we couldn't enumerate statuses, so fall back to
        // snapshotting everything (no exclusions) to stay safe.
        any_dirty = true;
    }

    if !any_dirty {
        return Ok(None);
    }

    // Build the pathspec argument list for `git stash create`.  Each excluded
    // path is passed as `:(exclude,literal)<path>` so it is matched verbatim
    // (no glob interpretation) and stripped from the resulting stash tree.
    // If nothing is excluded, run the plain `git stash create --include-untracked`.
    let mut args: Vec<String> = vec![
        "stash".into(), "create".into(), "--include-untracked".into(),
    ];
    if !exclude_paths.is_empty() {
        // Positional pathspec after `--` so git knows it is not a rev.
        // We pass "." first to include everything by default, then each exclude pattern.
        args.push("--".into());
        args.push(".".into());
        for p in &exclude_paths {
            args.push(format!(":(exclude,literal){p}"));
        }
    }

    let create_out = git.command()
        .args(&args)
        .current_dir(&workdir)
        .output()
        .map_err(|e| GitError::Other(format!("stash create failed to spawn: {e}")))?;

    if !create_out.status.success() {
        let stderr = String::from_utf8_lossy(&create_out.stderr);
        return Err(GitError::Other(format!("stash create failed: {stderr}")));
    }

    let snapshot_oid = String::from_utf8_lossy(&create_out.stdout).trim().to_string();
    if snapshot_oid.is_empty() {
        // Empty stdout means git decided there was nothing to stash.  This can
        // happen when every dirty file was excluded by the policy — still record
        // a journal entry (without snapshot_oid pinning) so the UI can tell the
        // user that their work was untracked AND too large to preserve.
        if skipped.is_empty() {
            return Ok(None);
        }
        let id = next_id(&git_dir);
        let entry = RecoveryEntry {
            id,
            created_at: chrono_now(),
            kind,
            summary: summary.into(),
            snapshot_oid: String::new(),
            head_oid: None,
            head_branch: None,
            ref_name: String::new(),
            consumed: true, // nothing to restore — mark as consumed
            skipped_files: skipped,
        };
        if let Err(e) = append_entry(&git_dir, &entry) {
            tracing::warn!("recovery journal append failed (log-only): {e}");
        }
        return Ok(Some(entry));
    }

    // HEAD context — used by the restore UI to show "HEAD was at …" and to
    // offer a "reset to that state" option.
    let (head_oid, head_branch) = {
        let oid = repo.head().ok().and_then(|h| h.target()).map(|o| o.to_string());
        let branch = repo.head().ok().and_then(|h| {
            if h.is_branch() { h.shorthand().map(String::from) } else { None }
        });
        (oid, branch)
    };

    let id       = next_id(&git_dir);
    let ref_name = ref_name(kind, id);

    // Pin the snapshot under refs/arbor/recovery/… so git gc cannot reclaim it.
    let update_ref_out = git.command()
        .args(["update-ref", &ref_name, &snapshot_oid])
        .current_dir(&workdir)
        .output()
        .map_err(|e| GitError::Other(format!("update-ref failed to spawn: {e}")))?;

    if !update_ref_out.status.success() {
        let stderr = String::from_utf8_lossy(&update_ref_out.stderr);
        return Err(GitError::Other(format!("update-ref failed: {stderr}")));
    }

    let entry = RecoveryEntry {
        id,
        created_at: chrono_now(),
        kind,
        summary:     summary.into(),
        snapshot_oid,
        head_oid,
        head_branch,
        ref_name,
        consumed:     false,
        skipped_files: skipped,
    };

    if let Err(e) = append_entry(&git_dir, &entry) {
        tracing::warn!("recovery journal append failed: {e}");
    }

    Ok(Some(entry))
}

// ---------------------------------------------------------------------------
// Journal listing + pruning
// ---------------------------------------------------------------------------

/// List all known recovery entries, newest first.  Also prunes entries that
/// are past `retention_days` (0 = never expire) or exceed the MAX_ENTRIES cap.
pub fn list_entries(git: &GitCli, repo: &Repository, retention_days: u32) -> Result<Vec<RecoveryEntry>> {
    let git_dir = repo.path().to_path_buf();
    let mut entries = read_all_entries(&git_dir);

    // Newest first so pruning keeps the most relevant ones when over-capacity.
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let ttl_secs: Option<i64> = if retention_days == 0 {
        None
    } else {
        Some(retention_days as i64 * 86_400)
    };

    let now    = chrono_now();
    let mut kept: Vec<RecoveryEntry> = Vec::with_capacity(entries.len());
    let mut to_unref: Vec<String> = Vec::new();
    for (i, e) in entries.into_iter().enumerate() {
        let expired = ttl_secs.map_or(false, |ttl| (now - e.created_at) > ttl);
        let over    = i >= MAX_ENTRIES;
        if expired || over {
            to_unref.push(e.ref_name.clone());
        } else {
            kept.push(e);
        }
    }

    // Drop the refs of pruned entries so their snapshot commits become
    // reachable only from reflog and can eventually be GC'd.  Log-only
    // entries (empty ref_name) have no ref to delete — skip them.
    if !to_unref.is_empty() {
        if let Some(workdir) = repo.workdir() {
            for rn in to_unref.iter().filter(|r| !r.is_empty()) {
                let _ = git.command()
                    .args(["update-ref", "-d", rn])
                    .current_dir(workdir)
                    .output();
            }
        }
        // Rewrite the journal without the pruned entries.
        if let Err(e) = write_all_entries(&git_dir, &kept) {
            tracing::warn!("recovery journal rewrite failed: {e}");
        }
    }

    Ok(kept)
}

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

/// Describe what a restore would do without actually performing it — lets the
/// UI render a preview ("will overwrite 3 modified files, add 2 untracked").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePreview {
    pub changed_files: Vec<String>,
    pub workdir_is_dirty: bool,
}

pub fn preview_restore(git: &GitCli, repo: &Repository, entry_id: u64, retention_days: u32) -> Result<RestorePreview> {
    let entries = list_entries(git, repo, retention_days)?;
    let entry   = entries.iter().find(|e| e.id == entry_id)
        .ok_or_else(|| GitError::Other(format!("recovery entry {entry_id} not found")))?;

    // Log-only entries have no snapshot commit to diff against — refuse early
    // so the UI can show the "not restorable" banner instead of a stack trace.
    if entry.snapshot_oid.is_empty() {
        return Err(GitError::Other(
            "this entry is log-only (excluded by snapshot policy) — cannot be restored".into()
        ));
    }

    let oid = git2::Oid::from_str(&entry.snapshot_oid)
        .map_err(|_| GitError::Other("invalid snapshot oid in journal".into()))?;
    let snapshot_commit = repo.find_commit(oid)?;
    let snapshot_tree   = snapshot_commit.tree()?;

    let head_tree = repo.head().ok()
        .and_then(|h| h.peel_to_commit().ok())
        .and_then(|c| c.tree().ok());

    let diff = match head_tree {
        Some(ref t) => repo.diff_tree_to_tree(Some(t), Some(&snapshot_tree), None)?,
        None        => repo.diff_tree_to_tree(None,     Some(&snapshot_tree), None)?,
    };

    let mut files: Vec<String> = Vec::new();
    for delta in diff.deltas() {
        if let Some(p) = delta.new_file().path().and_then(|p| p.to_str()) {
            files.push(p.to_string());
        } else if let Some(p) = delta.old_file().path().and_then(|p| p.to_str()) {
            files.push(p.to_string());
        }
    }

    let workdir_is_dirty = {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        repo.statuses(Some(&mut opts))
            .map(|s| s.iter().any(|st| {
                let b = st.status();
                !b.is_empty() && !b.contains(git2::Status::CURRENT) && !b.contains(git2::Status::IGNORED)
            }))
            .unwrap_or(false)
    };

    Ok(RestorePreview { changed_files: files, workdir_is_dirty })
}

/// Restore the snapshot by running `git stash apply <oid>` — but first take a
/// *new* snapshot of the current workdir so the restore itself is reversible.
/// The journal entry is marked `consumed = true` on success.
pub fn restore(git: &GitCli, repo: &Repository, entry_id: u64, policy: &SnapshotPolicy) -> Result<RecoveryEntry> {
    let git_dir = repo.path().to_path_buf();
    let workdir = repo.workdir()
        .ok_or_else(|| GitError::Other("bare repository: no workdir to restore into".into()))?
        .to_path_buf();

    // Safety net: snapshot the *current* state so the restore is reversible.
    // A failed safety snapshot must never block the restore the user asked for.
    if let Err(e) = snapshot_with_policy(
        git, repo, RecoveryKind::Other, format!("before restoring recovery #{entry_id}"), policy,
    ) {
        tracing::warn!("recovery snapshot skipped: {e}");
    }

    // Locate the entry in the journal.
    let mut entries = read_all_entries(&git_dir);
    let entry = entries.iter()
        .find(|e| e.id == entry_id)
        .cloned()
        .ok_or_else(|| GitError::Other(format!("recovery entry {entry_id} not found")))?;

    if entry.snapshot_oid.is_empty() {
        return Err(GitError::Other(
            "this entry is log-only (excluded by snapshot policy) — cannot be restored".into()
        ));
    }

    // `git stash apply <OID>` merges the snapshot into the working tree.
    // It never drops refs/stash, so our recovery ref stays alive even after
    // a successful restore.
    let out = git.command()
        .args(["stash", "apply", &entry.snapshot_oid])
        .current_dir(&workdir)
        .output()
        .map_err(|e| GitError::Other(format!("git stash apply failed to spawn: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(GitError::Other(format!("restore failed: {stderr}")));
    }

    // Mark as consumed in the journal.
    for e in entries.iter_mut() {
        if e.id == entry_id { e.consumed = true; }
    }
    if let Err(e) = write_all_entries(&git_dir, &entries) {
        tracing::warn!("recovery journal rewrite failed after restore: {e}");
    }

    Ok(entry)
}

/// Delete a snapshot and its journal entry — used by the UI "Discard" button.
pub fn delete(git: &GitCli, repo: &Repository, entry_id: u64) -> Result<()> {
    let git_dir = repo.path().to_path_buf();
    let workdir = repo.workdir()
        .ok_or_else(|| GitError::Other("bare repository".into()))?
        .to_path_buf();

    let mut entries = read_all_entries(&git_dir);
    let idx = entries.iter().position(|e| e.id == entry_id)
        .ok_or_else(|| GitError::Other(format!("recovery entry {entry_id} not found")))?;
    let removed = entries.remove(idx);

    let _ = git.command()
        .args(["update-ref", "-d", &removed.ref_name])
        .current_dir(&workdir)
        .output();

    if let Err(e) = write_all_entries(&git_dir, &entries) {
        tracing::warn!("recovery journal rewrite failed after delete: {e}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclude_reason_flags_oversize() {
        let policy = SnapshotPolicy { max_file_size: 100, deny_extensions: vec![], retention_days: 30 };
        assert!(policy.exclude_reason("big.bin", 200).is_some());
        assert!(policy.exclude_reason("small.bin", 50).is_none());
    }

    #[test]
    fn exclude_reason_flags_denied_extension_case_insensitive() {
        let policy = SnapshotPolicy {
            max_file_size: u64::MAX,
            deny_extensions: vec!["zip".into()],
            retention_days: 30,
        };
        assert!(policy.exclude_reason("archive.ZIP", 10).is_some());
        assert!(policy.exclude_reason("archive.zip", 10).is_some());
        assert!(policy.exclude_reason("notes.txt", 10).is_none());
        // No extension at all → never denied by the extension rule.
        assert!(policy.exclude_reason("Makefile", 10).is_none());
    }

    #[test]
    fn kind_slug_is_stable() {
        assert_eq!(RecoveryKind::ResetHard.slug(), "reset-hard");
        assert_eq!(RecoveryKind::StashForceApply.slug(), "stash-force-apply");
        assert_eq!(RecoveryKind::Other.slug(), "other");
    }
}
