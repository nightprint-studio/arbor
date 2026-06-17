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
