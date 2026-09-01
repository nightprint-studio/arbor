//! `terminal` domain — non-streaming PTY/shell handlers routed through the
//! in-process `platform` broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[platform::handler(program = "platform")]` self-registers it under its own
//! function name. The PTY/shell work already lives in the reusable
//! [`crate::terminal`] module ([`TerminalManager`], the built-in shell
//! catalogue, the platform-default lookup), so handlers **delegate straight to
//! it** — behavior (locks held, config save, errors) is byte-identical.
//!
//! The original commands were `async fn` only to fit them onto Tauri's command
//! runtime — none awaits anything (the `TerminalManager` ops are sync, guarded
//! by the `AppState` mutex). The broker dispatches synchronously, so the
//! handlers are plain `fn`.
//!
//! `terminal_default_shell` / `list_builtin_shells` never touched `AppState`,
//! but the handler macro requires a context first arg, so they take
//! `_state: &AppState` and ignore it — same as the original parameter-less
//! commands.
//!
//! No hooks fire in this domain.
//!
//! `start_shell_detection` spawns a background detection job. It emits the
//! standard `arbor://job-*` events for the Jobs overlay and delivers the
//! detection result over the standardized [`Stream`] seam on base
//! `arbor://shell-detection` (`stream_id == job_id`): the final shell list rides
//! the `-done` event under `{ shells: [...] }` with the `{ stream_id, seq }`
//! envelope. Reached through the generic broker it holds only `&AppState`, so
//! the thread captures the **event sink** (`Arc<dyn EventSink>`) plus an `Arc`
//! to the job registry instead of an `AppHandle`.
//!
//! NOT migrated (stays inline in `terminal_commands`, handled by a later
//! emit/seam pass):
//!   * `terminal_create` — takes an `AppHandle` and spawns a PTY that streams
//!     output via the `arbor://terminal-*` events.

use std::sync::Arc;

use arbor_ipc::prelude::{EventSink, Stream};

use crate::error::AppError;
use crate::ipc::platform;
use crate::terminal::{self, BUILTIN_SHELLS, DetectedShell, TerminalInfo, TerminalManager};
use crate::AppState;

// ---------------------------------------------------------------------------
// write / resize / close / list
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn terminal_write(state: &AppState, id: String, data: String) -> Result<(), AppError> {
    let mut mgr = state.lock_terminals()?;
    mgr.write(&id, data.as_bytes())
}

#[platform::handler(program = "platform")]
fn terminal_resize(state: &AppState, id: String, cols: u16, rows: u16) -> Result<(), AppError> {
    let mut mgr = state.lock_terminals()?;
    mgr.resize(&id, cols, rows)
}

#[platform::handler(program = "platform")]
fn terminal_close(state: &AppState, id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_terminals()?;
    mgr.close(&id)
}

#[platform::handler(program = "platform")]
fn terminal_list(state: &AppState) -> Result<Vec<TerminalInfo>, AppError> {
    let mgr = state.lock_terminals()?;
    Ok(mgr.list())
}

#[platform::handler(program = "platform")]
fn terminal_default_shell(_state: &AppState) -> Result<String, AppError> {
    Ok(terminal::platform_default())
}

// ---------------------------------------------------------------------------
// terminal_exec  (plugin API + direct frontend use)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct TerminalExecResult {
    pub exit_code: i32,
    pub stdout:    String,
    pub stderr:    String,
}

#[platform::handler(program = "platform")]
fn terminal_exec(
    state:       &AppState,
    command:     String,
    cwd:         Option<String>,
    plugin_name: Option<String>,
) -> Result<TerminalExecResult, AppError> {
    if let Some(ref pname) = plugin_name {
        use arbor_plugin_types::prelude::TerminalLevel;
        let host = state.lock_plugin_host()?;

        let plugin = host.plugins.iter().find(|p| p.manifest.name == *pname);
        if let Some(p) = plugin {
            match p.manifest.permissions.terminal {
                TerminalLevel::None => {
                    return Err(AppError::Other(format!(
                        "plugin '{pname}' has no terminal permission (set terminal = \"any\" or terminal = \"commands\" in plugin.toml)"
                    )));
                }
                TerminalLevel::Any => { /* full access */ }
                TerminalLevel::Commands => {
                    let first = command.split_whitespace().next().unwrap_or("");
                    let allowed = &p.manifest.permissions.terminal_scope;
                    if !allowed.iter().any(|a| first.eq_ignore_ascii_case(a.as_str())) {
                        return Err(AppError::Other(format!(
                            "plugin '{pname}' is not allowed to run '{first}' \
                             (allowed commands: {allowed:?})"
                        )));
                    }
                }
            }
        }
    }

    let (exit_code, stdout, stderr) =
        TerminalManager::exec_command(&command, cwd.as_deref())?;

    Ok(TerminalExecResult { exit_code, stdout, stderr })
}

// ---------------------------------------------------------------------------
// Shell catalogue
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct BuiltinShellInfo {
    pub id:        String,
    pub name:      String,
    pub cmd:       String,
    pub platforms: Vec<String>,
}

/// Return the static catalogue of built-in shells filtered to the host
/// platform — used by the settings UI and the new-terminal dropdown.
#[platform::handler(program = "platform")]
fn list_builtin_shells(_state: &AppState) -> Result<Vec<BuiltinShellInfo>, AppError> {
    Ok(BUILTIN_SHELLS
        .iter()
        .filter(|s| terminal::registry::shell_supports_host(s.platforms))
        .map(|s| BuiltinShellInfo {
            id:        s.id.to_string(),
            name:      s.name.to_string(),
            cmd:       s.cmd.to_string(),
            platforms: s.platforms.iter().map(|p| (*p).to_string()).collect(),
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Terminals config get/set
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn get_terminals_config(
    state: &AppState,
) -> Result<crate::config::app_config::TerminalsConfig, AppError> {
    let cfg = state.lock_config()?;
    Ok(cfg.terminals.clone())
}

#[platform::handler(program = "platform")]
fn set_terminals_config(
    state:  &AppState,
    config: crate::config::app_config::TerminalsConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.terminals = config;
    let snapshot = cfg.clone();
    drop(cfg);
    crate::config::app_config::save(&snapshot).map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// Shell detection
// ---------------------------------------------------------------------------

/// Kick off shell detection as a non-cancellable background job — mirrors
/// `start_ide_detection`.  The detected shell list arrives via the streaming
/// seam: `arbor://shell-detection-done` carries `{ shells: [...] }` under the
/// `{ stream_id, seq }` envelope (`stream_id == job_id`).
///
/// Returns the job-id immediately; the detection runs in a background thread
/// which emits `arbor://job-output`, `arbor://job-done` and the
/// `arbor://shell-detection-*` lifecycle through the captured event sink.
#[platform::handler(program = "platform")]
fn start_shell_detection(state: &AppState) -> Result<String, AppError> {
    use crate::jobs::{JobInfo, JobRegistry, JobStatus};

    let sink: Arc<dyn EventSink> = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    let jobs = Arc::clone(&state.jobs);

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

    sink.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        "Shell Detection",
        "plugin_name": "arbor",
        "command":     "detect shells",
        "category":    "System",
    }));

    // `stream_id == job_id`: one identity for the Jobs entry and the stream.
    let stream = Stream::new(Arc::clone(&sink), "arbor://shell-detection", job_id.clone());
    stream.started(serde_json::json!({}));

    let jid       = job_id.clone();
    let sink_bg   = Arc::clone(&sink);
    let jobs_bg   = Arc::clone(&jobs);
    let stream_bg = stream.clone();
    let _thread = std::thread::Builder::new()
        .name("arbor-shell-detection".into())
        .spawn(move || {
            let results: Vec<DetectedShell> =
                terminal::detect_available_shells(&path_overrides);

            for r in &results {
                let line = if r.available {
                    format!("✓  {} — {}", r.name, r.detected_path.as_deref().unwrap_or(""))
                } else {
                    format!("✗  {} — not found", r.name)
                };
                if let Ok(mut jobs) = jobs_bg.lock() {
                    jobs.append_output(&jid, line.clone());
                };
                sink_bg.emit("arbor://job-output", serde_json::json!({
                    "job_id": &jid,
                    "text":   line,
                }));
            }

            if let Ok(mut jobs) = jobs_bg.lock() {
                jobs.set_status(&jid, JobStatus::Completed { exit_code: 0 });
            };

            sink_bg.emit("arbor://job-done", serde_json::json!({
                "job_id":    &jid,
                "success":   true,
                "exit_code": 0,
            }));
            // The detected shell list rides the standardized terminal `-done`
            // event under `{ shells }` + the `{ stream_id, seq }` envelope.
            stream_bg.done(serde_json::json!({ "shells": &results }));
        });

    Ok(job_id)
}
