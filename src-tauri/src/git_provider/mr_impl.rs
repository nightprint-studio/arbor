//! MR/PR data types — defined in `corvus-git-provider-api`, re-exported here so
//! existing `mr_impl::*` call sites (commands) keep resolving.
//!
//! All MR/PR REST behavior now lives behind the `GitProvider` trait, implemented
//! by the `corvus-git-provider-{github,gitlab}` crates. This module is just the
//! DTO alias surface; it no longer holds any client code.

pub use corvus_git_provider_api::mr::*;
