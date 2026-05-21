use std::path::Path;
use tauri::{Manager, State};

use crate::config::app_config;
use crate::error::AppError;
use crate::git::worktree::{self, ProjectType, WorktreeInfo, BUILTIN_IDES};
use crate::AppState;
use crate::process_ext::NoWindowExt;

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_worktrees(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<WorktreeInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let repo_path = Path::new(&repo.path);
    worktree::list_worktrees(repo_path, repo_path)
}

// ---------------------------------------------------------------------------
// Add
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn add_worktree(
    state: State<'_, AppState>,
    tab_id: String,
    dest_path: String,
    branch: String,
    new_branch: Option<String>,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let repo_path = Path::new(&repo.path);
    worktree::add_worktree(
        repo_path,
        &dest_path,
        &branch,
        new_branch.as_deref(),
    )
}

// ---------------------------------------------------------------------------
// Remove
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn remove_worktree(
    state: State<'_, AppState>,
    tab_id: String,
    worktree_path: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let repo_path = Path::new(&repo.path);
    worktree::remove_worktree(repo_path, &worktree_path)
}

// ---------------------------------------------------------------------------
// Detect project type (standalone, no repo required)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn detect_project_type(path: String) -> Result<ProjectType, AppError> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(AppError::Other(format!("Path does not exist: {path}")));
    }
    Ok(worktree::detect_project_type(p))
}

// ---------------------------------------------------------------------------
// IDE detection — background job
// ---------------------------------------------------------------------------

/// Kick off IDE detection as a non-cancellable background job.
/// Each IDE is probed in a detached thread; results are broadcast via
/// `arbor://ide-detection-done` once all probes complete.
/// Returns the assigned job_id so callers can correlate events if needed.
#[tauri::command]
pub fn start_ide_detection(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, AppError> {
    use tauri::Emitter;
    use crate::jobs::{JobInfo, JobRegistry, JobStatus};

    // Snapshot path overrides from config (don't hold the lock in the thread).
    let path_overrides = {
        let cfg = state.lock_config()?;
        cfg.ide.path_overrides.clone()
    };

    // Register the job.
    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            "IDE Detection".to_string(),
            plugin_name:     "arbor".to_string(),
            command:         "detect IDEs".to_string(),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("System".to_string()),
            non_cancellable: true,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
        });
        id
    };

    let _ = app_handle.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        "IDE Detection",
        "plugin_name": "arbor",
        "command":     "detect IDEs",
        "category":    "System",
    }));

    // Probe each IDE in a detached thread — never blocks the Tauri command thread.
    let jid   = job_id.clone();
    let handle = app_handle.clone();
    let _thread = std::thread::Builder::new()
        .name("arbor-ide-detection".into())
        .spawn(move || {
            use crate::git::worktree::DetectedIde;
            let mut results: Vec<DetectedIde> = Vec::with_capacity(BUILTIN_IDES.len());

            for ide in BUILTIN_IDES {
                // Resolve the command to probe.
                let cmd = match path_overrides.get(ide.id) {
                    Some(ov) if !ov.is_empty() => ov.clone(),
                    _                           => ide.cmd.to_string(),
                };

                // Probe: prefer explicit path check, then `which`/`where`.
                let (available, detected_path) = if Path::new(&cmd).exists() {
                    (true, Some(cmd.clone()))
                } else {
                    let found = probe_which(&cmd);
                    (found.is_some(), found)
                };

                let line = if available {
                    format!("✓  {} — {}", ide.name, detected_path.as_deref().unwrap_or(&cmd))
                } else {
                    format!("✗  {} — not found", ide.name)
                };

                // Append to ring-buffer.
                {
                    let s = handle.state::<AppState>();
                    if let Ok(mut jobs) = s.jobs.lock() {
                        jobs.append_output(&jid, line.clone());
                    };
                }
                let _ = handle.emit("arbor://job-output", serde_json::json!({
                    "job_id": &jid,
                    "text":   line,
                }));

                results.push(DetectedIde {
                    id:             ide.id.to_string(),
                    name:           ide.name.to_string(),
                    available,
                    detected_path,
                });
            }

            // Mark job complete.
            {
                let s = handle.state::<AppState>();
                if let Ok(mut jobs) = s.jobs.lock() {
                    jobs.set_status(&jid, JobStatus::Completed { exit_code: 0 });
                };
            }

            // Broadcast job-done + custom event carrying the results.
            let _ = handle.emit("arbor://job-done", serde_json::json!({
                "job_id":    &jid,
                "success":   true,
                "exit_code": 0,
            }));
            let _ = handle.emit("arbor://ide-detection-done", &results);
        });

    Ok(job_id)
}

/// Resolve a command via `which` (Unix) / `where` (Windows).
fn probe_which(cmd: &str) -> Option<String> {
    #[cfg(windows)]
    let prog = "where";
    #[cfg(not(windows))]
    let prog = "which";

    std::process::Command::new(prog)
        .arg(cmd)
        .no_window()
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.lines().next().map(|l| l.trim().to_string())
        })
}

// ---------------------------------------------------------------------------
// Open in IDE
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn open_in_ide(
    state: State<'_, AppState>,
    path: String,
    ide_id: Option<String>,
) -> Result<(), AppError> {
    let config = state.lock_config()?;
    let ide_cfg = config.ide.clone();
    drop(config);

    // Per-repo override (Settings → Project → External Integrations) wins
    // over the global default when the caller didn't pin a specific IDE.
    // Best-effort: a missing/unreadable `.arbor/config.toml` just falls
    // through to the global default, the original behavior.
    let repo_ide_id: Option<String> = if ide_id.is_none() {
        crate::config::repo_config::load(&path).ok().and_then(|c| c.ide_id)
    } else {
        None
    };

    let effective_id = ide_id.as_deref()
        .or(repo_ide_id.as_deref())
        .unwrap_or(&ide_cfg.default_ide)
        .to_owned();
    let (command, extra_args) = resolve_ide(&effective_id, &ide_cfg)?;
    worktree::open_in_ide(&path, &command, &extra_args)
}

// ---------------------------------------------------------------------------
// Per-repo IDE preference (`.arbor/config.toml` → `ide_id`)
// ---------------------------------------------------------------------------

/// Read the project-bound IDE preference, or `None` when the repo defers
/// to the global default. Convenience wrapper over `get_repo_config` so
/// the Settings panel doesn't have to round-trip the whole RepoConfig.
#[tauri::command]
pub fn get_repo_ide(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<Option<String>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(crate::config::repo_config::load(&repo.path)?.ide_id)
}

/// Persist (or clear) the project-bound IDE preference. Pass `None` to
/// remove the override and fall back to the global default.
#[tauri::command]
pub fn set_repo_ide(
    state:  State<'_, AppState>,
    tab_id: String,
    ide_id: Option<String>,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let mut cfg = crate::config::repo_config::load(&repo.path).unwrap_or_default();
    cfg.ide_id = ide_id.filter(|s| !s.is_empty());
    crate::config::repo_config::save(&repo.path, &cfg)
}

// ---------------------------------------------------------------------------
// IDE config get/set
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_ide_config(
    state: State<'_, AppState>,
) -> Result<crate::config::app_config::IdeConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.ide.clone())
}

#[tauri::command]
pub fn set_ide_config(
    state: State<'_, AppState>,
    config: crate::config::app_config::IdeConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.ide = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// Helper: resolve IDE command + args from config
// ---------------------------------------------------------------------------

fn resolve_ide(
    ide_id: &str,
    ide_cfg: &crate::config::app_config::IdeConfig,
) -> Result<(String, Vec<String>), AppError> {
    // 1. Custom user-defined IDEs
    if let Some(custom) = ide_cfg.custom_ides.iter().find(|c| c.id == ide_id) {
        return Ok((custom.command.clone(), custom.args.clone()));
    }

    // 2. Built-in IDE — check for a path override first
    if let Some(builtin) = BUILTIN_IDES.iter().find(|b| b.id == ide_id) {
        let cmd = if let Some(ov) = ide_cfg.path_overrides.get(ide_id) {
            if !ov.is_empty() { ov.clone() } else { builtin.cmd.to_owned() }
        } else {
            builtin.cmd.to_owned()
        };
        let args = builtin.args.iter().map(|s| s.to_string()).collect();
        return Ok((cmd, args));
    }

    Err(AppError::Other(format!("Unknown IDE '{ide_id}'")))
}
