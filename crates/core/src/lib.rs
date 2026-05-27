//! Shared core for Arbor crates.
//!
//! Holds the cross-crate primitives that have no business depending on Tauri:
//!
//! - [`paths`]   — locations on disk Arbor owns (config / data / cache roots).
//! - [`http`]    — `reqwest` client with the Arbor user-agent + default timeout.
//! - [`error`]   — [`CoreError`] for failures inside this crate; mapped to
//!                 the host's `AppError` at the boundary.
//! - [`app_ctx`] — [`AppCtx`] trait, the Tauri-agnostic handle domain crates
//!                 use to emit events / read user-focus state / locate the
//!                 Arbor data root. The Tauri shell crate implements it.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — every Arbor library crate exposes its public
//! surface through a `prelude` module. Consumers either glob-import it
//! once per file (`use arbor_core::prelude::*;`) or fully qualify
//! (`arbor_core::prelude::arbor_config_path("foo")`). The per-feature
//! modules (`paths`, `http`, …) stay `pub` for discoverability and rustdoc
//! navigation, but call sites should go through the prelude so a single
//! glob import is enough.
//!
//! See `docs/crate-refactor.md` for the full split plan.

pub mod app_ctx;
pub mod error;
pub mod http;
pub mod paths;
pub mod prelude;
