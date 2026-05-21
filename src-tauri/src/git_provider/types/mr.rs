use serde::{Deserialize, Serialize};

use super::ProviderKind;

// ── Aliases for existing `mr/` types ─────────────────────────────────────
//
// Keep the JSON shape byte-identical to the frontend. Phase 5 moves the
// definitions in here.
pub use crate::git_provider::mr_impl::MergeRequest as MrInfo;
pub use crate::git_provider::mr_impl::MrDetail;
pub use crate::git_provider::mr_impl::MrComment;
pub use crate::git_provider::mr_impl::MrFileDiff as MrFile;
pub use crate::git_provider::mr_impl::MrCommit;
pub use crate::git_provider::mr_impl::MergedMrHint;
pub use crate::git_provider::mr_impl::CreateMrParams as MrCreateRequest;

/// Stable, provider-aware identifier for a merge / pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrId {
    pub provider: ProviderKind,
    /// GitHub: owner login. GitLab: full namespace path (e.g. `myorg/mygroup`).
    pub owner_or_path: String,
    /// GitHub: repository name. GitLab: `None` (path is in `owner_or_path`).
    pub repo_name: Option<String>,
    pub number:    u64,
}

/// Subset of fields editable via `update_mr` — every field is optional;
/// the provider only patches the ones that are `Some`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MrUpdateRequest {
    pub title:        Option<String>,
    pub description:  Option<String>,
    pub target_branch: Option<String>,
    pub draft:        Option<bool>,
    pub labels:       Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MergeOpts {
    pub squash:        bool,
    pub delete_branch: bool,
    /// GitHub-only: title for squashed commit.
    pub commit_title:  Option<String>,
    /// GitHub-only: body for squashed commit.
    pub commit_message: Option<String>,
    /// GitHub: "merge" | "squash" | "rebase". GitLab maps to its own enum.
    pub strategy:      Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrConflict {
    pub has_conflicts: bool,
    /// Files with conflict markers (when known).
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MrFilter {
    /// "open" | "closed" | "merged" | "all"
    pub state:  Option<String>,
    pub author: Option<String>,
    pub assignee: Option<String>,
    pub labels: Option<Vec<String>>,
    pub query:  Option<String>,
    pub page:     Option<u32>,
    pub per_page: Option<u32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Pending,
}
