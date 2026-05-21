//! Plugin-to-plugin dependency declarations + recorded load failures.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// Name of the required plugin (must match the `name` field in its plugin.toml).
    pub name: String,
    /// Semver version requirement, e.g. ">=1.0.0". Empty string accepts any version.
    #[serde(default)]
    pub version: String,
    /// If true, a missing or incompatible dependency is a warning rather than an error.
    #[serde(default)]
    pub optional: bool,
}

/// Entry kept for plugins that failed to satisfy their dependency graph at
/// load time. Surfaced in the Plugin Manager so the user can see why a plugin
/// is unavailable.
#[derive(Debug, Clone)]
pub struct PluginLoadFailure {
    pub name:        String,
    pub version:     String,
    pub description: String,
    pub author:      String,
    pub error:       String,
}
