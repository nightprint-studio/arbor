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

#[arbor_rpc::handler]
fn __set_git_program(state: &CorvusState, program: Option<String>) -> Result<(), String> {
    state.set_git_program(program);
    Ok(())
}

/// Push an app-config slice (keyed by `section`), so the OOP handlers read the
/// user-tuned config instead of falling back to a built-in default. The shell
/// sends `"recovery"` (the snapshot policy) on repo open and on config change;
/// later config-dependent domains ride the same method with their own section.
#[arbor_rpc::handler]
fn __set_config(state: &CorvusState, section: String, value: serde_json::Value) -> Result<(), String> {
    state.set_config(&section, value);
    Ok(())
}
