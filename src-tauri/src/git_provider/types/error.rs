//! Re-export of the provider-contract error type.
//!
//! `ProviderError` lives in `corvus-git-provider-api`. The shell-side
//! `AppError` → `ProviderError` conversion is `super::super::app_err_to_provider`
//! (it can't be a `From` impl across the crate boundary — orphan rule).
pub use corvus_git_provider_api::error::*;
