//! Error type for the plugin API surface.
//!
//! Mapped 1:1 onto runtime-specific errors by the adapters (`mlua::Error` for
//! the Lua runtime, an equivalent for wasm). Domain crates that contribute
//! plugin functions return this enum; the runtime translates it into whatever
//! the script side expects.

use std::fmt::Display;

/// Errors a plugin-facing function can raise.
///
/// `PermissionDenied` is emitted by the registry's invocation gate before the
/// function body runs. The remaining variants are produced by the function
/// body itself or by the bridging helpers on [`PluginValue`](crate::value::PluginValue).
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// The plugin's manifest does not grant the permission the function
    /// requires. The first string is the permission key (`"gitprovider"`),
    /// the second is the required value (`"write"`, `"read"`, …).
    #[error("permission denied: '{0}' requires '{1}'")]
    PermissionDenied(String, String),

    /// The arguments table did not have the expected shape (missing key,
    /// wrong type, malformed list, …).
    #[error("bad args: {0}")]
    BadArgs(String),

    /// The requested target (tab, repo, provider, plugin, …) was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// A domain-level failure (HTTP error from a git provider, libgit2
    /// failure, scheduler conflict, …). The string carries the user-readable
    /// message; the runtime adapter is free to attach a richer source on the
    /// way out.
    #[error("domain: {0}")]
    Domain(String),

    /// Catch-all for everything that doesn't fit a more specific variant.
    #[error("{0}")]
    Other(String),
}

impl PluginError {
    /// Construct a [`PluginError::BadArgs`] from anything string-like.
    pub fn bad_args(msg: impl Into<String>) -> Self {
        Self::BadArgs(msg.into())
    }

    /// Construct a [`PluginError::NotFound`] from anything string-like.
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// Construct a [`PluginError::Domain`] from any `Display` source (e.g.
    /// `reqwest::Error`, `git2::Error`). The source's `Display` impl is
    /// captured eagerly so the original error type does not need to flow
    /// through the plugin API boundary.
    pub fn domain<E: Display>(e: E) -> Self {
        Self::Domain(e.to_string())
    }

    /// Construct a [`PluginError::Other`] from anything string-like.
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}
