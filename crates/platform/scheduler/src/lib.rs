//! Single trigger engine (FixedRate / FixedDelay / Cron) with cooperative
//! cancellation, per-tick focus + custom gating, and on-the-fly mutability
//! (cancel / enable / disable / swap trigger without re-registering).
//!
//! See `README.md` for the rationale (why this crate exists at all) and
//! [`prelude`] for the canonical public surface — call sites should reach
//! types through `arbor_scheduler::prelude::...` rather than the per-feature
//! submodules. The submodules stay `pub` for rustdoc navigation.

pub mod action;
pub mod error;
pub mod key;
pub mod opts;
pub mod prelude;
pub mod scheduler;
pub mod snapshot;
pub mod trigger;

mod runner;
