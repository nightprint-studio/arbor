//! `merge` domain — thin shell wrapper over [`corvus_git::merge`].
//!
//! The pure git logic (conflict three-way load, resolve/remove, merge-commit
//! finaliser, `git merge`, `git merge --abort`, `MERGE_MSG` reader, and the
//! `ConflictContent` / `ConflictPresence` / `MergeOutcome` / `MergeStrategy`
//! types) now lives in the Tauri-free `corvus-git` crate. This module keeps
//! the original shell signatures (no explicit `GitCli`) by resolving the
//! shell's git program from [`crate::git_cli`] and forwarding — so existing
//! callers compile unchanged and behavior is byte-identical.
//!
//! The streaming MR-prep flow ([`prepare_mr_conflict_resolution`] & friends)
//! stays here: it is Tauri-coupled (progress callbacks, job log streaming) and
//! is consumed only by `mr_commands`.

use git2::Repository;

use crate::error::Result;
use crate::process_ext::NoWindowExt;

// Re-export the types that moved into the crate so existing call sites
// (`crate::git::merge::ConflictContent`, …) keep resolving.
pub use corvus_git::merge::{ConflictContent, ConflictPresence, MergeOutcome, MergeStrategy};

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> corvus_git::prelude::GitCli {
    corvus_git::prelude::GitCli::from_optional(crate::git_cli::snapshot().path)
}

// ---------------------------------------------------------------------------
// Forwarders — original shell signatures, delegating to corvus-git
// ---------------------------------------------------------------------------

pub fn get_conflict_presence(repo: &Repository) -> Result<Vec<ConflictPresence>> {
    Ok(corvus_git::merge::get_conflict_presence(repo)?)
}

pub fn get_conflict_content(
    repo: &Repository,
    rel_path: &str,
    encoding_override: Option<&str>,
) -> Result<ConflictContent> {
    Ok(corvus_git::merge::get_conflict_content(repo, rel_path, encoding_override)?)
}

pub fn resolve_stash_conflict(
    repo: &mut Repository,
    rel_path: &str,
    content: &str,
    encoding: Option<&str>,
) -> Result<()> {
    Ok(corvus_git::merge::resolve_stash_conflict(repo, rel_path, content, encoding)?)
}

pub fn remove_conflict_file(repo: &mut Repository, rel_path: &str) -> Result<()> {
    Ok(corvus_git::merge::remove_conflict_file(repo, rel_path)?)
}

pub fn resolve_conflict(
    repo: &mut Repository,
    rel_path: &str,
    content: &str,
    encoding: Option<&str>,
) -> Result<()> {
    Ok(corvus_git::merge::resolve_conflict(repo, rel_path, content, encoding)?)
}

pub fn complete_merge(repo: &mut Repository, message: &str) -> Result<String> {
    Ok(corvus_git::merge::complete_merge(repo, message)?)
}

pub fn merge_branch(
    workdir: &std::path::Path,
    branch_name: &str,
    strategy: MergeStrategy,
) -> Result<MergeOutcome> {
    Ok(corvus_git::merge::merge_branch(&git(), workdir, branch_name, strategy)?)
}

pub fn abort_merge(workdir: &std::path::Path) -> Result<()> {
    Ok(corvus_git::merge::abort_merge(&git(), workdir)?)
}

pub fn get_merge_message(repo: &Repository) -> Result<String> {
    Ok(corvus_git::merge::get_merge_message(repo)?)
}

// ---------------------------------------------------------------------------
// MR conflict-resolution prep — phased, streamable (stays shell-side)
// ---------------------------------------------------------------------------

/// Phases of the MR conflict-resolution prep flow.  Used by the orchestrator
/// to label progress events that flow to the frontend ProgressStepper widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MrPrepPhase {
    Status,
    Fetch,
    Checkout,
    Merge,
}

impl MrPrepPhase {
    pub fn key(self) -> &'static str {
        match self {
            Self::Status   => "status",
            Self::Fetch    => "fetch",
            Self::Checkout => "checkout",
            Self::Merge    => "merge",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Status   => "Checking workdir",
            Self::Fetch    => "Fetching from origin",
            Self::Checkout => "Switching to source branch",
            Self::Merge    => "Merging target",
        }
    }
    pub fn index(self) -> u32 {
        match self {
            Self::Status => 0, Self::Fetch => 1, Self::Checkout => 2, Self::Merge => 3,
        }
    }
    pub const TOTAL: u32 = 4;
}

/// Outcome of the prep flow.  `Conflicts` is the happy-path "user must resolve"
/// signal; the caller opens the conflict-resolution modal.
pub enum MrPrepOutcome {
    Clean,
    Conflicts,
}

/// Phase events emitted by [`prepare_mr_conflict_resolution`] via the
/// `on_event` callback.  The orchestrator translates these into Tauri events
/// for the JobsOverlay (text logs) and the ProgressStepper widget (typed).
pub enum MrPrepEvent<'a> {
    /// Phase began — frontend should advance the stepper.
    /// `detail` is an optional sub-text (e.g. the refs being fetched).
    PhaseStart { phase: MrPrepPhase, detail: Option<String> },
    /// One line of stdout/stderr from the underlying git command.
    Output    { #[allow(dead_code)] phase: MrPrepPhase, line: &'a str },
}

/// Prepare the local workspace for resolving a pull/merge-request conflict.
///
/// Flow:
///   1. Require a clean workdir (no staged / unstaged / untracked changes) —
///      merging into a dirty workdir would overwrite the user's work.
///   2. `git fetch --no-tags origin <source> <target>` — refresh ONLY the two
///      refs we care about (much faster than fetching the whole remote on
///      repos with many branches).
///   3. `git checkout <source>` — move to the MR source branch. When the
///      branch only exists on the remote, `git` auto-creates a local tracking
///      branch (DWIM behaviour), so this works for branches the user never
///      checked out locally.
///   4. `git merge --no-edit origin/<target>` — merge the MR target back into
///      the source.  Returns [`MrPrepOutcome::Conflicts`] when conflicts are
///      produced so the caller can open the resolver modal.
///
/// `on_event` is invoked synchronously on the calling thread for every
/// phase transition and every stdout/stderr line.  Pass a no-op closure when
/// progress reporting is not needed.
pub fn prepare_mr_conflict_resolution(
    workdir:       &std::path::Path,
    source_branch: &str,
    target_branch: &str,
    mut on_event:  impl FnMut(MrPrepEvent<'_>),
) -> Result<MrPrepOutcome> {
    use crate::error::AppError;

    // ── 1. Clean workdir check ──────────────────────────────────────────────
    on_event(MrPrepEvent::PhaseStart { phase: MrPrepPhase::Status, detail: None });
    let status = crate::git_cli::command()
        .args(["status", "--porcelain"])
        .current_dir(workdir)
        .no_window()
        .output()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;
    if !status.status.success() {
        return Err(AppError::Other(
            String::from_utf8_lossy(&status.stderr).trim().to_string(),
        ));
    }
    if !status.stdout.is_empty() {
        return Err(AppError::Other(
            "Working tree has uncommitted changes — commit or stash them before \
             resolving merge conflicts.".into(),
        ));
    }

    // ── 2. Fetch only the two refs we need ──────────────────────────────────
    let fetch_detail = format!("{source_branch}, {target_branch}");
    on_event(MrPrepEvent::PhaseStart {
        phase:  MrPrepPhase::Fetch,
        detail: Some(fetch_detail),
    });
    let origin_url = git2::Repository::open(workdir)
        .ok()
        .and_then(|r| r.find_remote("origin").ok().and_then(|rem| rem.url().map(String::from)))
        .unwrap_or_default();
    let auth_args = crate::git_cli::http_auth_args_for_url(&origin_url);
    run_git_streaming(
        workdir,
        &auth_args,
        // --no-tags + targeted refspecs avoids enumerating every branch on the
        // remote (the original `git fetch origin` is the single biggest source
        // of latency on repos with many branches).
        &[
            "fetch", "--no-tags", "--progress",
            "origin", source_branch, target_branch,
        ],
        MrPrepPhase::Fetch,
        &mut on_event,
    )?;

    // ── 3. Checkout source branch (DWIM creates tracking branch) ────────────
    on_event(MrPrepEvent::PhaseStart {
        phase:  MrPrepPhase::Checkout,
        detail: Some(source_branch.to_string()),
    });
    run_git_streaming(
        workdir,
        &[],
        &["checkout", source_branch],
        MrPrepPhase::Checkout,
        &mut on_event,
    )?;

    // ── 4. Merge origin/<target> into the source branch ─────────────────────
    let target_ref = format!("origin/{target_branch}");
    on_event(MrPrepEvent::PhaseStart {
        phase:  MrPrepPhase::Merge,
        detail: Some(target_ref.clone()),
    });
    match merge_branch_streaming(workdir, &target_ref, &mut on_event) {
        Ok(()) => Ok(MrPrepOutcome::Clean),
        Err(AppError::Other(msg)) if msg.starts_with("CONFLICTS:") => Ok(MrPrepOutcome::Conflicts),
        Err(e) => Err(e),
    }
}

/// Run a git subcommand, streaming stdout+stderr lines through `on_event` as
/// they arrive.  Returns Err on non-zero exit, with stderr as the message.
fn run_git_streaming(
    workdir:  &std::path::Path,
    pre_args: &[String],
    args:     &[&str],
    phase:    MrPrepPhase,
    on_event: &mut impl FnMut(MrPrepEvent<'_>),
) -> Result<()> {
    use crate::error::AppError;
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let mut cmd = crate::git_cli::command();
    cmd.args(pre_args)
       .args(args)
       .current_dir(workdir)
       .no_window()
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut child = cmd.spawn()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;

    // Drain stderr on a side thread; collect for error reporting on failure.
    let stderr_pipe = child.stderr.take().expect("piped");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let stderr_thread = std::thread::spawn(move || {
        let mut all = String::new();
        for line in BufReader::new(stderr_pipe).lines().flatten() {
            all.push_str(&line);
            all.push('\n');
            let _ = tx.send(line);
        }
        all
    });

    // Stdout on the main loop, interleaved with stderr drained from rx.
    let stdout_pipe = child.stdout.take().expect("piped");
    for line in BufReader::new(stdout_pipe).lines().flatten() {
        on_event(MrPrepEvent::Output { phase, line: &line });
        // Pull any stderr lines that arrived in the meantime.
        while let Ok(e) = rx.try_recv() {
            on_event(MrPrepEvent::Output { phase, line: &e });
        }
    }
    // Stdout closed — drain remaining stderr.
    while let Ok(e) = rx.recv() {
        on_event(MrPrepEvent::Output { phase, line: &e });
    }
    let stderr_full = stderr_thread.join().unwrap_or_default();

    let exit = child.wait()
        .map_err(|e| AppError::Other(format!("git wait failed: {e}")))?;
    if !exit.success() {
        return Err(AppError::Other(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or(""),
            stderr_full.trim(),
        )));
    }
    Ok(())
}

/// Streaming variant of [`merge_branch`].  Same conflict-vs-error contract.
fn merge_branch_streaming(
    workdir:    &std::path::Path,
    branch_ref: &str,
    on_event:   &mut impl FnMut(MrPrepEvent<'_>),
) -> Result<()> {
    use crate::error::AppError;

    // Capture all output into a buffer so we can scan for the conflict
    // sentinel after the fact, while still streaming each line live.
    let mut buf = String::new();
    let res = run_git_streaming_capturing(
        workdir,
        &[],
        &["merge", "--no-edit", branch_ref],
        MrPrepPhase::Merge,
        on_event,
        &mut buf,
    );
    match res {
        Ok(()) => Ok(()),
        Err(AppError::Other(_)) if
            buf.contains("Automatic merge failed") || buf.contains("CONFLICT")
        => Err(AppError::Other(format!("CONFLICTS:{}", buf.trim()))),
        Err(e) => Err(e),
    }
}

/// Same as `run_git_streaming` but also accumulates every emitted line into
/// `buf` so the caller can post-process the combined output.
fn run_git_streaming_capturing(
    workdir:  &std::path::Path,
    pre_args: &[String],
    args:     &[&str],
    phase:    MrPrepPhase,
    on_event: &mut impl FnMut(MrPrepEvent<'_>),
    buf:      &mut String,
) -> Result<()> {
    use crate::error::AppError;
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let mut child = crate::git_cli::command()
        .args(pre_args)
        .args(args)
        .current_dir(workdir)
        .no_window()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;

    let stderr_pipe = child.stderr.take().expect("piped");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let stderr_thread = std::thread::spawn(move || {
        let mut all = String::new();
        for line in BufReader::new(stderr_pipe).lines().flatten() {
            all.push_str(&line);
            all.push('\n');
            let _ = tx.send(line);
        }
        all
    });

    let stdout_pipe = child.stdout.take().expect("piped");
    for line in BufReader::new(stdout_pipe).lines().flatten() {
        buf.push_str(&line); buf.push('\n');
        on_event(MrPrepEvent::Output { phase, line: &line });
        while let Ok(e) = rx.try_recv() {
            buf.push_str(&e); buf.push('\n');
            on_event(MrPrepEvent::Output { phase, line: &e });
        }
    }
    while let Ok(e) = rx.recv() {
        buf.push_str(&e); buf.push('\n');
        on_event(MrPrepEvent::Output { phase, line: &e });
    }
    let _ = stderr_thread.join();

    let exit = child.wait()
        .map_err(|e| AppError::Other(format!("git wait failed: {e}")))?;
    if !exit.success() {
        return Err(AppError::Other(buf.trim().to_string()));
    }
    Ok(())
}
