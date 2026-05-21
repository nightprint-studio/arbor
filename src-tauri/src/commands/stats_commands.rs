use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use crate::error::AppError;
use crate::AppState;

fn stats_finish_job(app_handle: &tauri::AppHandle, job_id: &str, success: bool, message: &str) {
    let state = app_handle.state::<crate::AppState>();
    if let Ok(mut jobs) = state.jobs.lock() {
        let status = if success {
            crate::jobs::JobStatus::Completed { exit_code: 0 }
        } else {
            crate::jobs::JobStatus::Failed { error: message.to_string() }
        };
        jobs.set_status(job_id, status);
    }
    let _ = app_handle.emit("arbor://job-done", serde_json::json!({
        "job_id":    job_id,
        "success":   success,
        "exit_code": if success { 0i32 } else { -1i32 },
        "cancelled": false,
    }));
    let (title, level) = if success {
        ("Stats export complete", "success")
    } else {
        ("Stats export failed", "error")
    };
    let _ = app_handle.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   title,
        "message": message,
        "level":   level,
    }));
}

/// Export repository statistics to a JSON or HTML file.
///
/// Returns a job-id immediately; the export runs in a background thread.
/// Emits `arbor://job-done` and `plugin:notification` on completion.
#[tauri::command]
pub async fn export_repo_stats(
    tab_id: String,
    output_path: String,
    format: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<String, AppError> {
    use crate::jobs::{JobInfo, JobRegistry, JobStatus};

    // Grab repo path + name and check the stats cache — no mutex held into bg thread.
    let (repo_path, repo_name, cached_stats) = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let inner = repo.inner();
        let path = inner
            .workdir()
            .unwrap_or_else(|| inner.path())
            .to_path_buf();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| tab_id.clone());
        drop(mgr);

        let cached = state.stats_cache.lock()
            .ok()
            .and_then(|c| c.get(&tab_id).map(|(_, s)| s.clone()));
        (path, name, cached)
    };

    // Register a job entry so it appears in the Jobs overlay immediately.
    let job_id = {
        let mut jobs = state.jobs.lock()
            .map_err(|_| AppError::Other("mutex poisoned".into()))?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            format!("Export Stats as {}", format.to_uppercase()),
            plugin_name:     "arbor".into(),
            command:         format!("→ {output_path}"),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("Export".into()),
            non_cancellable: true,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
        });
        id
    };

    let _ = app.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        format!("Export Stats as {}", format.to_uppercase()),
        "plugin_name": "arbor",
        "command":     format!("→ {output_path}"),
        "category":    "Export",
    }));

    let jid = job_id.clone();
    let ah  = app.clone();

    tokio::task::spawn_blocking(move || {
        let emit_line = |line: &str| {
            let s = ah.state::<crate::AppState>();
            if let Ok(mut jobs) = s.jobs.lock() {
                jobs.append_output(&jid, line.to_string());
            }
            let _ = ah.emit("arbor://job-output", serde_json::json!({
                "job_id": &jid,
                "text":   line,
            }));
        };

        // Use cached stats if available, otherwise compute fresh.
        let stats = if let Some(s) = cached_stats {
            emit_line("Using cached statistics…");
            s
        } else {
            emit_line("Computing repository statistics…");
            let repo = match git2::Repository::open(&repo_path) {
                Ok(r)  => r,
                Err(e) => {
                    let err = format!("Cannot open repo: {e}");
                    emit_line(&format!("[error] {err}"));
                    stats_finish_job(&ah, &jid, false, &err);
                    return;
                }
            };
            let excl = crate::config::repo_config::load(&repo_path.to_string_lossy())
                .map(|c| c.stats_exclude)
                .unwrap_or_default();
            match crate::git::stats::compute_stats(&repo, &excl) {
                Ok(s)  => s,
                Err(e) => {
                    let err = format!("Failed to compute stats: {e}");
                    emit_line(&format!("[error] {err}"));
                    stats_finish_job(&ah, &jid, false, &err);
                    return;
                }
            }
        };

        emit_line(&format!("Writing {format} export…"));
        // Honour any plugin-supplied logo override so co-branded exports
        // pick up the same logo the user sees in-app.
        let logo_override = ah.state::<crate::AppState>().branding.snapshot().logo_svg;
        match crate::git::stats_export::export_to_file(
            &stats,
            std::path::Path::new(&output_path),
            &format,
            &repo_name,
            logo_override.as_deref(),
        ) {
            Ok(()) => {
                let ok_msg = format!("Stats exported to '{output_path}'.");
                emit_line(&ok_msg);
                stats_finish_job(&ah, &jid, true, &ok_msg);
            }
            Err(e) => {
                emit_line(&format!("[error] {e}"));
                stats_finish_job(&ah, &jid, false, &e);
            }
        }
    });

    Ok(job_id)
}

/// Kick off a background stats computation for the given tab.
///
/// Returns immediately (Ok). The result arrives as a Tauri event:
///   - `arbor://repo-stats-ready`  { tab_id, stats }  — success
///   - `arbor://repo-stats-error`  { tab_id, error }  — failure
///
/// If the current HEAD matches the cached SHA the event is emitted
/// synchronously from the cached value — no thread is spawned.
/// If a computation is already running for that tab, this call is a no-op.
#[tauri::command]
pub async fn compute_repo_stats(
    tab_id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), AppError> {
    // ── 1. Extract the repo path, HEAD sha and exclusion config ─────────────
    let (repo_path, head_sha, exclude) = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let inner = repo.inner();
        let sha = inner
            .head()
            .ok()
            .and_then(|h| h.target())
            .map(|oid| oid.to_string())
            .unwrap_or_default();
        // workdir() is None for bare repos; fall back to path() (.git dir).
        let path = inner
            .workdir()
            .unwrap_or_else(|| inner.path())
            .to_path_buf();
        let excl = crate::config::repo_config::load(&path.to_string_lossy())
            .map(|c| c.stats_exclude)
            .unwrap_or_default();
        (path, sha, excl)
    };

    // Cache key includes a fingerprint of the exclusion config so that
    // changing exclusions always invalidates the cached result.
    let exclude_key = format!(
        "ext:{};folders:{};files:{}",
        exclude.extensions.join(","),
        exclude.folders.join(","),
        exclude.files.join(","),
    );
    let cache_key = format!("{head_sha}|{exclude_key}");

    // ── 2. Return cached result immediately if HEAD + exclusions haven't changed
    {
        let cache = state.stats_cache.lock().map_err(|_| AppError::MutexPoisoned("stats_cache".into()))?;
        if let Some((cached_key, cached_stats)) = cache.get(&tab_id) {
            if *cached_key == cache_key {
                let stats = cached_stats.clone();
                drop(cache);
                let _ = app.emit("arbor://repo-stats-ready", serde_json::json!({
                    "tab_id": &tab_id,
                    "stats": stats,
                }));
                return Ok(());
            }
        }
    }

    // ── 3. Guard against duplicate concurrent runs ────────────────────────────
    {
        let mut computing = state.stats_computing.lock()
            .map_err(|_| AppError::MutexPoisoned("stats_computing".into()))?;
        if computing.contains(&tab_id) {
            return Ok(()); // already in progress — last one wins when it finishes
        }
        computing.insert(tab_id.clone());
    }

    // ── 4. Clone Arcs and spawn the background thread ─────────────────────────
    let cache_arc     = Arc::clone(&state.stats_cache);
    let computing_arc = Arc::clone(&state.stats_computing);
    let tab_id_bg     = tab_id.clone();

    std::thread::spawn(move || {
        let result = (|| -> std::result::Result<crate::git::stats::RepoStats, Box<dyn std::error::Error + Send + Sync>> {
            let repo = git2::Repository::open(&repo_path)?;
            Ok(crate::git::stats::compute_stats(&repo, &exclude)?)
        })();

        // Always unmark as computing, even on error.
        if let Ok(mut computing) = computing_arc.lock() {
            computing.remove(&tab_id_bg);
        }

        match result {
            Ok(stats) => {
                if let Ok(mut cache) = cache_arc.lock() {
                    cache.insert(tab_id_bg.clone(), (cache_key, stats.clone()));
                }
                let _ = app.emit("arbor://repo-stats-ready", serde_json::json!({
                    "tab_id": &tab_id_bg,
                    "stats": stats,
                }));
            }
            Err(e) => {
                tracing::error!("stats computation failed for tab {tab_id_bg}: {e}");
                let _ = app.emit("arbor://repo-stats-error", serde_json::json!({
                    "tab_id": &tab_id_bg,
                    "error": e.to_string(),
                }));
            }
        }
    });

    Ok(())
}
