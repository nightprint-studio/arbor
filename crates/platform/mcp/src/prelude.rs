//! Canonical entry point for `arbor-mcp`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_mcp::prelude::...`.

pub use crate::catalog::ToolCatalog;
pub use crate::progress::Progress;
pub use crate::resource::{Resource, ResourceCatalog, ResourceContents};
pub use crate::jsonrpc::{codes, Message, Response};
pub use crate::server::{
    ClientRecord, Guards, McpServer, ServerInfo, LATEST_PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS,
};
pub use crate::tool::{CallToolResult, Content, Tool, ToolAnnotations};
