use tauri::State;

use crate::AppState;
use crate::error::Result;
use crate::terminal::{self, BUILTIN_SHELLS, TerminalInfo};

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
