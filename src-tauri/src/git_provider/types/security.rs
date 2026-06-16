//! Re-export — see `corvus-git-provider-api::security`.
//!
//! The wire DTOs (severity ladder, findings, summary, filters) live in the
//! contract crate; the provider-agnostic computation helpers (median age,
//! local risk score, filter application) and the GitLab/GitHub fetch logic
//! still live in `super::super::security_impl`.
pub use corvus_git_provider_api::security::*;
