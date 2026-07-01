//! `arbor-studio-api` — the Tauri-free Studio orchestration facade.
//!
//! Consolidates everything that sits *above* the per-format backends:
//!   · [`registry`] — the `StudioRegistry` + the `studio_registry()`
//!     factory that wires all five backends with their `SchemaProvider`
//!     routing ([`schema_adapter`]) and cross-ref index providers
//!     ([`index_provider`]).
//!   · [`scanner`] — the format-agnostic repo walk + cross-ref /
//!     find-usages / broken-ref scans, delegating per-format def/usage
//!     enumeration to each crate's AST / projector.
//!   · [`index`] — the persistent `<repo>/.arbor/studio-index.json`
//!     incremental cross-ref index + aggregators.
//!   · [`config`] — the repo-root `.arbor/studio.toml` (excludes, schema
//!     bindings, external locations, reference-field overrides).
//!   · [`project_refactor`] — project-wide F12/F13 orchestration for the
//!     `DefaultBackend`-riding formats (TOML/YAML) + the special
//!     `.properties` flows.
//!   · [`dispatch`] — registry-level routing entry point.
//!
//! The launcher keeps only the Tauri command/rpc seam (the
//! `#[studio::handler(program = "studio")]` modules), which call into this
//! crate. These submodules are the WASM-guest compile target for
//! Studio-as-plugin.

pub mod config;
pub mod dispatch;
pub mod index;
pub mod index_provider;
pub mod prelude;
pub mod project_refactor;
pub mod refactor_glue;
pub mod registry;
pub mod scanner;
pub mod schema_adapter;

pub use dispatch::dispatch;
pub use registry::{studio_registry, StudioRegistry};

#[cfg(test)]
mod tests;
