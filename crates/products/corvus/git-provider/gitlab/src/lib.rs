//! `corvus-git-provider-gitlab` — the keyring-free GitLab implementation of the
//! Corvus [`corvus_git_provider_api::prelude::GitProvider`] trait (gitlab.com +
//! self-hosted instances).
//!
//! Credentials never touch the keyring here: the struct holds an
//! `Arc<dyn arbor_ipc::prelude::SessionProvider>` plus an opaque `account`
//! string. For GitLab the `account` IS the instance base URL; the shell maps it
//! to the real keyring entry and (gitlab.com-only) runs the OAuth refresh. The
//! HTTP/session seam lives in [`http`]; the per-domain REST ports live in the
//! sibling modules (filled in a later phase).
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
