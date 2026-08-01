//! Every hook name Garrulus fires, as a constant (decision **D10**), under the
//! product namespace `garrulus:` (decision **D9** — see
//! `docs/plugin-api-architecture.md`).
//!
//! ## This module declares nothing — and that is the point
//!
//! The names live in `arbor_plugin_types::hook_names::garrulus`, which is also
//! what `hook_catalog.rs` is built from. A fire site and the catalog entry it has
//! to match must come from **one** constant or they drift — and they did: an
//! earlier pass gave this crate a parallel set of its own. The two agreed by
//! coincidence, disagreed on three names, and the catalog's completeness tests
//! could not see the second set at all, so `cargo test` stayed green while
//! `corvus:repo_open` was being fired at a catalog that only knew
//! `arbor:repo_open`. One registry, re-exported here so call sites keep reading
//! `hooks::NOTE_SAVED`.
//!
//! ## Why the vault infix is gone
//!
//! Two of these were `on_vault_note_saved` / `on_vault_note_deleted` purely to
//! dodge Corvus's `on_note_saved` (a *git* note on a commit) in the single flat
//! hook space. The namespace prevents that collision structurally, so the infix
//! has no job left: they are `NOTE_SAVED` / `NOTE_DELETED` under `garrulus:`, and
//! Corvus's are the same two words under `corvus:`. The `on_` prefix goes for the
//! same reason — the namespace already says what the string is.
//!
//! The prefix stays **optional when subscribing**: inside garrulus's plugin host,
//! `arbor.events.on("note_saved", fn)` can only mean `garrulus:note_saved`,
//! because the dispatcher is per-product and `NS` is the very id `garrulus-be`
//! passes to `App::plugin_host`.

/// `NS`, one constant per event, and `ALL` in declaration order.
///
/// Glob-imported deliberately: this module *is* the product's hook vocabulary,
/// and a handler reads `hooks::NOTE_SAVED` through `garrulus_core::prelude::hooks`.
pub use arbor_plugin_types::prelude::hook_names::garrulus::*;
