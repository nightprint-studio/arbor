//! IPC surface for the AI tool endpoint: read/write its settings, see its state, answer
//! a consent prompt, read the call log.
//!
//! Keep-shell commands, all of them: they start and stop a listener, reach an
//! `AppHandle`, and talk to in-process state that has no business crossing to a backend.

use tauri::{AppHandle, State};

use crate::config::app_config::{self, McpConfig};
use crate::error::AppError;
use crate::mcp;
use crate::AppState;

/// Current MCP settings.
#[tauri::command]
pub fn get_mcp_config(state: State<'_, AppState>) -> Result<McpConfig, AppError> {
    Ok(state.lock_config()?.mcp.clone())
}

/// Persist MCP settings and bring the endpoint in line with them immediately —
/// enabling, disabling, moving port, or just changing which products are exposed.
///
/// Reconciling here rather than at the next restart is the point: a user who has just
/// switched something off expects it to be off.
#[tauri::command]
pub fn set_mcp_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: McpConfig,
) -> Result<mcp::McpStatus, AppError> {
    {
        let mut cfg = state.lock_config()?;
        cfg.mcp = config;
        let snapshot = cfg.clone();
        drop(cfg);
        app_config::save(&snapshot).map_err(|e| AppError::Other(e.to_string()))?;
    }
    mcp::reconcile(&app);
    Ok(mcp::status())
}

/// Whether the endpoint is up, on which port, and with which token — the three things
/// the `claude mcp add …` line needs.
#[tauri::command]
pub fn get_mcp_status() -> mcp::McpStatus {
    mcp::status()
}

/// Answer a pending consent prompt.
///
/// `remember` grants the tool for the rest of this run only. It is not written to the
/// config: a convenience for a working session should not quietly become a setting.
#[tauri::command]
pub fn mcp_consent_respond(id: String, tool: String, allow: bool, remember: bool) -> bool {
    mcp::consent::respond(&id, allow, remember, &tool)
}

/// Mint a new bearer token, invalidating every client already configured with the old
/// one. The endpoint restarts on the new credential.
#[tauri::command]
pub fn mcp_regenerate_token(app: AppHandle) -> mcp::McpStatus {
    mcp::regenerate_token(&app);
    mcp::status()
}

/// Drop every "allow for this session" grant.
#[tauri::command]
pub fn mcp_revoke_session_grants() {
    mcp::consent::clear_session_grants();
}

/// Every tool Arbor can expose, program by program — the reference behind the AI tools
/// modal.
///
/// Lists a program's tools whether or not it is currently exposed, marking which is which:
/// deciding to switch a product on is a decision about what its tools would let an
/// assistant do, and a list you can only see afterwards does not help make it. Reading an
/// inventory means starting that backend, which is the same act a client's `tools/list`
/// performs and is idempotent.
#[tauri::command]
pub fn get_mcp_tools(app: AppHandle) -> Vec<mcp::catalog::ProgramTools> {
    mcp::catalog::listing(&app)
}

/// Who has connected to the endpoint this run, and whether anything is listening now.
///
/// Introductions, not presence: the transport issues no session ids, so only `initialize`
/// identifies anyone and a client that went away leaves nothing behind to notice. The one
/// live figure is the count of open notification streams.
#[tauri::command]
pub fn get_mcp_clients() -> mcp::McpClients {
    mcp::clients()
}

/// The call log, newest first, plus the id of the run asking — so a reader can tell this
/// session's calls from the ones it inherited from earlier runs.
#[tauri::command]
pub fn get_mcp_audit() -> mcp::audit::ActivityLog {
    mcp::audit::entries()
}

/// Forget the call log. Offered because it holds the paths of everything an assistant
/// has looked at.
#[tauri::command]
pub fn clear_mcp_audit() {
    mcp::audit::clear();
}
