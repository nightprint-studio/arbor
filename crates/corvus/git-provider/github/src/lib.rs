//! `corvus-git-provider-github` — the keyring-free GitHub implementation of the
//! Corvus [`corvus_git_provider_api::prelude::GitProvider`] trait.
//!
//! Credentials never touch the keyring here: the struct holds an
//! `Arc<dyn arbor_ipc::prelude::SessionProvider>` plus an opaque `account`
//! string, and the shell maps that account to the real keyring entry and runs
//! the OAuth refresh. The HTTP/session seam lives in [`http`]; the per-domain
//! REST ports live in the sibling modules (filled in a later phase).
//!
//! ## Public API: use the [`prelude`].

// The domain stubs + placeholder return-shaping leave items unused until the
// assembly phase wires them into the `GitProvider` impl.
#![allow(dead_code)]

pub(crate) mod http;

mod auth;
mod avatar;
mod branch;
mod ci;
mod issues;
mod mr;
mod provider;
mod releases;
mod repo;
mod security;
mod webhooks;

pub mod prelude;
