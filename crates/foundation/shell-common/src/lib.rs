//! `arbor-shell-common` — the Arbor shell runtime for Model D (1 FE + N BE).
//!
//! Two responsibilities, both owned by the shell process (the holder of the
//! single WebView):
//!
//! - [`router::Router`] — maps a FE `invoke` to the right backend over
//!   [`arbor_ipc`], and (later) relays the backend's push events to the FE as
//!   Tauri events.
//! - [`broker::CredentialBroker`] — the *sole* keyring holder. FE, product
//!   backends and plugins never touch the keyring; they ask the broker. It
//!   caches short-lived access tokens in memory (refresh secrets stay in the
//!   keyring), with a TTL, invalidation on 401/403, and `zeroize`-on-drop.
//!   See `docs/crate-refactor-round2.md` §D.5.
//!
//! M1c scope: the router registry + dispatch and the keyring-backed broker with
//! caching. The host WebView2 / window-management / single-instance / deep-link
//! pieces, and event relay to the FE, fold in as the shell takes over from
//! `src-tauri` (M3).
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach this crate's surface through
//! `arbor_shell_common::prelude::...`.

pub mod broker;
pub mod prelude;
pub mod router;
