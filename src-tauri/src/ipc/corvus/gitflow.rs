//! `gitflow` domain — the **global** config-CRUD slice, still served in-process.
//!
//! The 9 operational Git Flow handlers AND the per-repo config-CRUD (which owns
//! `<repo>/.arbor/config.toml`) moved to `corvus-be`. What remains here are the 2
//! GLOBAL config reads/writes (`get`/`set_gitflow_global_config`) that own the
//! `AppConfig.gitflow` value — they stay shell-side because the shell owns
//! AppConfig. The pure work lives in [`crate::git::gitflow`].

use crate::config::app_config;
use crate::error::AppError;
use crate::git::gitflow::GitFlowConfig;
use crate::ipc::corvus;
use crate::AppState;

#[corvus::handler]
fn get_gitflow_global_config(state: &AppState) -> Result<GitFlowConfig, AppError> {
    let cfg = state.lock_config()?;
    Ok(cfg.gitflow.clone())
}

#[corvus::handler]
fn set_gitflow_global_config(state: &AppState, config: GitFlowConfig) -> Result<(), AppError> {
    {
        let mut cfg = state.lock_config()?;
        cfg.gitflow = config;
        app_config::save(&cfg)?;
    }
    // Push the new global config to corvus-be so its OOP gitflow handlers (and
    // the per-repo `get_gitflow_config` merge) see the live value (lock released
    // first — sync_config re-reads from disk).
    crate::ipc::sync_config(state);
    Ok(())
}
