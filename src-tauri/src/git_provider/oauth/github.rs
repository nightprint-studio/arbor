//! GitHub OAuth helpers exposed to the command layer.
//!
//! The Device Authorization Grant needs a `tauri::AppHandle` to emit the
//! completion event, so the trait method on `GithubProvider` can't actually
//! drive it.  These helpers are wired up by `commands::auth_commands`; the
//! trait method itself returns `Unsupported` until the flow is reachable
//! without an `AppHandle`.

use crate::auth::DeviceFlowInfo;
use crate::git_provider::types::error::ProviderError;

pub fn revoke_token() -> Result<(), ProviderError> {
    crate::git_provider::oauth::github_flow::disconnect()
        .map_err(|e| ProviderError::Internal(e.to_string()))
}

/// Kicks off the Device Authorization Grant via the existing implementation.
/// Returns the verification info (user code + URL) the UI should display.
pub async fn start(app: tauri::AppHandle) -> Result<DeviceFlowInfo, ProviderError> {
    crate::git_provider::oauth::github_flow::start_github_device_flow(app)
        .await
        .map_err(|e| ProviderError::Internal(e.to_string()))
}
