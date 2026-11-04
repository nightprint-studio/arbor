//! Environments and their types, and this machine's local values (including which environment is active).

use hirundo_core::prelude::HirundoState;
use hirundo_model::prelude::{EnvType, Environment, Local};
use serde::Serialize;

use crate::context::store_for;

#[derive(Debug, Serialize)]
pub struct EnvState {
    /// The types: the project's own, or the four Hirundo starts with.
    pub types: Vec<EnvType>,
    pub environments: Vec<Environment>,
    pub local: Local,
}

#[arbor_rpc::handler]
fn env_state(_state: &HirundoState, project: String) -> Result<EnvState, String> {
    let store = store_for(&project)?;
    Ok(EnvState { types: store.read_types()?, environments: store.list_environments()?, local: store.read_local() })
}

/// Replace the environment types. A type an environment still uses may be removed: that environment simply
/// has no type, no colour and no protection afterwards.
#[arbor_rpc::handler]
fn env_types_save(_state: &HirundoState, project: String, types: Vec<EnvType>) -> Result<(), String> {
    store_for(&project)?.write_types(&types)
}

#[arbor_rpc::handler]
fn environment_save(_state: &HirundoState, project: String, environment: Environment) -> Result<(), String> {
    store_for(&project)?.write_environment(&environment)
}

#[arbor_rpc::handler]
fn environment_delete(_state: &HirundoState, project: String, id: String) -> Result<(), String> {
    let store = store_for(&project)?;
    store.delete_environment(&id)?;
    // If it was the active one, none is: a request must not silently go to an environment that is gone.
    let mut local = store.read_local();
    if local.active_env == id {
        local.active_env.clear();
        store.write_local(&local)?;
    }
    Ok(())
}

/// Save the local values: the active environment and this machine's own variables. Never committed.
#[arbor_rpc::handler]
fn local_save(_state: &HirundoState, project: String, local: Local) -> Result<(), String> {
    store_for(&project)?.write_local(&local)
}

/// The keyring account of a secret variable, for the shell commands that store its value
/// (`hirundo_store_secret`). The window never builds this text itself: project, layer and name are
/// this backend's to combine, so there is one definition of what a secret is called.
#[arbor_rpc::handler]
fn secret_account(_state: &HirundoState, project: String, scope: hirundo_model::prelude::VarScope, id: String, name: String) -> Result<String, String> {
    use hirundo_model::prelude::VarSource;
    if name.trim().is_empty() {
        return Err("the variable needs a name".into());
    }
    Ok(hirundo_core::prelude::secret_account(&project, &VarSource::new(scope, &id), name.trim()))
}
