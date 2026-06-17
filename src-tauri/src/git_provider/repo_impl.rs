//! Remote-repo data types — defined in `corvus-git-provider-api`, re-exported
//! here so existing `repo_impl::*` call sites keep resolving.
//!
//! The remote-browser REST behavior (account/repo listing, file-tree browsing,
//! file content) now lives behind the `GitProvider` trait, implemented by the
//! `corvus-git-provider-{github,gitlab}` crates. The host-gated inline-image
//! proxy — which is host-dynamic and not a per-provider operation — lives in
//! `super::image_proxy`. This module is just the DTO alias surface.

pub use corvus_git_provider_api::repo::*;
