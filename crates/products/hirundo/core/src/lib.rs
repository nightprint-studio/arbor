//! `hirundo-core` — Hirundo's headless state and its `.hirundo/` store.
//!
//! The backend (`hirundo-be`) holds a [`HirundoState`](state::HirundoState) and serves handlers against the
//! [`store`] — collections, environments and local values as files — with `hirundo-model` doing every bit of
//! parsing, formatting and request preparation. Secrets are never stored: [`secrets`] asks the shell's keyring.
//!
//! ## Public API: use the [`prelude`]

pub mod prelude;
pub mod secrets;
pub mod state;
pub mod store;
