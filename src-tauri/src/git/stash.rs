use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
    pub oid: String,
}

/// Lightweight projection of a stash used by the graph rendering layer so
/// the frontend can draw a GitKraken-style dashed marker on the commit the
/// stash was created from. `parent_oid` is the first parent of the stash
/// commit (i.e. HEAD at stash time) — the natural anchor for visualising
/// "this stash came from that commit".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashRef {
    pub index:      usize,
    pub oid:        String,
    pub parent_oid: String,
    pub message:    String,
}

/// Returned by stash_apply / stash_pop so the frontend knows whether
/// conflicts need resolution or the apply was clean.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashApplyResult {
    /// True when the apply produced index conflicts that need manual resolution.
    pub has_conflicts: bool,
    /// Relative paths of conflicted files (populated when has_conflicts = true).
    pub conflicted_files: Vec<String>,
    /// Untracked files that blocked the apply entirely (git refused to start).
    /// When non-empty the apply was NOT performed — the user must confirm
    /// deleting these files before the stash can be applied.
    pub blocking_untracked: Vec<String>,
    /// True when the apply was a true no-op — every change in the stash was
    /// already present in the working tree.  Lets the UI surface
    /// "Already up to date / no changes" instead of a generic "Stash
    /// applied" toast that would lie about doing something.
    #[serde(default)]
    pub no_changes: bool,
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

pub fn list_stashes(repo: &mut Repository) -> Result<Vec<StashEntry>> {
    let mut entries: Vec<StashEntry> = Vec::new();
    repo.stash_foreach(|index, message, oid| {
        entries.push(StashEntry {
            index,
            message: message.to_string(),
            oid: oid.to_string(),
        });
        true
    })?;
    Ok(entries)
}

/// Collect graph-ready stash markers: each entry carries the stash's own
/// OID plus the OID of its first parent (the commit HEAD was at when the
/// stash was created), so the graph can anchor a dashed marker to that
/// base commit.
///
/// Stashes whose first parent cannot be resolved are skipped silently —
/// they would have nowhere to anchor on the graph and are surfaced to the
/// user through the Stash sidebar only.
pub fn collect_stash_refs(repo: &mut Repository) -> Result<Vec<StashRef>> {
    // `stash_foreach` borrows the repo mutably for the duration of the walk,
    // so we collect the raw (index, message, oid) tuples first and resolve
    // parent OIDs afterwards when the borrow has ended.
    let mut raw: Vec<(usize, String, String)> = Vec::new();
    repo.stash_foreach(|index, message, oid| {
        raw.push((index, message.to_string(), oid.to_string()));
        true
    })?;

    let mut out: Vec<StashRef> = Vec::with_capacity(raw.len());
    for (index, message, oid) in raw {
        let stash_oid = match git2::Oid::from_str(&oid) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let commit = match repo.find_commit(stash_oid) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let parent = match commit.parent(0) {
            Ok(p) => p,
            Err(_) => continue,
        };
        out.push(StashRef {
            index,
            oid,
            parent_oid: parent.id().to_string(),
            message,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Save a stash using the git CLI.
///
/// Using the CLI (instead of libgit2's `stash_save`) ensures that the
/// `.git/logs/refs/stash` reflog is written correctly on all platforms
/// (including Windows), making stashes visible to other git tools like
/// GitKraken, SourceTree, etc.
///
/// The message is formatted as `WIP on <branch>: <short-sha> <summary>` — the
/// canonical git stash format — so that tools like GitKraken can identify the
/// diff base from the message (they parse the short SHA to find the parent commit).
/// A user-supplied note is appended after the summary when present.
pub fn stash_save(workdir: &std::path::Path, message: Option<&str>, include_untracked: bool) -> Result<StashEntry> {

    // ── Build canonical WIP message ─────────────────────────────────────────
    // Format: "WIP on <branch>: <short-sha> <commit-summary>[ — <user-note>]"
    // This matches what `git stash push` generates by default (without -m),
    // ensuring maximum compatibility with GitKraken, SourceTree, and similar tools
    // that parse the short SHA out of the message to locate the diff base.

    let branch = git_current_branch(workdir).unwrap_or_else(|| "HEAD".into());
    let short_sha = git_head_short_sha(workdir).unwrap_or_else(|| "unknown".into());
    let head_summary = git_head_summary(workdir).unwrap_or_else(|| "stash".into());

    let canonical_msg = match message {
        Some(user_note) if !user_note.trim().is_empty() => {
            format!("WIP on {branch}: {short_sha} {user_note}")
        }
        _ => {
            format!("WIP on {branch}: {short_sha} {head_summary}")
        }
    };

    // Always pass `-u` when the caller asks: the old implementation dropped
    // `-u` unconditionally "for GitKraken compatibility" (3-parent stash
    // commits render oddly there). That was a cosmetic concern with a real
    // data-loss cost — new or uncommitted files would sit in the working
    // tree during a pull's force-checkout and get silently overwritten.
    // Honour the caller's intent: pre-pull stash passes true, deliberate
    // tracked-only user stashes can still pass false.
    let mut args: Vec<&str> = vec!["stash", "push"];
    if include_untracked {
        args.push("-u");
    }
    args.push("-m");
    args.push(&canonical_msg);

    let out = crate::git_cli::command()
        .args(&args)
        .current_dir(workdir)
        .no_window()
        .output()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(AppError::Other(format!("git stash push failed: {stderr}")));
    }

    // Resolve the OID of the newly created stash entry.
    let oid_out = crate::git_cli::command()
        .args(["rev-parse", "refs/stash"])
        .current_dir(workdir)
        .no_window()
        .output()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;

    let oid = String::from_utf8_lossy(&oid_out.stdout).trim().to_string();

    Ok(StashEntry { index: 0, message: canonical_msg, oid })
}

// ── Helpers for canonical stash message ─────────────────────────────────────

fn git_current_branch(workdir: &std::path::Path) -> Option<String> {
    let out = crate::git_cli::command()
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(workdir)
        .no_window()
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() || s == "HEAD" { None } else { Some(s) }
}

fn git_head_short_sha(workdir: &std::path::Path) -> Option<String> {
    let out = crate::git_cli::command()
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workdir)
        .no_window()
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn git_head_summary(workdir: &std::path::Path) -> Option<String> {
    let out = crate::git_cli::command()
        .args(["log", "-1", "--pretty=format:%s"])
        .current_dir(workdir)
        .no_window()
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // Truncate to ~60 chars to keep the stash message readable
    if s.len() > 60 {
        Some(format!("{}…", &s[..59]))
    } else if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Shared apply/pop implementation using the git CLI.
///
/// Using the CLI guarantees that:
/// - Changes are actually written to the filesystem (libgit2 on Windows can
///   silently succeed without modifying the working directory).
/// - The reflog is updated correctly so other tools stay in sync.
/// - Conflict state is written to .git/index exactly as `git stash apply` does.
///
/// If `drop_on_success` is true this behaves like `git stash pop` (the stash
/// entry is removed when the apply is clean).
fn run_stash_apply(
    workdir: &std::path::Path,
    repo:    &Repository,
    index:   usize,
    drop_on_success: bool,
) -> Result<StashApplyResult> {
    let stash_ref = format!("stash@{{{}}}", index);
    let subcmd    = if drop_on_success { "pop" } else { "apply" };

    let out = crate::git_cli::command()
        .args(["stash", subcmd, &stash_ref])
        .current_dir(workdir)
        .no_window()
        .output()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // ── Diagnostic dump ────────────────────────────────────────────────────
    // We've had several silent-skip reports (apply "succeeds" but a tracked
    // deletion never lands on disk).  Always log git's full output at INFO
    // level so the next repro tells us what git actually did — cheap and
    // self-contained, no extra flag needed.
    tracing::info!(
        target: "arbor::stash",
        cmd = subcmd, stash = %stash_ref, code = ?out.status.code(),
        "git stash {subcmd} ─ stdout:\n{}\nstderr:\n{}",
        stdout.trim(), stderr.trim(),
    );

    // Even on a successful exit git surfaces conflicts via the index when the
    // user resolved a previous attempt with `git rerere`/`git checkout` — be
    // defensive and check for stage entries before assuming a clean apply.
    if out.status.success() {
        let conflicted_files = collect_conflicted_files(repo)?;
        if !conflicted_files.is_empty() {
            return Ok(StashApplyResult {
                has_conflicts:      true,
                conflicted_files,
                blocking_untracked: vec![],
                no_changes:         false,
            });
        }
        // Even on success git can emit "Already up to date" / "nothing to
        // commit" when the stash content matches HEAD or was already
        // applied — surface that so the UI can show a friendlier toast.
        let noop = stdout.contains("Already up to date")
            || stdout.contains("nothing to commit");
        return Ok(StashApplyResult {
            has_conflicts:      false,
            conflicted_files:   vec![],
            blocking_untracked: vec![],
            no_changes:         noop,
        });
    }

    // Unified blocker handling — git can emit THREE different "I can't apply
    // this" categories simultaneously in the same output, and the previous
    // code only ever caught one of them:
    //
    //   (1) "Your local changes to the following files would be overwritten
    //       by merge:"           ← tracked + locally modified
    //   (2) "The following untracked working tree files would be overwritten
    //       by merge:"           ← untracked, would be stomped by tracked side
    //   (3) "<path> already exists, no checkout"
    //       + "could not restore untracked files from stash"
    //                            ← parent(2) restore phase
    //
    // When any of them fires git ABORTS entirely — no tracked merge, no
    // untracked restore, nothing.  So we collect every path from all three
    // categories and surface them as a single blocker list for the UI's
    // "force apply" modal, which will let the user pick per file and then
    // call force_stash_apply (which actually re-runs git stash apply after
    // pre-handling the blockers so the merge can run).
    let has_blockers =
           stderr.contains("untracked working tree files would be overwritten")
        || stderr.contains("Your local changes to the following files would be overwritten")
        || stderr.contains("could not restore untracked files from stash")
        || stdout.contains("could not restore untracked files from stash");
    if has_blockers {
        let blocking = parse_all_blocking_paths(&stderr, &stdout);
        if !blocking.is_empty() {
            // Byte-equal filter: only meaningful for paths whose stash-side
            // version comes from parent(2)'s untracked tree (tracked-modify
            // blockers by definition differ).  Anything not in parent(2)
            // stays in `differing` so the user sees it.
            let (differing, identical) =
                filter_identical_stash_untracked(repo, index, &blocking, workdir);

            if differing.is_empty() && !identical.is_empty() {
                tracing::info!(
                    "stash apply: {} blocking file(s) already match the stash byte-for-byte — treating as clean apply",
                    identical.len()
                );
                if drop_on_success {
                    let _ = crate::git_cli::command()
                        .args(["stash", "drop", &stash_ref])
                        .current_dir(workdir)
                        .no_window()
                        .output();
                }
                return Ok(StashApplyResult {
                    has_conflicts:      false,
                    conflicted_files:   vec![],
                    blocking_untracked: vec![],
                    no_changes:         true,
                });
            }

            return Ok(StashApplyResult {
                has_conflicts:      false,
                conflicted_files:   vec![],
                blocking_untracked: differing,
                no_changes:         false,
            });
        }
    }

    // Non-zero exit: may be merge conflicts — check the index for stage-1/2/3
    // entries, which is git's canonical signal for conflicts requiring resolution.
    let conflicted_files = collect_conflicted_files(repo)?;
    if !conflicted_files.is_empty() {
        return Ok(StashApplyResult { has_conflicts: true, conflicted_files, blocking_untracked: vec![], no_changes: false });
    }

    // Some git versions print "CONFLICT (...) in <path>" to stdout/stderr but
    // bail out before writing the index when an unrelated step fails (rerere
    // post-write, lstat ENOENT). Surface those as resolvable conflicts when
    // we can extract the filenames so the user lands in the resolver instead
    // of hitting a wall of error text.
    let parsed_conflicts = parse_conflict_paths(&stdout, &stderr);
    if !parsed_conflicts.is_empty() {
        return Ok(StashApplyResult {
            has_conflicts:      true,
            conflicted_files:   parsed_conflicts,
            blocking_untracked: vec![],
            no_changes:         false,
        });
    }

    // (the dedicated "Your local changes ..." branch is gone — that case
    // now flows through the unified blocker handler above)

    // git refused because the stash ref doesn't exist (already popped, manual
    // reflog edit, etc.). Map to the typed variant so the UI can refresh the
    // stash list rather than render a generic toast.
    if stderr.contains("is not a valid reference")
        || stderr.contains("is not a stash-like commit")
        || stderr.contains("No stash entries found")
    {
        return Err(AppError::StashNotFound(index));
    }

    // "Stash and local content are identical" / no-op apply: git exits non-zero
    // with stderr like "Cannot apply: nothing to commit, working tree clean".
    // Treat as a benign success so the UI doesn't surface a scary error.
    if stderr.trim().is_empty() && stdout.contains("Already up to date") {
        return Ok(StashApplyResult { has_conflicts: false, conflicted_files: vec![], blocking_untracked: vec![], no_changes: true });
    }

    // Genuine, unmapped failure — surface git's text verbatim.
    Err(AppError::Other(format!("git stash {subcmd} failed: {stderr}")))
}

pub fn stash_apply(repo: &mut Repository, index: usize) -> Result<StashApplyResult> {
    let workdir = repo.workdir()
        .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
        .to_path_buf();
    run_stash_apply(&workdir, repo, index, false)
}

pub fn stash_pop(repo: &mut Repository, index: usize) -> Result<StashApplyResult> {
    let workdir = repo.workdir()
        .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
        .to_path_buf();
    run_stash_apply(&workdir, repo, index, true)
}

pub fn stash_drop(repo: &mut Repository, index: usize) -> Result<()> {
    let workdir = repo.workdir()
        .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
        .to_path_buf();
    let stash_ref = format!("stash@{{{}}}", index);
    let out = crate::git_cli::command()
        .args(["stash", "drop", &stash_ref])
        .current_dir(&workdir)
        .no_window()
        .output()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;
    if !out.status.success() {
        return Err(AppError::StashNotFound(index));
    }
    Ok(())
}

/// Rename a stash entry by patching the reflog file directly.
///
/// Git does not have a native rename command.  `git stash store` only *prepends*
/// a new entry (it never edits an existing one), so the only reliable way to
/// rename `stash@{N}` in-place is to edit `.git/logs/refs/stash`.
///
/// Reflog format (one entry per line, oldest-first → `stash@{0}` is the LAST line):
///   `<old-oid> <new-oid> <name> <email> <timestamp> <tz>\t<message>`
///
/// We find the target line (`total - 1 - index`), replace everything after the
/// first tab with the new message, then write the file back atomically.
pub fn stash_rename(repo: &mut Repository, index: usize, new_message: &str) -> Result<StashEntry> {
    let git_dir = repo.path(); // .git/ directory (always available, even for bare repos)
    let reflog_path = git_dir.join("logs").join("refs").join("stash");

    let content = std::fs::read_to_string(&reflog_path)
        .map_err(|e| AppError::Other(format!("failed to read stash reflog: {e}")))?;

    // Collect as owned Strings to allow in-place mutation.
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    let total = lines.len();
    if index >= total {
        return Err(AppError::StashNotFound(index));
    }

    // stash@{0} → last line; stash@{N} → line at (total - 1 - N)
    let line_idx = total - 1 - index;

    // Extract the SHA (second whitespace-separated field) for the returned StashEntry.
    let sha = lines[line_idx].split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();

    // Replace the message (everything after the first '\t').
    let new_line = if let Some(tab_pos) = lines[line_idx].find('\t') {
        format!("{}\t{}", &lines[line_idx][..tab_pos], new_message)
    } else {
        format!("{}\t{}", &lines[line_idx], new_message)
    };
    lines[line_idx] = new_line;

    // Rebuild the file.
    let new_content = lines.join("\n") + "\n";

    // Atomic write: write to a sibling temp file, then rename over the target.
    // rename() is atomic on POSIX and best-effort on Windows (succeeds as long
    // as no process has the file exclusively open).  This prevents the reflog
    // from being left truncated or empty if the process is interrupted mid-write.
    let tmp_path = reflog_path.with_extension("tmp");
    std::fs::write(&tmp_path, new_content.as_bytes())
        .map_err(|e| AppError::Other(format!("failed to write temp reflog: {e}")))?;
    std::fs::rename(&tmp_path, &reflog_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        AppError::Other(format!("failed to atomically replace stash reflog: {e}"))
    })?;

    Ok(StashEntry { index, message: new_message.to_string(), oid: sha })
}

/// Abort a stash apply that left conflicts — resets index and working tree to HEAD.
///
/// Uses `git reset --merge HEAD` which:
/// - Resets conflicted index entries and their working-tree counterparts to HEAD.
/// - Preserves staged changes that are *not* involved in the conflict, so other
///   work-in-progress staged before the apply is not silently discarded.
///
/// If `--merge` fails, an error is returned WITHOUT falling back to
/// `--hard` — the old silent fallback could silently destroy unstaged work
/// unrelated to the conflict.  The caller (UI) is responsible for surfacing
/// the error so the user can decide how to proceed.
///
/// If `repo` is provided a pre-abort recovery snapshot is taken so the user
/// can always roll the state back from the Recovery tab.
#[allow(dead_code)]
pub fn abort_stash_apply(workdir: &std::path::Path) -> Result<()> {
    abort_stash_apply_with_snapshot(workdir, None)
}

/// Variant that also records a recovery snapshot when a repo handle is available.
pub fn abort_stash_apply_with_snapshot(
    workdir: &std::path::Path,
    repo:    Option<&Repository>,
) -> Result<()> {
    if let Some(r) = repo {
        crate::git::recovery::try_snapshot(
            r,
            crate::git::recovery::RecoveryKind::Other,
            "before aborting stash apply".to_string(),
        );
    }

    let out = crate::git_cli::command()
        .args(["reset", "--merge", "HEAD"])
        .current_dir(workdir)
        .no_window()
        .output()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;

    if out.status.success() {
        return Ok(());
    }

    // Do NOT fall back to `--hard`: that path silently discarded unstaged
    // modifications that were unrelated to the stash conflict and caused at
    // least one real-world data-loss incident.  Surface the error instead so
    // the UI can ask the user how to proceed.
    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(AppError::Other(format!(
        "git reset --merge HEAD failed: {stderr}\n\
         Refusing to fall back to --hard (would discard unrelated unstaged work). \
         Resolve the conflict manually or run `git reset --hard HEAD` from the terminal \
         if you're sure the workdir can be thrown away — the Recovery tab has a snapshot."
    )))
}

/// Finalise a stash apply after the user has resolved its blocking files.
///
/// CONTEXT: when `git stash apply` finds blockers it ABORTS entirely —
/// no tracked merge, no untracked restore, no index change.  So we can't
/// just "fill in the blanks"; we have to pre-clear the obstacles and let
/// git run the full apply itself.  The old implementation tried to skip
/// the retry and write blobs directly; that left tracked deletions /
/// non-blocker tracked changes silently un-applied (the `apply` toast
/// said success but the workdir was still in pre-apply state for everyone
/// except the explicit blocker files).
///
/// New flow:
///   1. Recovery snapshot (so the user can always roll back from the tab).
///   2. `rm` every `files_to_delete` path from the workdir.
///   3. Rename every existing `files_to_keep` path to
///      `<dir>/.<basename>.arbor-keep-mine.tmp`.
///   4. Re-run `git stash apply` — now unobstructed for the rm'd paths
///      and the renamed paths.
///   5. Restore the keep-mine backups (overwriting whatever git just
///      wrote at those paths) so the user's chosen version sticks.
///
/// Parameters:
///   `files_to_delete` — paths the user picked "use stash" for.  The
///     workdir copy is removed; the retried apply restores the stash's
///     version (whether tracked merge or parent(2) untracked restore).
///     A historical name — the actual semantics are "let stash win".
///   `files_to_keep` — paths the user picked "keep mine" for, plus any
///     custom-merged files already written by the frontend.  Backed up
///     and restored verbatim around the retry.
///   `drop_on_success` — when true (the user opened the modal from a
///     `pop` originally), drop the stash entry once everything is settled.
pub fn force_stash_apply(
    repo:            &mut Repository,
    index:           usize,
    files_to_delete: &[String],
    files_to_keep:   &[String],
    drop_on_success: bool,
) -> Result<StashApplyResult> {
    let workdir = repo.workdir()
        .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
        .to_path_buf();

    // Recovery snapshot BEFORE any destructive mutation — the rm/mv steps
    // below are reversible only through this snapshot.  If anything below
    // panics or the retry'd apply ends in a state the user can't undo,
    // they can roll back from the Recovery tab.
    crate::git::recovery::try_snapshot(
        repo,
        crate::git::recovery::RecoveryKind::StashForceApply,
        format!(
            "resolve stash@{{{}}} blockers (use stash: {}, keep mine: {})",
            index, files_to_delete.len(), files_to_keep.len()
        ),
    );

    // ── Step 1: pre-clear "use stash" blockers ─────────────────────────────
    // For each path the user picked "use stash" on: remove the workdir
    // file so the retried `git stash apply` can restore the stash's
    // version (tracked merge or parent(2) restore — git decides).  If the
    // file doesn't exist we don't care.  Empty parent dirs are pruned so
    // git doesn't trip on a leftover dir of the wrong type.
    for rel in files_to_delete {
        let abs = workdir.join(rel);
        match std::fs::remove_file(&abs) {
            Ok(()) => {
                let mut p = abs.parent();
                while let Some(dir) = p {
                    if dir == workdir { break; }
                    if std::fs::remove_dir(dir).is_err() { break; }
                    p = dir.parent();
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => { /* fine */ }
            Err(e) => {
                return Err(AppError::Other(format!(
                    "failed to remove {rel} before retry: {e} (use Recovery tab to roll back)",
                )));
            }
        }
    }

    // ── Step 2: backup "keep mine" blockers ────────────────────────────────
    // Move each "keep mine" file to a sibling `<file>.arbor-keep-mine.tmp`
    // so `git stash apply` is free to write the stash's version into the
    // canonical path.  We restore the user's bytes after the retry — net
    // effect: stash applies cleanly EXCEPT for these paths, which keep
    // the user's content.  Track every backup so we can roll back on any
    // failure mid-way.
    let mut backups: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
    for rel in files_to_keep {
        let abs = workdir.join(rel);
        if !abs.exists() { continue; }
        let backup = backup_path_for(&abs);
        if let Err(e) = std::fs::rename(&abs, &backup) {
            // Roll back the backups we already made so the workdir is
            // left in a coherent (if incomplete) state.
            for (orig, bak) in backups.iter().rev() {
                let _ = std::fs::rename(bak, orig);
            }
            return Err(AppError::Other(format!(
                "failed to backup {rel} before retry: {e} (use Recovery tab to roll back)",
            )));
        }
        backups.push((abs, backup));
    }

    // ── Step 3: retry `git stash apply` ────────────────────────────────────
    // Reuse run_stash_apply so we benefit from its full blocker/conflict
    // parsing for whatever git does this time round.  Possible outcomes:
    //   • success / no_changes    → great, restore keep-mine backups and exit
    //   • conflicts (stage 1/2/3) → frontend's standard conflict resolver
    //                               will pick them up after we restore
    //   • new blockers            → bubble up so the modal can re-prompt
    //   • hard failure            → propagate as Err
    let result = run_stash_apply(&workdir, repo, index, drop_on_success);

    // ── Step 4: restore keep-mine backups ──────────────────────────────────
    // Always restore, regardless of outcome above — the user explicitly
    // asked us to keep their version of these files.  Overwrite whatever
    // git may have written there (it may have written stash's version, or
    // a conflict-marker file; either way the user's choice trumps it).
    for (orig, backup) in &backups {
        // Best-effort: if we can't move back we leave the .tmp alongside
        // so the user can recover manually — better than dropping data.
        let _ = std::fs::remove_file(orig);
        if let Err(e) = std::fs::rename(backup, orig) {
            tracing::warn!(
                "force_stash_apply: failed to restore keep-mine backup {:?} → {:?}: {e}",
                backup, orig,
            );
        }
    }

    result
}

/// Build the sibling path used to stash a "keep mine" file out of the way
/// during a force-apply retry.  Format: hidden dotfile next to the
/// original — `.<basename>.arbor-keep-mine.tmp` — so it never clashes
/// with anything the stash might write and is easy to spot/manual-recover.
fn backup_path_for(abs: &std::path::Path) -> std::path::PathBuf {
    let parent = abs.parent().unwrap_or_else(|| std::path::Path::new(""));
    let name   = abs.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    parent.join(format!(".{name}.arbor-keep-mine.tmp"))
}

// ---------------------------------------------------------------------------
// Blocking-file content
// ---------------------------------------------------------------------------

/// Content of a blocking untracked file: current on-disk version vs. the
/// version that would be written by the stash (read from the stash commit tree).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashBlockingContent {
    pub path: String,
    /// Current content of the file on disk. None when the file does not exist
    /// in the working tree.
    pub current_content: Option<String>,
    /// Content from the stash commit's tree. None when the file is binary or
    /// not present in the stash.
    pub stash_content: Option<String>,
    /// Encoding used to decode both blobs ("UTF-8" for modern files,
    /// "windows-1252" for legacy Latin-1 sources). Detected from the
    /// working-tree file when present, otherwise from the stash blob.
    pub encoding: Option<String>,
}

pub fn get_stash_file_content(
    repo:  &Repository,
    index: usize,
    path:  &str,
    encoding_override: Option<&str>,
) -> Result<StashBlockingContent> {
    use crate::git::encoding::{decode_with, detect, encoding_for_label};

    // ── Current file from working directory ──────────────────────────────────
    let workdir = repo.workdir()
        .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?;
    let current_bytes = std::fs::read(workdir.join(path)).ok();

    // ── Stash version from the stash commit tree ─────────────────────────────
    // Do NOT use reference.peel_to_commit() — use revparse_single + find_commit
    // as required by the project conventions (vendored libgit2 bug).
    //
    // Stash commit shape:
    //   parent(0) = HEAD at stash time
    //   parent(1) = index tree at stash time (tracked changes)
    //   parent(2) = untracked tree at stash time (only when stashed with -u)
    //
    // `commit.tree()` is the working-tree snapshot of TRACKED files only.
    // For files that were untracked at stash time we must fall back to
    // parent(2)'s tree — otherwise the diff view shows
    // "— rimosso nello stash —" even though the file is right there in
    // the stash payload.
    let stash_ref = format!("stash@{{{}}}", index);
    let stash_bytes: Option<Vec<u8>> = (|| -> Option<Vec<u8>> {
        let obj    = repo.revparse_single(&stash_ref).ok()?;
        let commit = repo.find_commit(obj.id()).ok()?;
        let read_from_tree = |tree: &git2::Tree| -> Option<Vec<u8>> {
            let entry = tree.get_path(std::path::Path::new(path)).ok()?;
            let blob  = repo.find_blob(entry.id()).ok()?;
            if blob.is_binary() { return None; }
            Some(blob.content().to_vec())
        };
        if let Ok(t) = commit.tree() {
            if let Some(bytes) = read_from_tree(&t) { return Some(bytes); }
        }
        // Fallback: untracked tree (parent 2). Absent on stashes created
        // without `-u`; that's fine — we just return None below.
        let untracked = commit.parent(2).ok()?.tree().ok()?;
        read_from_tree(&untracked)
    })();

    // Pick a single encoding to use for both sides — driven by the user's
    // override, or by sniffing the working-tree file (the one the user is
    // actually editing), with the stash blob as fallback.
    let enc = match encoding_override {
        Some(label) => encoding_for_label(label),
        None => current_bytes.as_deref()
            .or_else(|| stash_bytes.as_deref())
            .map(detect)
            .unwrap_or(encoding_rs::UTF_8),
    };

    let current_content = current_bytes.as_deref().map(|b| decode_with(b, enc));
    let stash_content   = stash_bytes.as_deref().map(|b| decode_with(b, enc));

    Ok(StashBlockingContent {
        path: path.to_string(),
        current_content,
        stash_content,
        encoding: Some(enc.name().to_string()),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn collect_conflicted_files(repo: &Repository) -> Result<Vec<String>> {
    // Reading the index directly is more reliable than checking the CONFLICTED
    // status bit, which libgit2 does not always set after a stash-apply conflict.
    // Files in conflict have stage 1 (base), 2 (ours), or 3 (theirs) entries in
    // the index; a normal file only ever has stage 0.
    let mut index = repo.index().map_err(AppError::Git)?;
    // Force-read from disk so we see the conflict state just written by stash_apply.
    index.read(true).map_err(AppError::Git)?;

    let mut seen = std::collections::HashSet::new();
    for entry in index.iter() {
        // flags bits [12:13] encode the stage number.
        let stage = (entry.flags & 0x3000) >> 12;
        if stage > 0 {
            seen.insert(String::from_utf8_lossy(&entry.path).into_owned());
        }
    }
    Ok(seen.into_iter().collect())
}

/// Best-effort scrape of "CONFLICT" / "Merge conflict in" lines from git's
/// output. Used as a fallback when the index doesn't carry stage-1/2/3
/// entries but stash apply still produced a logical conflict (rare — happens
/// when an unrelated post-write step bails out).
fn parse_conflict_paths(stdout: &str, stderr: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for stream in [stdout, stderr] {
        for line in stream.lines() {
            // "CONFLICT (content): Merge conflict in path/to/file.rs"
            // "CONFLICT (rename/delete): foo deleted in HEAD …"
            if let Some(rest) = line.strip_prefix("CONFLICT") {
                if let Some(idx) = rest.find(" in ") {
                    let path = rest[idx + 4..].trim();
                    if !path.is_empty() {
                        seen.insert(path.to_string());
                    }
                }
            }
            // "Merge conflict in path/to/file.rs" (no CONFLICT prefix variant)
            if let Some(rest) = line.strip_prefix("Merge conflict in ") {
                let path = rest.trim();
                if !path.is_empty() {
                    seen.insert(path.to_string());
                }
            }
        }
    }
    seen.into_iter().collect()
}

/// Partition `paths` into (differing, identical) by comparing each workdir
/// file against the corresponding blob in the stash's untracked tree (3rd
/// parent of the stash commit, present only when `-u` was used).
///
/// Files whose on-disk bytes match the stash's bytes exactly land in
/// `identical` — git refused to overwrite them but the user has nothing
/// to resolve since the two versions are byte-equal.  Any lookup failure
/// (missing parent, blob, file) keeps the path in `differing` so we err on
/// the side of letting the user decide.
fn filter_identical_stash_untracked(
    repo: &Repository,
    stash_index: usize,
    paths: &[String],
    workdir: &std::path::Path,
) -> (Vec<String>, Vec<String>) {
    let stash_oid_str = match crate::git_cli::command()
        .args(["rev-parse", &format!("stash@{{{stash_index}}}")])
        .current_dir(workdir)
        .no_window()
        .output()
    {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => return (paths.to_vec(), Vec::new()),
    };

    let oid = match git2::Oid::from_str(&stash_oid_str) {
        Ok(o) => o,
        Err(_) => return (paths.to_vec(), Vec::new()),
    };
    let stash_commit = match repo.find_commit(oid) {
        Ok(c) => c,
        Err(_) => return (paths.to_vec(), Vec::new()),
    };
    // Parent index 2 = the untracked-tree commit synthesised by
    // `git stash push -u`.  Without `-u` this parent is absent and the
    // failure mode we're filtering can't happen — but be defensive.
    let untracked_commit = match stash_commit.parent(2) {
        Ok(c) => c,
        Err(_) => return (paths.to_vec(), Vec::new()),
    };
    let untracked_tree = match untracked_commit.tree() {
        Ok(t) => t,
        Err(_) => return (paths.to_vec(), Vec::new()),
    };

    let mut differing = Vec::with_capacity(paths.len());
    let mut identical = Vec::new();
    for p in paths {
        let abs = workdir.join(p);
        let wd_bytes = match std::fs::read(&abs) {
            Ok(b) => b,
            Err(_) => { differing.push(p.clone()); continue; }
        };
        let entry = match untracked_tree.get_path(std::path::Path::new(p)) {
            Ok(e) => e,
            Err(_) => { differing.push(p.clone()); continue; }
        };
        let blob = match repo.find_blob(entry.id()) {
            Ok(b) => b,
            Err(_) => { differing.push(p.clone()); continue; }
        };
        if bytes_equal_ignoring_line_endings(blob.content(), &wd_bytes) {
            identical.push(p.clone());
        } else {
            differing.push(p.clone());
        }
    }
    (differing, identical)
}

/// Compare two byte slices treating CRLF and LF as equivalent.  Git stores
/// blobs with LF on disk and applies `core.autocrlf` conversion on
/// checkout — without this normalisation, re-applying a stash that was
/// just restored on Windows shows every text file as "differing".
fn bytes_equal_ignoring_line_endings(a: &[u8], b: &[u8]) -> bool {
    if a == b { return true; }
    let strip_cr: fn(&[u8]) -> Vec<u8> = |s| s.iter().copied().filter(|&c| c != b'\r').collect();
    strip_cr(a) == strip_cr(b)
}

/// Collect every blocker path from a failed `git stash apply` output,
/// across all three categories git may emit:
///
///   (a) `Your local changes to the following files would be overwritten by merge:`
///       — indented file list, terminated by a blank line or
///         "Please commit ..." / "Aborting"
///   (b) `The following untracked working tree files would be overwritten by merge:`
///       — same indented-list format
///   (c) `<path> already exists, no checkout`
///       — one path per line, scattered through stdout/stderr, terminated
///         by `error: could not restore untracked files from stash`
///
/// All three can appear in the same run (we saw it: tracked overwrite +
/// untracked overwrite + parent(2) restore failure together).  We scan
/// both streams, dedupe, return a sorted list.  `error:` / `warning:`
/// prefixes are stripped so we don't accidentally include category headers
/// as paths.
fn parse_all_blocking_paths(stderr: &str, stdout: &str) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut seen: BTreeSet<String> = BTreeSet::new();

    let is_list_header = |line: &str| -> bool {
        let l = line;
        l.contains("Your local changes to the following files would be overwritten")
            || l.contains("untracked working tree files would be overwritten")
    };
    // A "list line" inside categories (a)/(b) is indented with a tab or
    // spaces by git.  We also tolerate plain non-empty lines but bail at
    // common terminators.
    let is_list_terminator = |line: &str| -> bool {
        let t = line.trim_start();
        t.is_empty()
            || t.starts_with("Please")
            || t.starts_with("Aborting")
            || t.starts_with("error:")
            || t.starts_with("warning:")
            || t.starts_with("hint:")
    };

    for stream in [stderr, stdout] {
        let mut in_list = false;
        for line in stream.lines() {
            // Category (a) + (b): header → consume indented paths.
            if is_list_header(line) {
                in_list = true;
                continue;
            }
            if in_list {
                if is_list_terminator(line) {
                    in_list = false;
                    // Fall through so this same line still gets the
                    // "already exists" check below if applicable.
                } else {
                    let p = line.trim();
                    if !p.is_empty() {
                        seen.insert(p.to_string());
                    }
                    continue;
                }
            }
            // Category (c): one-shot "X already exists, no checkout" line,
            // can be prefixed with error:/warning:.
            let cleaned = line
                .trim_start_matches("error:")
                .trim_start_matches("warning:")
                .trim();
            if let Some(path) = cleaned.strip_suffix(" already exists, no checkout") {
                let p = path.trim();
                if !p.is_empty() {
                    seen.insert(p.to_string());
                }
            }
        }
    }
    seen.into_iter().collect()
}
