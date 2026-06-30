//! `deep_link` domain — the `[deep_link]` config slice, routed through the
//! platform backend.
//!
//! The URL→repo lookup (`find_repo_by_remote_url`) moved to corvus-be
//! (`crates/products/corvus/be/src/deep_link.rs`): it reads corvus's registry + workspaces,
//! which the launcher no longer mirrors for deep-links. The config below stays
//! here — `[deep_link]` is an `AppConfig` slice the launcher owns. The
//! `AppHandle`-coupled delivery handlers (`deep_link_ready`, `dispatch_deep_link`)
//! stay in the command module: they flush the cold-start buffer, manage windows,
//! and emit `arbor://…` events.

use crate::config::app_config::{self, AppConfig};
use crate::deep_link::DeepLinkConfig;
use crate::error::AppError;
use crate::ipc::platform;
use crate::AppState;

#[platform::handler(program = "platform")]
fn get_deep_link_config(state: &AppState) -> Result<DeepLinkConfig, AppError> {
    Ok(state.lock_config()?.deep_link.clone())
}

#[platform::handler(program = "platform")]
fn set_deep_link_config(state: &AppState, config: DeepLinkConfig) -> Result<(), AppError> {
    let snapshot: AppConfig = {
        let mut c = state.lock_config()?;
        c.deep_link = config;
        c.clone()
    };
    app_config::save(&snapshot)?;
    Ok(())
}
