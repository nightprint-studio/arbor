use tauri::State;

use crate::error::AppError;
use crate::process_ext::NoWindowExt;
use crate::AppState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

// Delegates to the `git` CLI instead of libgit2's `Repository::reset`. The
// vendored libgit2 version bundled via `vendored-libgit2` has quirks that
// caused `r.reset()` to behave like a checkout (move HEAD without moving the
// current branch ref), which defeats the purpose of soft/mixed/hard resets.
// The CLI path is what users expect and matches the pattern already used for
// merge/rebase in this codebase.
#[tauri::command]
pub fn reset_to_commit(
    state: State<'_, AppState>,
    tab_id: String,
    oid: String,
    mode: ResetMode,
) -> Result<(), AppError> {
    // Validate the OID before spawning a subprocess.
    let git_oid =
        git2::Oid::from_str(&oid).map_err(|_| AppError::CommitNotFound(oid.clone()))?;

    // Extract workdir + run the hard-reset safety snapshot while we still
    // hold the repo, then release the lock before calling the CLI so libgit2
    // does not keep a stale view of HEAD/refs across the subprocess.
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get_mut(&tab_id)?;
        let r = repo.inner_mut();

        // Confirm the OID resolves to a commit in this repo before shelling
        // out — gives a cleaner error than a cryptic CLI failure.
        let _ = r.find_object(git_oid, Some(git2::ObjectType::Commit))?;

        if matches!(mode, ResetMode::Hard) {
            let short = oid.get(..7).unwrap_or(&oid);
            crate::git::recovery::try_snapshot(
                r,
                crate::git::recovery::RecoveryKind::ResetHard,
                format!("reset --hard to {short}"),
            );
        }

        r.workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    };

    let flag = match mode {
        ResetMode::Soft  => "--soft",
        ResetMode::Mixed => "--mixed",
        ResetMode::Hard  => "--hard",
    };

    tracing::info!("reset_to_commit: running `git reset {flag} {oid}` in {}", workdir.display());

    let out = crate::git_cli::command()
        .args(["reset", flag, &oid])
        .current_dir(&workdir)
        .no_window()
        .output()
        .map_err(AppError::Io)?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    tracing::info!(
        "reset_to_commit: exit={:?} stdout={:?} stderr={:?}",
        out.status.code(), stdout.trim(), stderr.trim()
    );

    if !out.status.success() {
        let msg = if !stderr.trim().is_empty() { stderr.to_string() } else { stdout.to_string() };
        return Err(AppError::Other(format!("git reset failed: {}", msg.trim())));
    }

    Ok(())
}

#[tauri::command]
pub fn create_tag(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
    oid: String,
    message: Option<String>,
) -> Result<(), AppError> {
    let annotated = message.is_some();
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let r = repo.inner();
        let git_oid =
            git2::Oid::from_str(&oid).map_err(|_| AppError::CommitNotFound(oid.clone()))?;
        let obj = r.find_object(git_oid, Some(git2::ObjectType::Commit))?;
        if let Some(msg) = message {
            let sig = r.signature()?;
            r.tag(&name, &obj, &sig, &msg, false)?;
        } else {
            r.tag_lightweight(&name, &obj, false)?;
        }
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({
            "tab_id":    &tab_id,
            "name":      &name,
            "oid":       &oid,
            "annotated": annotated,
        });
        let _ = host.fire_hook("on_tag_create", &ctx.to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn delete_tag(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
) -> Result<(), AppError> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner().tag_delete(&name)?;
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "tab_id": &tab_id, "name": &name });
        let _ = host.fire_hook("on_tag_delete", &ctx.to_string());
    }
    Ok(())
}
