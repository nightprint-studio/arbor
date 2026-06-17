//! Re-export — see `corvus-git-provider-api::security`.
//!
//! The wire DTOs (severity ladder, findings, summary, filters) AND the
//! provider-agnostic computation helpers (median age, local risk score, filter
//! application) all live in the contract crate now; the GitLab/GitHub fetch
//! logic lives in the `corvus-git-provider-{github,gitlab}` crates behind the
//! `GitProvider` trait.
pub use corvus_git_provider_api::security::*;
