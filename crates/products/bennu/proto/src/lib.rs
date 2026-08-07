//! `bennu-proto` — the Bennu IPC contract.
//!
//! The Phase-0 request/response types the `bennu-be` handlers return and the
//! Bennu frontend deserializes. Pure serde, **Tauri-free by construction**: it
//! depends only on `serde` + `serde_json`, so both the backend (`bennu-be`) and
//! any future in-process caller can share one set of shapes without pulling in the
//! backend runtime.
//!
//! These types are the wire contract for the six Phase-0 methods
//! (`bennu_open_project`, `bennu_project_tree`, `bennu_read_file`,
//! `bennu_capabilities`, `bennu_completion`, `bennu_diagnostics`). The `capabilities`
//! bitset and the capability-detection logic that produces it live in
//! `bennu-project`; this crate only carries the serialized [`CapabilitySet`] the FE
//! sees.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: every Arbor library crate exposes its public surface
//! through a `prelude` module. Call sites reach these types through
//! `bennu_proto::prelude::...`. The submodules stay `pub` for rustdoc navigation,
//! but the prelude is the canonical call-site path.

pub mod contract;
/// The language-server side of the contract — its own module because `contract.rs` is
/// already long and everything in there answers an LSP-backed handler.
pub mod lsp;
pub mod prelude;
