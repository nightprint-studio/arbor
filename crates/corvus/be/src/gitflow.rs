//! `gitflow` domain — Git Flow lifecycle served **out-of-process** by corvus-be.
//!
//! The 9 *operational* handlers run here (status / init / feature·release·hotfix
//! start·finish). The *config-CRUD* handlers (`get`/`set` global + the per-repo
//! `.arbor/config.toml` writes + `has_gitflow_repo_override`) stay shell-side —
//! the shell owns the config files.
//!
//! Each handler resolves the **effective** config exactly as the in-process copy
//! (`crate::ipc::corvus::gitflow::effective_config`): the global config the shell
//! pushed into the config bag (section `"gitflow"`, via `__set_config`) overlaid
//! by the repo's own `.arbor/config.toml` override when present. corvus-be reads
//! that per-repo file straight from the workdir it already opens — no extra
//! round-trip. Git Flow does not snapshot, so no recovery policy is needed; the
//! PR-finish path just pushes the branch (`git push`, identical to in-process)
//! and returns `CreatePr` for the frontend to open the MR form — no provider
//! call here.
//!
//! The `on_flow_*` hooks fire inline at the co-located plugin host (W0a) after
//! the repo handle is dropped, with payloads identical to the in-process copy.

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{FlowFinishResult, FlowStartResult, GitFlowConfig, GitFlowStatus};
use git2::Repository;
use serde::Deserialize;
use serde_json::json;

use crate::repo::{git, open};

/// Just the Git Flow slice of a repo's `.arbor/config.toml` — serde ignores the
/// rest of the file. Mirrors `RepoConfig.gitflow` shell-side.
#[derive(Deserialize, Default)]
struct RepoGitflow {
    #[serde(default)]
    gitflow: Option<GitFlowConfig>,
}

/// Effective Git Flow config for an open repo: the pushed global config, overlaid
/// by the repo's per-repo `.arbor/config.toml` override when present. Resolution
/// is byte-identical to the in-process `effective_config`.
fn effective_config(state: &CorvusState, repo: &Repository) -> GitFlowConfig {
    let global: GitFlowConfig = state
        .config("gitflow")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    let workdir = repo
        .workdir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let per_repo_file = std::path::Path::new(&workdir).join(".arbor").join("config.toml");
    let repo_override = std::fs::read_to_string(&per_repo_file)
        .ok()
        .and_then(|s| toml::from_str::<RepoGitflow>(&s).ok())
        .and_then(|c| c.gitflow);

    repo_override.unwrap_or(global)
}

// ── Status ───────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn gitflow_get_status(state: &CorvusState, tab_id: String) -> Result<GitFlowStatus, String> {
    let repo = open(state, &tab_id)?;
    let config = effective_config(state, &repo);
    corvus_git::gitflow::get_gitflow_status(&repo, &config).map_err(|e| e.to_string())
}

// ── Init ─────────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn gitflow_init(state: &CorvusState, tab_id: String) -> Result<(), String> {
    {
        let repo = open(state, &tab_id)?;
        let config = effective_config(state, &repo);
        corvus_git::gitflow::gitflow_init(&git(state), &repo, &config).map_err(|e| e.to_string())?;
    }
    state.fire_hook("on_flow_init", json!({ "tab_id": tab_id }));
    Ok(())
}

#[arbor_rpc::handler]
fn gitflow_init_create_main(
    state: &CorvusState,
    tab_id: String,
    from_initial: bool,
) -> Result<(), String> {
    {
        let repo = open(state, &tab_id)?;
        let config = effective_config(state, &repo);
        corvus_git::gitflow::gitflow_init_create_main(&git(state), &repo, &config, from_initial)
            .map_err(|e| e.to_string())?;
    }
    state.fire_hook("on_flow_init", json!({ "tab_id": tab_id }));
    Ok(())
}

// ── Feature ──────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn gitflow_feature_start(
    state: &CorvusState,
    tab_id: String,
    name: String,
) -> Result<FlowStartResult, String> {
    let result = {
        let repo = open(state, &tab_id)?;
        let config = effective_config(state, &repo);
        corvus_git::gitflow::feature_start(&git(state), &repo, &config, &name)
            .map_err(|e| e.to_string())?
    };
    state.fire_hook(
        "on_flow_feature_start",
        json!({ "tab_id": tab_id, "name": name, "base_branch": result.base_branch }),
    );
    Ok(result)
}

#[arbor_rpc::handler]
fn gitflow_feature_finish(
    state: &CorvusState,
    tab_id: String,
    name: String,
    force_pr: bool,
) -> Result<FlowFinishResult, String> {
    let result = {
        let repo = open(state, &tab_id)?;
        let config = effective_config(state, &repo);
        corvus_git::gitflow::feature_finish_or_pr(&git(state), &repo, &config, &name, force_pr)
            .map_err(|e| e.to_string())?
    };
    state.fire_hook("on_flow_feature_finish", json!({ "tab_id": tab_id, "name": name }));
    Ok(result)
}

// ── Release ──────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn gitflow_release_start(
    state: &CorvusState,
    tab_id: String,
    version: String,
) -> Result<FlowStartResult, String> {
    let result = {
        let repo = open(state, &tab_id)?;
        let config = effective_config(state, &repo);
        corvus_git::gitflow::release_start(&git(state), &repo, &config, &version)
            .map_err(|e| e.to_string())?
    };
    state.fire_hook(
        "on_flow_release_start",
        json!({ "tab_id": tab_id, "version": version, "base_branch": result.base_branch }),
    );
    Ok(result)
}

#[arbor_rpc::handler]
fn gitflow_release_finish(
    state: &CorvusState,
    tab_id: String,
    version: String,
    tag_message: String,
    force_pr: bool,
) -> Result<FlowFinishResult, String> {
    let result = {
        let repo = open(state, &tab_id)?;
        let config = effective_config(state, &repo);
        corvus_git::gitflow::release_finish_or_pr(
            &git(state),
            &repo,
            &config,
            &version,
            &tag_message,
            force_pr,
        )
        .map_err(|e| e.to_string())?
    };
    state.fire_hook(
        "on_flow_release_finish",
        json!({ "tab_id": tab_id, "version": version }),
    );
    Ok(result)
}

// ── Hotfix ───────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn gitflow_hotfix_start(
    state: &CorvusState,
    tab_id: String,
    name: String,
) -> Result<FlowStartResult, String> {
    let result = {
        let repo = open(state, &tab_id)?;
        let config = effective_config(state, &repo);
        corvus_git::gitflow::hotfix_start(&git(state), &repo, &config, &name)
            .map_err(|e| e.to_string())?
    };
    state.fire_hook(
        "on_flow_hotfix_start",
        json!({ "tab_id": tab_id, "name": name, "base_branch": result.base_branch }),
    );
    Ok(result)
}

#[arbor_rpc::handler]
fn gitflow_hotfix_finish(
    state: &CorvusState,
    tab_id: String,
    name: String,
    tag_message: String,
    force_pr: bool,
) -> Result<FlowFinishResult, String> {
    let result = {
        let repo = open(state, &tab_id)?;
        let config = effective_config(state, &repo);
        corvus_git::gitflow::hotfix_finish_or_pr(
            &git(state),
            &repo,
            &config,
            &name,
            &tag_message,
            force_pr,
        )
        .map_err(|e| e.to_string())?
    };
    state.fire_hook("on_flow_hotfix_finish", json!({ "tab_id": tab_id, "name": name }));
    Ok(result)
}
