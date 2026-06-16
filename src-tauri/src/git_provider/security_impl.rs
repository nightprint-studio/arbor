//! Security dashboard — common types re-export.
//!
//! The wire DTOs, the `Severity` / `FindingState` enums and the pure
//! computation helpers (`age_days_from_iso`, `median`, `medians_from_findings`,
//! `compute_local_risk_score`, `risk_label`, `apply_filters`) live in
//! `corvus-git-provider-api`. They are re-exported here so external
//! `security_impl::*` call sites keep resolving.

pub use corvus_git_provider_api::security::*;
