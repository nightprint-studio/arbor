//! Public re-exports for the security dashboard surface.
//!
//! Concrete types live in `crate::git_provider::security_impl`; this module
//! mirrors the convention used by `types/ci.rs` and keeps the canonical
//! `git_provider::types::*` namespace consistent.

pub use crate::git_provider::security_impl::{
    Severity,
    FindingState,
    FindingIdentifier,
    SecurityFinding,
    SeverityCounts,
    SeverityMedians,
    RiskScore,
    TimePoint,
    VulnTimeSeries,
    SecuritySummary,
    SecurityFilters,
    MAX_FINDINGS_FETCH,
};
