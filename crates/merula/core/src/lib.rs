//! `merula-core` — the headless backend core for Merula (the audio / live-coding
//! product).
//!
//! The merula twin of `corvus-core`: the canonical [`MerulaState`] the `merula-be`
//! process owns, **Tauri-free by construction**. Unlike `corvus-core` (a
//! featherweight state seed) this crate is heavier — `MerulaState`'s
//! `session: Mutex<Option<Session>>` field ties the struct's *type definition* to
//! [`Session`](session::Session), which pulls in the whole audio substrate: the
//! `!Send` cpal-backed [audio thread](audio_thread), the [control](control) channel,
//! the BE→FE [events](events) contract, and the [`MerulaConfig`](config::MerulaConfig)
//! type. So the full state substrate lives here; only the `#[arbor_rpc::handler]`
//! command bodies stay in `merula-be`.
//!
//! Every module here depends only on [`arbor_ipc`], the Tauri-free `merula` facade,
//! `arbor_core` path helpers, and serde — no `tauri`, no `arbor_rpc`, no `arbor_be`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `merula_core::prelude::...`. The submodules stay `pub` for rustdoc navigation.

pub mod prelude;
pub mod state;
pub mod session;
pub mod control;
pub mod events;
pub mod config;
// The `!Send` audio-thread substrate. `pub` because a merula-be handler
// (`audio_cmds`) calls `audio_thread::build_registry` directly to pre-decode a
// registry off the RT thread.
pub mod audio_thread;
// The pack **read surface** (descriptor table + install status + the lazy
// `load_subset_into` the audio thread decodes through). The `merula_packs` /
// `merula_pack_set_active` handlers stay in merula-be and re-import these helpers.
pub mod packs;
// The sound-alias map read by the registry builder. The `get/set_merula_aliases`
// handlers stay in merula-be's `fstate`; only the read helper the audio substrate
// needs lives here.
pub mod aliases;
