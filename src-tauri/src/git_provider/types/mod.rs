//! Type definitions for the `GitProvider` trait surface.
//!
//! Struct definitions live in the impl modules
//! `crate::git_provider::{mr_impl, ci_impl, repo_impl}` (relocated from
//! `crate::mr`, `crate::pipeline::ci_client`, `crate::remote_browser` in
//! Phase 5).  This module re-exports them under their spec names
//! (e.g. `MrInfo = MergeRequest`) so the trait speaks the canonical
//! vocabulary while serde output stays byte-identical to the legacy
//! frontend types.

#![allow(unused_imports)]

pub mod error;
pub mod capability;
pub mod auth;
pub mod repo;
pub mod mr;
pub mod ci;
pub mod release;
pub mod issue;
pub mod webhook;
pub mod branch;
pub mod security;

pub use error::ProviderError;
pub use capability::Capabilities;
pub use auth::{ProviderAuth, ProviderUser, OAuthHandle};
pub use repo::{RemoteRepoInfo, RepoCreateRequest, RepoVisibility, ListReposOpts, RepoRef};
pub use mr::{
    MrInfo, MrDetail, MrComment, MrFile, MrCommit, MergedMrHint,
    MrCreateRequest, MrUpdateRequest, MrId, MergeOpts, MrConflict,
    MrFilter, ReviewState,
};
pub use ci::{CiRun, CiJob, CiWorkflow, CiProviderInfo, CiFilter, PipelineCreateRequest};
pub use release::{Release, ReleaseAsset, ReleaseCreateRequest};
pub use issue::{RepoIssue, IssueCreateRequest, IssueFilter};
pub use webhook::{Webhook, WebhookCreateRequest};
pub use branch::BranchProtection;
pub use security::{
    Severity, FindingState, FindingIdentifier, SecurityFinding,
    SeverityCounts, SeverityMedians, RiskScore, TimePoint, VulnTimeSeries,
    SecuritySummary, SecurityFilters, MAX_FINDINGS_FETCH,
};

// `ProviderKind` lives in the parent module — re-exported here so the
// `mr.rs` types can reference it without a circular path. We import-and-
// re-export rather than duplicate.
pub use super::ProviderKind;
