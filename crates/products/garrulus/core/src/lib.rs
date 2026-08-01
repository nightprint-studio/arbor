//! `garrulus-core` — the headless backend core for Garrulus (the note-vault
//! product).
//!
//! The garrulus twin of `corvus-core` / `sitta-core`: the canonical
//! [`GarrulusState`](state::GarrulusState) the `garrulus-be` process owns,
//! **Tauri-free by construction**. Unlike sitta's (which owns nothing), this state
//! owns three long-lived pieces, because a note vault genuinely has session state:
//!
//! - the **open vault** (`garrulus-vault`) — one at a time, per process;
//! - the **index** (`garrulus-index`) — a cache, rebuilt at vault open and
//!   upserted per note on save, never the record;
//! - the configured **sync remote** (`garrulus-sync`) — the seam the sync button
//!   drives; the background only ever `probe`s it.
//!
//! Each sits behind its own `RwLock` so a read-heavy handler (search, backlinks)
//! never serialises against another. The locking discipline handlers must follow
//! is stated on [`GarrulusState`](state::GarrulusState): **drop the guard before
//! firing a hook**, or Lua running inside the hook can re-enter and deadlock.
//!
//! Modules here depend only on [`arbor_ipc`], `arbor_plugin_api`, `arbor_core`
//! (path resolution for the typed config) the garrulus leaf crates and serde — no
//! `tauri`, no `arbor_rpc`, no `arbor_be`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites (in `garrulus-be`) reach this crate's surface
//! through `garrulus_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub mod config;
pub mod hooks;
pub mod prelude;
pub mod remote;
pub mod state;
pub mod vaults;
