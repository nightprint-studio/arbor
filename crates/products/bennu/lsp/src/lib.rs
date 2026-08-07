//! `bennu-lsp` — a generic Language Server Protocol **client**.
//!
//! The implementation behind the slot `bennu-intel` has documented since Phase 0: the FE
//! speaks one code-intel protocol for every language, Java goes to the native index-backed
//! engine, and everything else goes to a language server. Rust is the first tenant
//! (rust-analyzer) but not a special case — it is one entry in [`catalogue`], and a language
//! nobody anticipated needs a `[[lsp.servers]]` block in the user's config rather than a
//! release.
//!
//! ## What lives where
//!
//! | module | responsibility |
//! |---|---|
//! | [`jsonrpc`] | the base protocol: `Content-Length` framing, JSON-RPC message shapes |
//! | [`types`] | the protocol subset Bennu speaks, hand-rolled as serde structs |
//! | [`uri`] | `file:` URI ↔ path, percent-encoding and Windows drives included |
//! | [`line_index`] | `{line, character}` ↔ UTF-8 byte offset, in any position encoding |
//! | [`client`] | one server process: threads, request correlation, server→client traffic |
//! | [`session`] | one *initialized* server: handshake, capability gate, document sync |
//! | [`ops`] | the editor features, as requests |
//! | [`convert`] | protocol answers → [`model`] values |
//! | [`semantic`] | the delta-encoded token stream → coloured byte spans |
//! | [`catalogue`] | which servers exist, what they serve, where their root is |
//! | [`discovery`] | where their executables actually are on this machine |
//! | [`model`] | what a session returns: byte offsets and absolute paths |
//!
//! ## The three things that are easy to get wrong
//!
//! **Coordinates.** LSP counts a `character` within a line, in units the server chooses —
//! UTF-16 by default, which is neither characters nor bytes. Bennu counts bytes from the
//! start of the file. Every position crossing the seam goes through [`line_index`], and a
//! server that says nothing about its encoding means UTF-16, not "whatever the client
//! asked for".
//!
//! **Document state.** The editor owns the buffer; the server keeps a copy. A request whose
//! offsets refer to text the server has not seen is answered confidently and wrongly, which
//! is why every position-based request re-syncs first rather than trusting editor events to
//! have arrived in order.
//!
//! **Liveness.** A language server is a process that can take half a minute to become
//! useful and can die at any point. Nothing here waits indefinitely, every request is
//! bounded, and a dead server releases its waiters instead of letting them time out one by
//! one.
//!
//! ## No new dependencies
//!
//! serde, serde_json, and two workspace-internal crates. The protocol types are written out
//! in [`types`] rather than pulled from a crate that models the whole specification: Bennu
//! drives a bounded subset, the mapping onto Bennu's own wire types has to be written
//! either way, and the shapes that are genuinely tricky (`boolean | Options` capabilities,
//! the four legal goto answers, the two ways a completion carries its edit) are the ones
//! with tests next to them.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate through `bennu_lsp::prelude::...`.

pub mod catalogue;
pub mod client;
pub mod convert;
pub mod discovery;
pub mod jsonrpc;
pub mod line_index;
pub mod model;
/// The editor features as requests — an `impl LspSession` block, no types of its own.
pub mod ops;
pub mod prelude;
pub mod semantic;
// LSP snippet bodies, reduced to plain text plus where its tab stops are.
pub mod snippet;
pub mod session;
pub mod types;
pub mod uri;
