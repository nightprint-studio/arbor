//! Repo-registry reads — the open-tab registry (`tab_id` → path) headless handlers
//! query to resolve a tab without the shell's `RepoManager`. The tab is registered
//! by `open_repo` / `init_repo` directly (`CorvusState::register_repo`); these
//! read-only `__`-prefixed methods are the shell's window into that set. Internal
//! plumbing, hence the `__` prefix.

use corvus_core::prelude::CorvusState;

/// Resolve a `tab_id` to its registered repo path (the read side of the open-tab
/// registry). The shell's own consumers (studio file-tools,
/// open-in-browser, workspace check) call this so the launcher no longer keeps a
/// `RepoManager`/git2 repo cache of its own — corvus-be is the sole owner of the
/// open-tab → path registry. `None` when the tab isn't registered.
#[arbor_rpc::handler]
fn __repo_tab_path(state: &CorvusState, tab_id: String) -> Result<Option<String>, String> {
    Ok(state.repo_path(&tab_id))
}

/// Every open tab as `(tab_id, path)`. The shell derives both its "is this path
/// open?" checks and the plugin-host repo context (name = path basename) from
/// this, now that it holds no repo registry of its own.
#[arbor_rpc::handler]
fn __repo_open_tabs(state: &CorvusState) -> Result<Vec<(String, String)>, String> {
    Ok(state.open_tabs())
}

/// Push an app-config slice (keyed by `section`), so the OOP handlers read the
/// user-tuned config instead of falling back to a built-in default. The shell
/// sends `"recovery"` (the snapshot policy) on repo open and on config change;
/// later config-dependent domains ride the same method with their own section.
///
/// The `"git"` section is special: on arrival corvus-be re-runs detection with the
/// shell-pushed `executable_path` override. The PortableGit dir is a fixed global
/// path (`~/.config/arbor/git`) corvus-be resolves itself, so it isn't pushed.
#[arbor_rpc::handler]
fn __set_config(state: &CorvusState, section: String, value: serde_json::Value) -> Result<(), String> {
    state.set_config(&section, value);
    if section == "git" {
        if let Some(cfg) = state.config("git") {
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
