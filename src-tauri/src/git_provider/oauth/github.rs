//! GitHub OAuth helpers exposed to the connector layer.
//!
//! The Device Authorization Grant emits its completion event through an
//! [`EventSink`] (the Model-D event egress), so it runs from the `&AppState`
//! provider handler without a `tauri::AppHandle`.

use arbor_ipc::prelude::EventSink;

use crate::auth::DeviceFlowInfo;
use crate::git_provider::types::error::ProviderError;

/// Kicks off the Device Authorization Grant via the existing implementation.
/// Returns the verification info (user code + URL) the UI should display.
pub async fn start(sink: std::sync::Arc<dyn EventSink>) -> Result<DeviceFlowInfo, ProviderError> {
    crate::git_provider::oauth::github_flow::start_github_device_flow(sink)
        .await
        .map_err(|e| ProviderError::Internal(e.to_string()))
}
