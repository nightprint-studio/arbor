//! `arbor-mcp` — Arbor's Model Context Protocol server.
//!
//! MCP is how an external AI client (Claude Code, Claude Desktop, anything else that
//! speaks it) discovers what a program can do and asks it to do one of those things.
//! This crate is the protocol half: JSON-RPC 2.0 over Streamable HTTP on loopback,
//! `initialize` / `tools/list` / `tools/call`, and the origin + token checks that keep
//! the endpoint to this machine.
//!
//! It knows nothing about Arbor. The host implements [`ToolCatalog`] and owns every
//! decision that matters — which tools are visible, whether this one may run right now,
//! how big an answer is allowed to be. That split is what keeps consent policy in the
//! launcher, where a user can see it, instead of scattered through a protocol handler.
//!
//! ```ignore
//! let http = arbor_http::Server::bind("127.0.0.1:8787".parse()?).await?;
//! let server = Arc::new(McpServer::new(
//!     Arc::new(my_catalog),
//!     ServerInfo { name: "arbor".into(), version: "0.3.0".into(), instructions: None },
//!     Guards::with_token(token),
//! ));
//! server.serve(http, shutdown_signal).await;
//! ```
//!
//! ## Public API: use the [`prelude`]

pub mod catalog;
pub mod jsonrpc;
pub mod prelude;
pub mod progress;
pub mod resource;
pub mod server;
pub mod tool;

pub use catalog::ToolCatalog;
pub use progress::Progress;
pub use resource::{Resource, ResourceCatalog, ResourceContents};
pub use server::{ClientRecord, Guards, McpServer, ServerInfo, LATEST_PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS};
pub use tool::{CallToolResult, Content, Tool, ToolAnnotations};
