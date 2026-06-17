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
//! NOT migrated (stays inline in `terminal_commands`, handled by a later
//! emit/seam pass):
//!   * `terminal_create` — takes an `AppHandle` and spawns a PTY that streams
//!     output via the `arbor://terminal-*` events.
//!   * `start_shell_detection` — takes an `AppHandle` and emits
//!     `arbor://job-*` / `arbor://shell-detection-done`.

use crate::error::AppError;
use crate::ipc::platform;
use crate::terminal::{self, BUILTIN_SHELLS, TerminalInfo, TerminalManager};
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
    Ok(terminal::platform_default().to_string())
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
