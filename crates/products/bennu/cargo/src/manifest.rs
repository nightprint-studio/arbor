//! The span-aware TOML reader — **moved to `bennu-toml`**.
//!
//! It was never about Cargo: it records where every table and key is, and knows nothing about what
//! any of them mean. The second consumer (`bennu-fulcrum-i18n`, which needs the same spans to
//! navigate to a message key) is what made that worth saying out loud in the crate graph.
//!
//! This module stays as the re-export so `bennu_cargo::prelude::Manifest` — the path every call
//! site in the workspace already uses — keeps meaning what it meant.
pub use bennu_toml::prelude::*;
