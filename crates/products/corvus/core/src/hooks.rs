//! Every hook name Corvus fires, as a constant (decision **D10**), under the
//! product namespace `corvus:` (decision **D9** — see
//! `docs/plugin-api-architecture.md`).
//!
//! ## This module declares nothing — and that is the point
//!
//! The names live in `arbor_plugin_types::hook_names::corvus`, which is also what
//! `hook_catalog.rs` is built from. A fire site and the catalog entry it has to
//! match must come from **one** constant or they drift — and they did: an earlier
//! pass gave this crate a parallel set of its own. The two agreed by coincidence,
//! disagreed on three names, and the catalog's completeness tests could not see
//! the second set at all, so `cargo test` stayed green while `corvus:repo_open`
//! was being fired at a catalog that only knew `arbor:repo_open` — a hook no
//! subscriber could ever match, where the spelling that *did* match the fire was
//! the one the subscribe warning called unknown. One registry, re-exported here
//! so call sites keep reading `hooks::COMMIT`.
//!
//! ## What is deliberately NOT here
//!
//! `repo_open`, `repo_close` and `tab_switch` are **`arbor:`** names, not
//! `corvus:` ones, because the shell and `arbor-plugin-rpc`'s reload path fire
//! them too and have no product id to prefix with. Reach them through
//! [`crate::prelude::arbor_hooks`]. A `corvus:` twin is exactly what made them
//! dead hooks once already.
//!
//! ## Why the namespace
//!
//! Hooks were the last part of the Lua surface living in a single flat space:
//! `arbor.repo.*` / `arbor.notes.*` are namespaced, and `arbor.events.emit`
//! already qualifies an unprefixed event with the emitting plugin's name.
//! Garrulus proved the gap — `on_note_saved` meant "git note written" here and
//! "vault note written" there. With `corvus:note_saved` vs `garrulus:note_saved`
//! the collision is structurally impossible instead of avoided by hand, and the
//! `on_` prefix goes away because the namespace already says what the string is.
//!
//! The prefix stays **optional when subscribing**: inside a corvus plugin host,
//! `arbor.events.on("commit", fn)` can only mean `corvus:commit`, because the
//! dispatcher is per-product and `NS` is the very id passed to
//! `App::plugin_host`.

/// `NS`, one constant per event, and `ALL` in declaration order.
///
/// Glob-imported deliberately: this module *is* the product's hook vocabulary,
/// and a handler reads `hooks::COMMIT` through `corvus_core::prelude::hooks`.
pub use arbor_plugin_types::prelude::hook_names::corvus::*;
