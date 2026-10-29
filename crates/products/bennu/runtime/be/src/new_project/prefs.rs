//! What the wizard remembers between runs — last location, last generator, the Git toggle.
//!
//! Its own pair of handlers (like the welcome tour's) rather than a field of the big config the
//! settings dialog round-trips: finishing a wizard is one small write, and routing it through
//! `set_bennu_config` would make it serialise every setting the user has, including any a settings
//! page open at the same moment is in the middle of editing.

use bennu_core::config::{load, save, NewProjectConfig};
use bennu_core::prelude::BennuState;

/// Read the wizard's remembered choices (defaults on a missing or corrupt file).
#[arbor_rpc::handler]
fn get_bennu_new_project(_state: &BennuState) -> Result<NewProjectConfig, String> {
    Ok(load().new_project)
}

/// Remember the wizard's choices.
#[arbor_rpc::handler]
fn set_bennu_new_project(_state: &BennuState, config: NewProjectConfig) -> Result<(), String> {
    let mut current = load();
    current.new_project = config;
    save(&current)
}
