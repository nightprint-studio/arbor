//! Canonical entry point for `garrulus-core`'s public API.
//!
//! Workspace convention: call sites (in `garrulus-be`) reach this crate's surface
//! through `garrulus_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub use crate::state::GarrulusState;

// The module, not its contents: a handler reads `hooks::NOTE_SAVED`, which says
// what the string is, where a glob would drop a bare `NOTE_SAVED` / `SYNC_DONE`
// into every handler's scope (and this prelude is glob-imported).
pub use crate::hooks;

// `config_path` is re-exported under a qualified name: `garrulus-vault`'s prelude
// (glob'd below) owns a `config_path(root)` that answers a different question —
// the vault's own settings file rather than this profile's product config.
pub use crate::config::{
    config_path as garrulus_config_path, load as load_config, save as save_config, GarrulusConfig,
    GarrulusEditorConfig, PRODUCT_GARRULUS,
};

pub use crate::remote::{
    build_remote, credential_provider, RemoteConfig, DEFAULT_GIT_REMOTE,
};

pub use crate::vaults::{
    load_vaults, remember_vault, save_vaults, set_vault_remote, vault_cache_dir, vault_id_for,
    vault_remote, vaults_path, VaultEntry, VaultRegistry,
};

// Re-exported so `garrulus-be` reaches the vault / index / sync vocabulary
// through the product's own prelude, exactly as `picus-project` re-exports the
// `picus-types` names a consumer working in project terms cannot avoid naming.
// This is what keeps garrulus-be's dependency list to the five backend crates
// plus `garrulus-core`: the state's `Vault` / `Index` / `dyn SyncRemote` are
// handed back through these accessors, so a handler must be able to name them.
pub use garrulus_index::prelude::*;
pub use garrulus_sync::prelude::*;
pub use garrulus_vault::prelude::*;

// Re-exported because a caller that has to name the git binary — `garrulus-be`'s
// "create the remote repository" flow runs `git remote add` before installing the
// remote — would otherwise need its own `corvus-git` dependency for one type that
// `build_remote` already resolves internally. `garrulus-sync`'s prelude does not
// carry it (the sync crate hides git behind `SyncRemote`), so it is re-exported
// here, where the git-shaped part of the product's surface already lives.
pub use corvus_git::prelude::GitCli;

// Re-exported because `GarrulusState::hooks_handle` hands back an
// `Arc<HookDispatcher>` (and `PluginValue` is needed to fire onto it) — a
// background worker firing hooks reaches both through this prelude without a
// direct `arbor-plugin-api` dependency.
pub use arbor_plugin_api::prelude::{HookDispatcher, PluginValue};
