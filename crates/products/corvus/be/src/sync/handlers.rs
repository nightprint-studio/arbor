//! RPC handlers for the sync feature. Enabling resolves/creates the private repo
//! (async, provider round-trip) and does a first push; status/push/preview are
//! the operational surface the FE drives. The per-item **pull apply** is a
//! follow-up (it pairs with the merge UI); `sync_pull_preview` is the read-only
//! half already available.

use corvus_core::prelude::CorvusState;

use crate::corvus_config;

use super::{engine, remote, sources, SyncStatus};

/// Build the status snapshot (config + a best-effort live dirty flag).
fn status_of(state: &CorvusState) -> SyncStatus {
    let cfg = corvus_config::load(state).sync;
    let dirty = if cfg.enabled {
        sources::build(state, &cfg)
            .map(|files| super::is_dirty(sources::fingerprint(&files)))
            .unwrap_or(false)
    } else {
        false
    };
    SyncStatus {
        enabled: cfg.enabled,
        provider: cfg.provider,
        repo_full_name: cfg.repo_full_name,
        clone_url: cfg.clone_url,
        interval_secs: cfg.interval_secs,
        include_workspaces: cfg.include_workspaces,
        include_settings: cfg.include_settings,
        include_mods: cfg.include_mods,
        include_plugin_data: cfg.include_plugin_data,
        last_push_at: cfg.last_push_at,
        last_pull_at: cfg.last_pull_at,
        last_machine: cfg.last_machine,
        dirty,
        awaiting_pull: cfg.awaiting_pull,
    }
}

#[arbor_rpc::handler]
fn sync_status(state: &CorvusState) -> Result<SyncStatus, String> {
    Ok(status_of(state))
}

/// Enable sync: resolve or create the private repo, persist the config, and do a
/// first push. `provider` is mandatory (`"github"`); `repo_name` optional (blank
/// → the default, adopting an existing one across machines).
#[arbor_rpc::handler]
async fn sync_enable(
    state: &CorvusState,
    provider: String,
    repo_name: Option<String>,
) -> Result<SyncStatus, String> {
    if provider != "github" {
        return Err("Settings sync currently supports GitHub only.".to_string());
    }
    let (target, created) = remote::resolve_or_create(&provider, repo_name.as_deref()).await?;
    corvus_config::update_sync(state, |s| {
        s.enabled = true;
        s.provider = Some(provider.clone());
        s.repo_name = repo_name.clone().filter(|n| !n.trim().is_empty());
        s.repo_full_name = Some(target.full_name.clone());
        s.clone_url = Some(target.clone_url.clone());
        // Adopting an existing repo → don't push (it holds another machine's
        // data); the user pulls first. A brand-new repo is safe to seed.
        s.awaiting_pull = !created;
    })?;
    if created {
        push_now(state).await?;
    }
    Ok(status_of(state))
}

#[arbor_rpc::handler]
fn sync_disable(state: &CorvusState) -> Result<SyncStatus, String> {
    corvus_config::update_sync(state, |s| s.enabled = false)?;
    Ok(status_of(state))
}

#[arbor_rpc::handler]
async fn sync_push_now(state: &CorvusState) -> Result<SyncStatus, String> {
    push_now(state).await?;
    Ok(status_of(state))
}

/// Diff the remote bundle against local state into a per-item plan the merge UI
/// renders (read-only).
#[arbor_rpc::handler]
async fn sync_pull_preview(state: &CorvusState) -> Result<super::pull::PullPlan, String> {
    super::pull::preview(state).await
}

/// Apply the user's per-item selections from the remote bundle (workspaces with
/// repo-id remap, settings, mod enable-states, plugin data).
#[arbor_rpc::handler]
async fn sync_pull_apply(
    state: &CorvusState,
    selections: super::pull::PullSelections,
) -> Result<super::pull::PullSummary, String> {
    super::pull::apply(state, selections).await
}

/// Build → push the bundle now, updating the fingerprint baseline + status.
async fn push_now(state: &CorvusState) -> Result<(), String> {
    let cfg = corvus_config::load(state).sync;
    let target = remote::from_config(&cfg).ok_or_else(|| "Sync is not configured.".to_string())?;
    let files = sources::build(state, &cfg)?;
    let fp = sources::fingerprint(&files);
    engine::push(&target, &files).await?;
    super::record_pushed(fp);
    corvus_config::update_sync(state, |s| {
        s.last_push_at = Some(super::now_epoch());
        s.last_machine = Some(super::machine_id());
        // An explicit push is the user choosing local → remote; it settles the
        // adopt-first-pull state.
        s.awaiting_pull = false;
    })?;
    state.emit(
        "arbor://corvus-sync-pushed",
        serde_json::json!({ "at": super::now_epoch() }),
    );
    Ok(())
}
