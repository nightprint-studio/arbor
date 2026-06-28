//! Repo-registry sync — the shell pushes the open tabs' paths (and the resolved
//! git program) here so headless handlers can resolve a `tab_id` without the
//! shell's `RepoManager`. These methods are advertised in `Hello`; the shell
//! calls them on repo open / close. Internal plumbing, hence the `__` prefix.

use corvus_core::prelude::CorvusState;

#[arbor_rpc::handler]
fn __repo_register(state: &CorvusState, tab_id: String, path: String) -> Result<(), String> {
    state.register_repo(tab_id, path);
    Ok(())
}

#[arbor_rpc::handler]
fn __repo_deregister(state: &CorvusState, tab_id: String) -> Result<(), String> {
    state.deregister_repo(&tab_id);
    Ok(())
}

/// Push an app-config slice (keyed by `section`), so the OOP handlers read the
/// user-tuned config instead of falling back to a built-in default. The shell
/// sends `"recovery"` (the snapshot policy) on repo open and on config change;
/// later config-dependent domains ride the same method with their own section.
///
/// The `"git"` section is special: corvus-be self-detects its system git, so on
/// arrival it applies the shell-pushed absolute `portable_dir` (the shell owns
/// the active profile; corvus-be can't recompute it) and re-runs detection with
/// the configured `executable_path` override.
#[arbor_rpc::handler]
fn __set_config(state: &CorvusState, section: String, value: serde_json::Value) -> Result<(), String> {
    state.set_config(&section, value);
    if section == "git" {
        if let Some(cfg) = state.config("git") {
            if let Some(dir) = cfg.get("portable_dir").and_then(|v| v.as_str()) {
                corvus_git_cli::set_portable_dir_override(std::path::PathBuf::from(dir));
            }
            let configured = cfg
                .get("executable_path")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(std::path::PathBuf::from);
            corvus_git_cli::detect(configured.as_deref());
        }
    }
    Ok(())
}
