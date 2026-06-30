//! `NamespaceContributor` — the trait every domain crate implements to pour
//! its plugin-facing surface into the central [`crate::registry::PluginRegistry`].
//!
//! The host's composition root (today `setup_plugin_system` in `src-tauri`,
//! after PR #4 in `arbor-plugin-core`) instantiates one contributor per crate
//! and calls `contribute(&mut reg)` on each. The contributors are
//! independent — `arbor-git-provider-api` doesn't see (and doesn't need)
//! `arbor-issue-tracker-api`.

use crate::registry::PluginRegistry;

/// A domain crate's bundled contribution: namespaces, hooks, permissions.
///
/// Sync on purpose — building the registry happens once at boot. Anything that
/// would need to be awaited (warming caches, opening connections) lives behind
/// the registered functions themselves, not in `contribute`.
pub trait NamespaceContributor {
    fn contribute(&self, reg: &mut PluginRegistry);
}
