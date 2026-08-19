//! The seam between the protocol and whatever is actually behind it.
//!
//! This crate answers `tools/list` and `tools/call`; it has no idea what a tool is or
//! does. A host implements [`ToolCatalog`] and owns every product decision: which tools
//! exist, whether this caller may run this one right now, what the result should look
//! like, and how big it is allowed to be.
//!
//! `call` returns a [`CallToolResult`] rather than a `Result` on purpose. Everything a
//! catalogue can hit — an unknown tool, a denied consent prompt, a backend that isn't
//! running — is something the *model* should read and react to, not a transport fault.
//! Reserve the JSON-RPC error channel for the protocol actually breaking.

use async_trait::async_trait;
use serde_json::Value;

use crate::progress::Progress;
use crate::tool::{CallToolResult, Tool};

/// What the MCP server exposes.
#[async_trait]
pub trait ToolCatalog: Send + Sync + 'static {
    /// The currently visible tools. Called on every `tools/list`, so a host that has to
    /// go and ask a backend should cache — but the result may legitimately change
    /// between calls (a product switched off, a backend that came up).
    async fn list(&self) -> Vec<Tool>;

    /// Run one tool. `arguments` is whatever the client sent, unvalidated beyond being
    /// a JSON value: schema conformance is advisory on the wire, so the host validates
    /// what it cares about.
    ///
    /// `progress` is where a long tool narrates itself. It is always present and usually
    /// inert — a host reports its steps the same way regardless, and the transport decides
    /// whether the client asked to hear them. See [`Progress`].
    async fn call(&self, name: &str, arguments: Value, progress: &Progress) -> CallToolResult;
}
