use git2::Repository;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::git::encoding::{decode_bytes, decode_with, encode_for_disk};
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Lightweight presence info for every conflicted file in the index:
/// whether stage-2 (ours) and stage-3 (theirs) exist.  Used by the
/// conflict modal sidebar to show "added by them" / "deleted by them"
/// badges immediately on open, without paying the cost of loading every
/// file's three-way content up front.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictPresence {
    pub path: String,
    pub ours_present: bool,
    pub theirs_present: bool,
}

/// Three-way content of a conflicted file: clean ours/theirs blobs from the
/// git index (no conflict markers) plus the current working-tree content
/// (which contains the raw `<<<<<<<` / `=======` / `>>>>>>>` markers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictContent {
    pub path: String,
    /// Short name for the current HEAD branch (e.g. "main").
    pub ours_label: String,
    /// Short name for the incoming branch / short SHA (e.g. "feature/foo").
    pub theirs_label: String,
    /// Content of the file as it was in HEAD (index stage 2).  Empty when
    /// the file does not exist on our side (`!ours_present`) — typical of
    /// "added by them" conflicts where only stage-3 is populated.
    pub ours_content: String,
    /// Content of the file as it is in the incoming branch (index stage 3).
    /// Empty when the file was removed on their side (`!theirs_present`).
    pub theirs_content: String,
    /// Common ancestor content (index stage 1), if present.
    pub base_content: Option<String>,
    /// The on-disk file that currently contains conflict markers.
    pub working_content: String,
    /// Encoding label inferred from the working-tree file (and used for the
    /// stage blobs). Round-tripped back through `resolve_conflict` so the
    /// file is re-encoded with its original byte representation. "UTF-8" for
    /// modern files; legacy Java/PHP sources on Windows are often
    /// "windows-1252".
    pub encoding: String,
    /// True when the file has a stage-2 (ours) index entry — i.e. the file
    /// exists on the current branch.  False for "added by them" cases.
    pub ours_present: bool,
    /// True when the file has a stage-3 (theirs) index entry — i.e. the
    /// file exists on the incoming side.  False for "deleted by them" cases.
    pub theirs_present: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Scan the index once and return the stage-2/stage-3 presence flags for
/// every conflicted path.  Cheap — no blob reads.
pub fn get_conflict_presence(repo: &Repository) -> Result<Vec<ConflictPresence>> {
    let mut index = repo.index()?;
    // Force-read so we observe state written by the current merge / stash
    // apply attempt rather than a possibly-stale in-memory cache.
    index.read(true)?;

    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    for i in 0..index.len() {
        if let Some(entry) = index.get(i) {
            let stage = ((entry.flags >> 12) & 0x3) as i32;
            // stage-0 = normal file (no conflict); skip.
            if stage == 0 { continue; }
            let path = match std::str::from_utf8(&entry.path) {
                Ok(p) => p.to_string(),
                Err(_) => continue,
            };
            let e = map.entry(path).or_insert((false, false));
            if stage == 2 { e.0 = true; }
            if stage == 3 { e.1 = true; }
        }
    }
    Ok(map.into_iter()
        .map(|(path, (ours, theirs))| ConflictPresence {
            path,
            ours_present:   ours,
            theirs_present: theirs,
        })
        .collect())
}

/// Read the three-way content for a conflicted file path (relative to workdir).
///
/// `encoding_override`, when supplied, forces a specific encoding (e.g.
/// `"windows-1252"`) instead of running auto-detection. The label travels
/// back in the response so the same value is round-tripped to
/// `resolve_conflict` and the file is re-encoded with the user's pick.
pub fn get_conflict_content(
    repo: &Repository,
    rel_path: &str,
    encoding_override: Option<&str>,
) -> Result<ConflictContent> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?;

    let ours_label = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(String::from))
        .unwrap_or_else(|| "HEAD".into());

    let theirs_label = merge_head_label(repo).unwrap_or_else(|_| "MERGE_HEAD".into());

    let index = repo.index()?;

    // Read the working-tree file as raw bytes first — this drives encoding
    // detection (or applies the explicit override). We fall back to a
    // consistent encoding across all stage blobs so the three-way view
    // doesn't mix decoders mid-stream.
    let working_bytes = std::fs::read(workdir.join(rel_path))
        .map_err(|e| AppError::Other(format!("cannot read '{rel_path}': {e}")))?;

    let (working_content, encoding) = match encoding_override {
        Some(label) => {
            let enc = crate::git::encoding::encoding_for_label(label);
            (decode_with(&working_bytes, enc), enc)
        }
        None => decode_bytes(&working_bytes),
    };

    // Stages 1/2/3 may legitimately be absent: stage-2 is missing for
    // "added by them" conflicts, stage-3 for "deleted by them", stage-1 for
    // 2-way conflicts with no common ancestor.  Surface that as a flag
    // instead of failing the whole load — the user still needs to resolve
    // the file (typically accept-theirs or remove).
    let ours_opt   = stage_blob_with_encoding(repo, &index, rel_path, 2, encoding)?;
    let theirs_opt = stage_blob_with_encoding(repo, &index, rel_path, 3, encoding)?;
    let base_content = stage_blob_with_encoding(repo, &index, rel_path, 1, encoding)?;

    let ours_present   = ours_opt.is_some();
    let theirs_present = theirs_opt.is_some();

    Ok(ConflictContent {
        path: rel_path.to_string(),
        ours_label,
        theirs_label,
        ours_content:   ours_opt.unwrap_or_default(),
        theirs_content: theirs_opt.unwrap_or_default(),
        base_content,
        working_content,
        encoding: encoding.name().to_string(),
        ours_present,
        theirs_present,
    })
}

/// Write resolved content to disk and reset the index entry to HEAD (unstaged).
///
/// Used for stash conflict resolution: conflict markers are cleared from the
/// index but the file is NOT staged — it appears as a plain working-tree
/// modification so the user can review and stage it manually.
///
/// `encoding` is the label returned by `get_conflict_content` (`"UTF-8"` for
/// modern files, `"windows-1252"` for legacy Latin-1 sources). Pass `None`
/// to default to UTF-8.
pub fn resolve_stash_conflict(
    repo: &mut Repository,
    rel_path: &str,
    content: &str,
    encoding: Option<&str>,
) -> Result<()> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository".into()))?
        .to_path_buf();

    // 1. Write the resolved content to the working tree, re-encoded to the
    //    file's original byte encoding so we don't silently rewrite a
    //    windows-1252 file as UTF-8.
    let bytes = encode_for_disk(content, encoding);
    std::fs::write(workdir.join(rel_path), &bytes)
        .map_err(|e| AppError::Other(format!("cannot write '{rel_path}': {e}")))?;

    // 2. Drop the conflict (stage 1/2/3) entries from the index.  The exact
    //    mechanism depends on whether the path also exists in HEAD:
    //
    //    a. Path IS in HEAD → `git reset HEAD <path>` re-installs the HEAD
    //       version as stage-0, dropping the unmerged stages.  This matches
    //       what plain `git stash apply` resolution does.
    //
    //    b. Path is NOT in HEAD (it was *added* by the stash — typical of
    //       `added by them` conflicts) → `reset_default` fails with
    //       "invalid entry mode" because libgit2 has no HEAD entry to read
    //       a valid mode from.  In that case we just yank every stage entry
    //       for the path so it becomes a plain untracked working-tree file,
    //       which is exactly what the user means by "keep the stash's
    //       file" for an added-by-stash conflict.
    //
    //    Without (b) the first click failed with the cryptic
    //    `invalid entry mode; class=Index (10)` toast, then "magically"
    //    succeeded on the second click because libgit2 had cached a mode
    //    from the workdir during the failed first attempt.
    let head_obj = repo
        .revparse_single("HEAD")
        .map_err(|e| AppError::Other(format!("cannot resolve HEAD: {e}")))?;

    let path_in_head = head_obj
        .peel_to_tree()
        .ok()
        .and_then(|t| t.get_path(std::path::Path::new(rel_path)).ok())
        .is_some();

    if path_in_head {
        repo.reset_default(Some(&head_obj), std::iter::once(rel_path))
            .map_err(|e| AppError::Other(format!("git reset HEAD '{rel_path}' failed: {e}")))?;
    } else {
        // Drop every stage entry for this path — equivalent to
        // `git rm --cached --force <path>` for the unmerged stages only.
        // Leaves the workdir file (which we just wrote) untouched and
        // unstaged.
        let mut index = repo.index()?;
        index.read(true)?;
        // remove_path drops stage-0; conflict_remove drops 1/2/3.
        // Either may fail if the corresponding entry is absent — both
        // failures are benign here (it just means there was nothing to
        // remove), so we swallow them.
        let _ = index.remove_path(std::path::Path::new(rel_path));
        let _ = index.conflict_remove(std::path::Path::new(rel_path));
        index.write()
            .map_err(|e| AppError::Other(format!("could not write index after dropping unmerged stages for '{rel_path}': {e}")))?;
    }

    Ok(())
}

/// Resolve a conflicted path by removing the file: deletes the working
/// tree copy (if present) and drops every index entry (stages 0/1/2/3).
/// Used for modify/delete and add/modify conflicts where the user picks
/// "accept deletion" — equivalent to `git rm` on the path.
pub fn remove_conflict_file(repo: &mut Repository, rel_path: &str) -> Result<()> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository".into()))?
        .to_path_buf();

    let abs = workdir.join(rel_path);
    if abs.exists() {
        std::fs::remove_file(&abs)
            .map_err(|e| AppError::Other(format!("cannot delete '{rel_path}': {e}")))?;
    }

    let mut index = repo.index()?;
    // remove_path drops every stage (1/2/3 for conflicts, 0 for normal entry).
    index.remove_path(std::path::Path::new(rel_path))?;
    index.write()?;
    Ok(())
}

/// Write resolved content to disk and stage the file (removing conflict entries).
///
/// `encoding` is the label returned by `get_conflict_content` — see
/// `resolve_stash_conflict` for details.
pub fn resolve_conflict(
    repo: &mut Repository,
    rel_path: &str,
    content: &str,
    encoding: Option<&str>,
) -> Result<()> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository".into()))?
        .to_path_buf();

    let bytes = encode_for_disk(content, encoding);
    std::fs::write(workdir.join(rel_path), &bytes)
        .map_err(|e| AppError::Other(format!("cannot write '{rel_path}': {e}")))?;

    let mut index = repo.index()?;
    index.add_path(std::path::Path::new(rel_path))?;
    index.write()?;
    Ok(())
}

/// Finalise conflict resolution.
///
/// Two paths:
/// - **Real merge**: `MERGE_HEAD` exists → create the merge commit with two
///   parents (HEAD + MERGE_HEAD) and return its OID.
/// - **Orphan conflict state**: `MERGE_HEAD` is absent (the index carried
///   unmerged entries from an aborted operation or external tampering).
///   There is no merge to commit here — each file's resolution has already
///   been staged by `resolve_conflict`, so we simply return `Ok("")` to
///   signal "nothing to commit, the state is clean". The frontend shows a
///   friendly message and closes the modal; the user can then commit the
///   staged changes from the Stage area if they want.
pub fn complete_merge(repo: &mut Repository, message: &str) -> Result<String> {
    let mut index = repo.index()?;
    index.read(true)?; // reload from disk to pick up all staged resolutions

    if index.has_conflicts() {
        return Err(AppError::Other(
            "there are still unresolved conflicts — resolve all files before completing the merge"
                .into(),
        ));
    }

    let merge_head_path = repo.path().join("MERGE_HEAD");
    if !merge_head_path.exists() {
        // Orphan-conflict path: nothing to commit as a merge.
        return Ok(String::new());
    }

    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let head_oid = repo
        .head()?
        .target()
        .ok_or_else(|| AppError::Other("HEAD has no target OID".into()))?;
    let head_commit = repo.find_commit(head_oid)?;

    let merge_head_sha = std::fs::read_to_string(&merge_head_path)
        .map_err(|_| AppError::Other("no MERGE_HEAD file found".into()))?;
    let merge_oid = git2::Oid::from_str(merge_head_sha.trim())
        .map_err(|_| AppError::Other("invalid OID in MERGE_HEAD".into()))?;
    let merge_commit = repo.find_commit(merge_oid)?;

    let sig = repo.signature()?;
    let oid = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        message,
        &tree,
        &[&head_commit, &merge_commit],
    )?;

    repo.cleanup_state()?;
    Ok(oid.to_string())
}

/// Outcome of a clean merge — distinguishes the "nothing changed" cases from
/// real merges so the UI can show an appropriate toast.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MergeOutcome {
    /// HEAD already contained the merged branch — nothing to do.
    AlreadyUpToDate,
    /// HEAD was fast-forwarded — no merge commit created.
    FastForward,
    /// A merge commit was created.
    Merged,
    /// `--squash` strategy: source changes are staged but no commit was
    /// created. The user must commit manually from the Stage area.
    Squashed,
}

/// Strategy flag for [`merge_branch`].
///
/// Maps 1:1 to the equivalent `git merge` flag combination — kept as an
/// enum so the IPC surface is typed and the frontend cannot pass freeform
/// strings.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// `git merge --no-edit` — fast-forward when possible, otherwise merge commit.
    #[default]
    Default,
    /// `git merge --no-ff --no-edit` — always create a merge commit.
    NoFf,
    /// `git merge --ff-only --no-edit` — refuse if a fast-forward is not possible.
    FfOnly,
    /// `git merge --squash` — flatten incoming changes into the index, no commit.
    Squash,
}

/// Merge `branch_name` into the current HEAD via the git CLI using the given
/// `strategy`.
///
/// Returns the [`MergeOutcome`] on a clean merge (no conflicts).
/// Returns `Err` whose message starts with `"CONFLICTS:"` when the merge
/// produces conflicts, so the caller can distinguish it from a hard failure
/// and redirect the user to the conflict resolver.
pub fn merge_branch(
    workdir: &std::path::Path,
    branch_name: &str,
    strategy: MergeStrategy,
) -> Result<MergeOutcome> {
    // Capture HEAD before the merge so we can tell whether anything moved.
    let head_before = crate::git_cli::command()
        .args(["rev-parse", "HEAD"])
        .current_dir(workdir)
        .no_window()
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    // Build the argv based on the strategy.  `--squash` is incompatible with
    // `--no-edit` (squash never opens an editor anyway), the others all use
    // `--no-edit` to avoid blocking on a TTY.
    let args: Vec<&str> = match strategy {
        MergeStrategy::Default => vec!["merge", "--no-edit", branch_name],
        MergeStrategy::NoFf    => vec!["merge", "--no-ff", "--no-edit", branch_name],
        MergeStrategy::FfOnly  => vec!["merge", "--ff-only", "--no-edit", branch_name],
        MergeStrategy::Squash  => vec!["merge", "--squash", branch_name],
    };

    let out = crate::git_cli::command()
        .args(&args)
        .current_dir(workdir)
        .no_window()
        .output()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;

    if !out.status.success() {
        let stdout   = String::from_utf8_lossy(&out.stdout);
        let stderr   = String::from_utf8_lossy(&out.stderr);
        let combined = format!("{stdout}{stderr}").trim().to_string();
        if combined.contains("Automatic merge failed") || combined.contains("CONFLICT") {
            return Err(AppError::Other(format!("CONFLICTS:{combined}")));
        }
        return Err(AppError::Other(combined));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);

    // Squash never moves HEAD, never produces a merge commit — git prints
    // "Squash commit -- not updating HEAD".  Surface it as a dedicated
    // outcome so the UI can prompt the user to finalise the commit.
    if matches!(strategy, MergeStrategy::Squash) {
        if stdout.contains("Already up to date") || stdout.contains("Already up-to-date") {
            return Ok(MergeOutcome::AlreadyUpToDate);
        }
        return Ok(MergeOutcome::Squashed);
    }

    if stdout.contains("Already up to date") || stdout.contains("Already up-to-date") {
        return Ok(MergeOutcome::AlreadyUpToDate);
    }

    // Fast-forward vs merge commit: git prints "Fast-forward" for FF, otherwise
    // creates a merge commit. Confirm via HEAD parents — a merge commit has
    // 2+ parents.
    if stdout.contains("Fast-forward") {
        return Ok(MergeOutcome::FastForward);
    }

    // Fall back to a HEAD-moved comparison so we don't mis-report
    // language-localised git outputs as plain Merge.
    let head_after = crate::git_cli::command()
        .args(["rev-parse", "HEAD"])
        .current_dir(workdir)
        .no_window()
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if head_before.is_some() && head_before == head_after {
        return Ok(MergeOutcome::AlreadyUpToDate);
    }

    Ok(MergeOutcome::Merged)
}

// ---------------------------------------------------------------------------
// MR conflict-resolution prep — phased, streamable
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

/// Abort whatever merge-like operation is currently in progress.
///
/// Dispatches on the sentinel files in `.git/`:
///   - `CHERRY_PICK_HEAD` → `git cherry-pick --abort`
///   - `REVERT_HEAD`      → `git revert --abort`
///   - `REBASE_HEAD` or `rebase-merge/` or `rebase-apply/` → `git rebase --abort`
///   - `MERGE_HEAD`       → `git merge --abort`
///
/// Falls back to restoring every unmerged path from HEAD when none of those
/// are present but the index still has unmerged entries (residue from an
/// aborted operation or external tampering) — the semantic equivalent of
/// "abort" for an orphan-conflict state.
pub fn abort_merge(workdir: &std::path::Path) -> Result<()> {
    let gitdir = workdir.join(".git");

    // Dispatch tables so the per-op fork stays readable.
    // (gitdir marker, subcommand args for `git …`, human label for errors)
    let ops: &[(&str, &[&str], &str)] = &[
        ("CHERRY_PICK_HEAD", &["cherry-pick", "--abort"], "cherry-pick"),
        ("REVERT_HEAD",      &["revert",      "--abort"], "revert"),
    ];
    for (marker, args, label) in ops {
        if gitdir.join(marker).exists() {
            let out = crate::git_cli::command()
                .args(*args)
                .current_dir(workdir)
                .no_window()
                .output()
                .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(AppError::Other(format!(
                    "git {label} --abort failed: {stderr}",
                )));
            }
            return Ok(());
        }
    }

    // Rebase uses either a single sentinel file or a directory.
    if gitdir.join("REBASE_HEAD").exists()
        || gitdir.join("rebase-merge").exists()
        || gitdir.join("rebase-apply").exists()
    {
        let out = crate::git_cli::command()
            .args(["rebase", "--abort"])
            .current_dir(workdir)
            .no_window()
            .output()
            .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(AppError::Other(format!("git rebase --abort failed: {stderr}")));
        }
        return Ok(());
    }

    // Regular merge.
    if gitdir.join("MERGE_HEAD").exists() {
        let out = crate::git_cli::command()
            .args(["merge", "--abort"])
            .current_dir(workdir)
            .no_window()
            .output()
            .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(AppError::Other(format!("git merge --abort failed: {stderr}")));
        }
        return Ok(());
    }

    // Orphan case: gather unmerged paths from the index.
    let ls = crate::git_cli::command()
        .args(["ls-files", "--unmerged"])
        .current_dir(workdir)
        .no_window()
        .output()
        .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;

    if !ls.status.success() {
        let stderr = String::from_utf8_lossy(&ls.stderr);
        return Err(AppError::Other(format!("git ls-files --unmerged failed: {stderr}")));
    }

    // Each line is "<mode> <sha> <stage>\t<path>" — one line per stage,
    // so dedupe by path.
    let stdout = String::from_utf8_lossy(&ls.stdout);
    let mut paths: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for line in stdout.lines() {
        if let Some(p) = line.split('\t').nth(1) {
            paths.insert(p.to_string());
        }
    }

    if paths.is_empty() {
        return Err(AppError::Other(
            "Nothing to abort: no merge in progress and no unmerged index entries.".into(),
        ));
    }

    // Split paths by whether they exist in HEAD.  "Added by them" files
    // (typical of stash-apply / cherry-pick conflicts where the incoming
    // side introduces a brand-new file) have no HEAD blob to restore from,
    // so `git checkout HEAD -- <path>` fails with "pathspec did not match
    // any file(s)" and aborts the whole batch.  Resolve them by removing
    // the file outright — there is no local content to lose because the
    // workdir copy is entirely from the incoming side.
    let mut exists_in_head: Vec<String> = Vec::new();
    let mut absent_in_head: Vec<String> = Vec::new();
    for p in &paths {
        let out = crate::git_cli::command()
            .args(["cat-file", "-e", &format!("HEAD:{p}")])
            .current_dir(workdir)
            .no_window()
            .output();
        match out {
            Ok(o) if o.status.success() => exists_in_head.push(p.clone()),
            _ => absent_in_head.push(p.clone()),
        }
    }

    let mut errors: Vec<String> = Vec::new();

    if !exists_in_head.is_empty() {
        let mut cmd = crate::git_cli::command();
        cmd.args(["checkout", "HEAD", "--"])
            .current_dir(workdir)
            .no_window();
        for p in &exists_in_head {
            cmd.arg(p);
        }
        let out = cmd.output()
            .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;
        if !out.status.success() {
            errors.push(format!(
                "restore from HEAD: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }

    if !absent_in_head.is_empty() {
        // `git rm -f` drops the file from both index (stages 1/2/3) and
        // working tree in one shot.  We fall back to a manual loop when
        // the batch call fails because one path in the list cannot be
        // removed — that way the other "added by them" paths still get
        // cleaned up instead of leaving the repo half-aborted.
        let mut cmd = crate::git_cli::command();
        cmd.args(["rm", "-f", "--"])
            .current_dir(workdir)
            .no_window();
        for p in &absent_in_head {
            cmd.arg(p);
        }
        let out = cmd.output()
            .map_err(|e| AppError::Other(format!("failed to spawn git: {e}")))?;
        if !out.status.success() {
            for p in &absent_in_head {
                let _ = crate::git_cli::command()
                    .args(["update-index", "--remove", "--force-remove", p])
                    .current_dir(workdir)
                    .no_window()
                    .output();
                let _ = std::fs::remove_file(workdir.join(p));
            }
            // Verify: any path still in the unmerged list after the
            // fallback counts as a genuine failure worth surfacing.
            let recheck = crate::git_cli::command()
                .args(["ls-files", "--unmerged"])
                .current_dir(workdir)
                .no_window()
                .output();
            if let Ok(o) = recheck {
                let remaining = String::from_utf8_lossy(&o.stdout);
                let still: Vec<&str> = remaining.lines()
                    .filter_map(|l| l.split('\t').nth(1))
                    .filter(|p| absent_in_head.iter().any(|a| a == p))
                    .collect();
                if !still.is_empty() {
                    errors.push(format!(
                        "remove added-by-them files: {} still unmerged",
                        still.join(", ")
                    ));
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(AppError::Other(format!(
            "Abort partially failed: {}",
            errors.join("; ")
        )));
    }

    Ok(())
}

/// Read the pre-filled merge commit message from `.git/MERGE_MSG`.
///
/// Strips Git's auto-appended "Conflicts:" section (and the indented file
/// list that follows) — the user already sees the conflicting files in
/// the resolution UI, so repeating them in the commit message is noise.
pub fn get_merge_message(repo: &Repository) -> Result<String> {
    let path = repo.path().join("MERGE_MSG");
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| AppError::Other("no MERGE_MSG file".into()))?;
    let cleaned: String = raw
        .lines()
        .take_while(|line| {
            let l = line.trim_start_matches('#').trim_start();
            !l.starts_with("Conflicts:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(cleaned.trim().to_string())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read the content of a file at the given index stage (1=base, 2=ours,
/// 3=theirs) and decode it using the supplied `encoding`. The encoding is
/// chosen once per conflict (from the working-tree file) so the three-way
/// view stays internally consistent.
fn stage_blob_with_encoding(
    repo: &Repository,
    index: &git2::Index,
    path: &str,
    stage: i32,
    encoding: &'static encoding_rs::Encoding,
) -> Result<Option<String>> {
    for i in 0..index.len() {
        if let Some(entry) = index.get(i) {
            let entry_stage = ((entry.flags >> 12) & 0x3) as i32;
            let entry_path = std::str::from_utf8(&entry.path).unwrap_or("");
            if entry_stage == stage && entry_path == path {
                let blob = repo.find_blob(entry.id)?;
                return Ok(Some(decode_with(blob.content(), encoding)));
            }
        }
    }
    Ok(None)
}

/// Try to find a human-readable label for MERGE_HEAD (branch name or short SHA).
fn merge_head_label(repo: &Repository) -> Result<String> {
    let sha = std::fs::read_to_string(repo.path().join("MERGE_HEAD"))
        .map_err(|_| AppError::Other("no MERGE_HEAD".into()))?;
    let sha = sha.trim().to_string();

    if let Ok(oid) = git2::Oid::from_str(&sha) {
        if let Ok(refs) = repo.references() {
            for r in refs.flatten() {
                if r.is_branch() && r.target() == Some(oid) {
                    if let Some(name) = r.shorthand() {
                        return Ok(name.to_string());
                    }
                }
            }
        }
    }

    // Fallback: first 7 chars of the SHA
    Ok(sha.chars().take(7).collect())
}
