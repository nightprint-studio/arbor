//! The single canonical entry point for this crate's public API.
//!
//! Call sites import `use corvus_git_provider_api::prelude::*;` (or the
//! fully-qualified `corvus_git_provider_api::prelude::Foo`). The submodules
//! stay `pub` for rustdoc navigation, but the prelude is the path of record.

pub use crate::auth::{OAuthHandle, ProviderAuth, ProviderUser};
pub use crate::branch::BranchProtection;
pub use crate::capability::Capabilities;
pub use crate::ci::{CiFilter, CiJob, CiProviderInfo, CiRun, CiWorkflow, PipelineCreateRequest};
pub use crate::error::ProviderError;
pub use crate::issue::{IssueCreateRequest, IssueFilter, RepoIssue};
pub use crate::kind::ProviderKind;
pub use crate::mr::{
    CreateMrParams, MergeOpts, MergeRequest, MergedMrHint, MrCapabilities, MrCheck, MrComment,
    MrCommit, MrConflict, MrCreateRequest, MrDetail, MrEvent, MrFeatureStatus, MrFile, MrFileDiff,
    MrFilter, MrId, MrInfo, MrLabel, MrState, MrUpdateRequest, MrUser, ReviewState,
};
pub use crate::provider::GitProvider;
pub use crate::registry::{host_from_url, GitProviderRegistry};
pub use crate::release::{Release, ReleaseAsset, ReleaseCreateRequest};
pub use crate::repo::{
    ListReposOpts, RemoteAccount, RemoteFileContent, RemoteRepo, RemoteRepoInfo, RemoteTreeEntry,
    RepoCreateRequest, RepoRef, RepoVisibility,
};
pub use crate::security::{
    age_days_from_iso, apply_filters, compute_local_risk_score, median, medians_from_findings,
    risk_label, FindingIdentifier, FindingState, RiskScore, SecurityFilters, SecurityFinding,
    SecuritySummary, Severity, SeverityCounts, SeverityMedians, TimePoint, VulnTimeSeries,
    MAX_FINDINGS_FETCH,
};
pub use crate::webhook::{Webhook, WebhookCreateRequest};
