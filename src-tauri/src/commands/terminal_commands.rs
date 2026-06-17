use tauri::State;

use crate::AppState;
use crate::error::Result;
use crate::terminal::{self, BUILTIN_SHELLS, DetectedShell, TerminalInfo};

// ---------------------------------------------------------------------------
// terminal_create
// ---------------------------------------------------------------------------
//
// DEFERRED from the `platform` broker migration: takes an `AppHandle` and
// spawns a PTY that streams its output via the `arbor://terminal-*` events.
// Handled by a later emit/seam pass.

/// Spawn a new PTY process and return a TerminalInfo with its UUID.
///
/// `shell` is a shell **id** from the built-in catalogue (cmd, powershell,
/// pwsh, bash, git-bash, …) or a user-defined custom-shell id.  When empty /
/// missing the user's default-shell is used (or the platform default).
#[tauri::command]
pub async fn terminal_create(
    state: State<'_, AppState>,
    app:   tauri::AppHandle,
    shell: Option<String>,
    cwd:   Option<String>,
    cols:  Option<u16>,
    rows:  Option<u16>,
) -> Result<TerminalInfo> {
    let (exe, args, display_name) = {
        let cfg = state.lock_config()?;
        let (exe, args) = terminal::resolve_shell(shell.as_deref(), &cfg.terminals);
        let display = display_name_for(shell.as_deref(), &exe, &cfg.terminals);
        (exe, args, display)
    };

    let working_dir = cwd
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string())
        });

    let cols = cols.unwrap_or(120);
    let rows = rows.unwrap_or(30);

    let mut mgr = state.lock_terminals()?;
    mgr.create(exe, args, display_name, working_dir, cols, rows, app)
}

/// Resolve a friendly display name for a shell id (built-in name, custom
/// name, or fall back to the executable basename).  Falls through to the
/// configured default-shell when `id` is missing/empty.
fn display_name_for(
    id: Option<&str>,
    exe: &str,
    cfg: &crate::config::app_config::TerminalsConfig,
) -> String {
    let resolved = id.map(str::trim).filter(|s| !s.is_empty())
        .or_else(|| cfg.default_shell.as_deref().map(str::trim).filter(|s| !s.is_empty()));

    if let Some(id) = resolved {
        if let Some(custom) = cfg.custom_shells.iter().find(|s| s.id == id) {
            return custom.name.clone();
        }
        if let Some(b) = BUILTIN_SHELLS.iter().find(|s| s.id == id) {
            return b.name.to_string();
        }
    }
    std::path::Path::new(exe)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| exe.to_string())
}

// ---------------------------------------------------------------------------
// Shell detection
// ---------------------------------------------------------------------------
//
// DEFERRED from the `platform` broker migration: takes an `AppHandle` and emits
// `arbor://job-*` / `arbor://shell-detection-done`. Handled by a later
// emit/seam pass.

/// Kick off shell detection as a non-cancellable background job — mirrors
/// `start_ide_detection`.  Results arrive via `arbor://shell-detection-done`.
#[tauri::command]
pub fn start_shell_detection(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String> {
    use tauri::Emitter;
    use crate::jobs::{JobInfo, JobRegistry, JobStatus};

    let path_overrides = {
        let cfg = state.lock_config()?;
        cfg.terminals.path_overrides.clone()
    };

    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            "Shell Detection".to_string(),
            plugin_name:     "arbor".to_string(),
            command:         "detect shells".to_string(),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("System".to_string()),
            non_cancellable: true,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
            target:          None,
        });
        id
    };

    let _ = app_handle.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        "Shell Detection",
        "plugin_name": "arbor",
        "command":     "detect shells",
        "category":    "System",
    }));

    let jid    = job_id.clone();
    let handle = app_handle.clone();
    let _thread = std::thread::Builder::new()
        .name("arbor-shell-detection".into())
        .spawn(move || {
            use tauri::Manager as _;
            let results: Vec<DetectedShell> =
                terminal::detect_available_shells(&path_overrides);

            for r in &results {
                let line = if r.available {
                    format!("✓  {} — {}", r.name, r.detected_path.as_deref().unwrap_or(""))
                } else {
                    format!("✗  {} — not found", r.name)
                };
                let s = handle.state::<AppState>();
                if let Ok(mut jobs) = s.jobs.lock() {
                    jobs.append_output(&jid, line.clone());
                };
                let _ = handle.emit("arbor://job-output", serde_json::json!({
                    "job_id": &jid,
                    "text":   line,
                }));
            }

            {
                let s = handle.state::<AppState>();
                if let Ok(mut jobs) = s.jobs.lock() {
                    jobs.set_status(&jid, JobStatus::Completed { exit_code: 0 });
                };
            }

            let _ = handle.emit("arbor://job-done", serde_json::json!({
                "job_id":    &jid,
                "success":   true,
                "exit_code": 0,
            }));
            let _ = handle.emit("arbor://shell-detection-done", &results);
        });

    Ok(job_id)
}
