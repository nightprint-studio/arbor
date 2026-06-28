//! `arbor-be` — the Model-D backend runtime scaffold shared by every product
//! `*-be` (`corvus-be`, later `merula-be` / `sitta-be`).
//!
//! Each backend's `main` was ~the same prologue: build the framed-stdio writer +
//! event sink + reverse-channel caller + tokio runtime, construct the plugin
//! host, wire the scheduler, run a few pre-serve inits, reload plugins after the
//! `Hello`, and serve the dispatch loop. The replicable parts live here:
//!
//! - [`BackendIo`] — the four framed-stdio pieces (`stdout` / `sink` / `host` /
//!   `rt`), built in one call.
//! - [`App`] — the fluent runtime builder: `plugin_host` (host + `AppCtx` +
//!   product filter + hook dispatcher + scheduler, in one call) → `api_installer`
//!   → `init` → `run` (the serve loop, with a default post-`Hello` reload).
//! - [`Dispatcher`] — assembles the method routing from handler **groups** (the
//!   `#[handler]` inventory + reusable bundles), each with its own context, so the
//!   product never hand-rolls the maps, the name union, or the dispatch branching.
//!
//! What stays in the product binary: the concrete state, the namespace wiring, and
//! the **dispatcher groups** (they name the concrete context types). The RPC
//! *composition* — a bundle's handlers — is `arbor-rpc`'s `Builder`; this crate
//! routes the groups + runs the loop around them.
//!
//! ## Public API: use the [`prelude`]

pub mod app;
pub mod app_ctx;
pub mod dispatch;
pub mod io;
pub mod prelude;

pub use app::App;
pub use app_ctx::BackendAppCtx;
pub use dispatch::Dispatcher;
pub use io::BackendIo;
