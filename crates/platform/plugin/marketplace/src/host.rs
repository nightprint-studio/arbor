//! Host capabilities the marketplace needs from the embedding shell.
//!
//! The registry's local-merge step (surfacing dev / hand-copied plugins
//! alongside community + custom entries) and the installer's collision
//! guard both need to reflect on what's already on disk. Rather than pull
//! a hard dep on `arbor-plugin-core` into this crate (which would break
//! the layering — see `docs/crate-refactor.md`), the marketplace asks for
//! these capabilities through a small trait the shell implements once.
//!
//! Methods are intentionally minimal: each one is a "the host has this
//! and the marketplace needs it" capability. New methods are added only
//! when the marketplace actually needs one, never speculatively.

use std::collections::HashMap;
use std::path::PathBuf;

use arbor_plugin_types::prelude::Manifest;

pub trait MarketplaceHost: Send + Sync + 'static {
    /// Manifests of dev / hand-copied plugins discovered on disk. Used by
    /// [`crate::registry::MarketplaceRegistry::catalog`] to surface a
    /// `Local` row for anything the user has placed in the plugin folder
    /// that isn't tracked by the marketplace install ledger.
    fn discover_plugins(&self) -> Vec<Manifest>;

    /// Per-plugin enable flags as the host knows them. The marketplace
    /// uses this to paint the `enabled` field on Local rows so the modal
    /// matches what the Plugin Manager would show.
    fn plugin_states(&self) -> HashMap<String, bool>;

    /// Directory the host scans for dev plugins. The installer refuses to
    /// overwrite a non-empty folder inside it — a marketplace install
    /// would be shadowed by the dev copy at load time anyway, so the
    /// collision is surfaced as an error rather than silently lost.
    fn dev_plugin_dir(&self) -> PathBuf;

    /// Forget every credential a plugin owned, as part of uninstalling it.
    ///
    /// Removing the install directory does not remove secrets — they live in the OS
    /// keychain, and a plugin the user deleted has no business leaving a token behind that
    /// nothing on disk explains any more. The host is asked rather than told how, because
    /// the marketplace crate has no business knowing what a keychain is.
    ///
    /// Best-effort by design: a keychain that refuses is not a reason to leave the plugin
    /// half-installed, so the implementation logs and the uninstall carries on. Default is a
    /// no-op for hosts with no credential store at all.
    fn forget_plugin_credentials(&self, _plugin: &str) {}
}
