use serde::{Deserialize, Serialize};

use crate::kind::ProviderKind;

// ── MR/PR payloads ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MrState {
    Open,
    Closed,
    Merged,
}

impl std::fmt::Display for MrState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MrState::Open   => write!(f, "open"),
            MrState::Closed => write!(f, "closed"),
            MrState::Merged => write!(f, "merged"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrUser {
    pub login:        String,
    pub display_name: String,
    pub avatar_url:   Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrLabel {
    pub name:  String,
    pub color: String, // hex, e.g. "d73a4a"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrCheck {
    pub name:       String,
    /// "pending" | "running" | "success" | "failed" | "cancelled" | "skipped"
    pub status:     String,
    pub url:        Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrComment {
    pub id:         String,
    pub author:     MrUser,
    pub body:       String,
    pub created_at: String,
    /// Heuristic flag: true when the author looks like a bot account.
    /// GitHub: login ends with "[bot]" (the canonical bot suffix).
    /// GitLab: login or display name contains "bot" (case-insensitive).
    /// Lets the frontend hide automated comments by default.
    #[serde(default)]
    pub is_bot:     bool,
}

/// Activity entry for the MR/PR timeline — anything that's not a regular
/// user comment: state changes, label edits, assignments, force-pushes,
/// system notes, etc. Surfaced separately from `MrComment` so the UI can
/// filter Comments / Bots / Activity independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrEvent {
    pub id:         String,
    /// Coarse category — drives the icon and filter group on the frontend.
    /// Known values: "state" (closed/reopened/merged/draft toggles),
    /// "label", "assign", "review", "commit" (push/force-push),
    /// "rename", "system" (catch-all).
    pub kind:       String,
    /// The user who triggered the event. May be a bot for automated events.
    pub actor:      MrUser,
    /// Pre-rendered, human-readable summary ("added label bug",
    /// "force-pushed the source branch", "marked as ready for review", …).
    pub summary:    String,
    pub created_at: String,
}

/// Full information about a single Pull Request / Merge Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    /// Provider-native numeric ID (PR number on GitHub, MR iid on GitLab).
    pub number:        u64,
    pub title:         String,
    pub description:   String,
    pub state:         MrState,
    pub is_draft:      bool,
    pub author:        MrUser,
    pub source_branch: String,
    pub target_branch: String,
    pub web_url:       String,
    pub created_at:    String,
    pub updated_at:    String,
    pub labels:        Vec<MrLabel>,
    pub assignees:     Vec<MrUser>,
    pub reviewers:     Vec<MrUser>,
    /// "pending" | "success" | "failed" | "none"
    pub checks_status: String,
    /// Whether the MR can be cleanly merged. None = unknown.
    pub mergeable:     Option<bool>,
    /// "github" | "gitlab"
    pub provider:      String,
    pub comments_count: u32,
    /// Squash commits on merge (set at creation / from API).
    #[serde(default)]
    pub squash:        bool,
    /// Delete source branch after merge.
    #[serde(default)]
    pub delete_branch: bool,
    /// SHA of the commit that was created on the target branch when this MR/PR
    /// was merged (squash commit SHA for squash merges, merge commit SHA for
    /// regular merges).  None for open/closed-without-merge MRs.
    #[serde(default)]
    pub merge_commit_sha: Option<String>,
    /// SHA of the source branch tip at the time of merge (head.sha).
    #[serde(default)]
    pub head_sha: String,
    /// SHA of the target branch tip just before the merge (base.sha).
    #[serde(default)]
    pub base_sha: String,
    /// Auto-merge is currently armed on this PR/MR — it will merge itself when
    /// required checks pass (GitHub) / the pipeline succeeds (GitLab).
    /// While armed, the manual merge button + squash/delete-branch flags in
    /// the detail modal are suppressed; a "Disable auto-merge" affordance is
    /// shown instead.
    #[serde(default)]
    pub auto_merge_enabled: bool,
}

/// Lightweight hint for cross-referencing merged PRs/MRs in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedMrHint {
    /// Name of the source (feature) branch.
    pub source_branch:    String,
    /// SHA of the merge/squash commit created on the target branch.
    /// May not exist locally if the user hasn't fetched yet.
    pub merge_commit_sha: String,
    /// SHA of the feature branch tip at the time of merge (head.sha).
    /// Always present in the local graph.
    pub head_sha:         String,
    /// SHA of the target branch tip just before the merge (base.sha).
    /// Can be used as a fallback anchor when merge_commit_sha isn't local yet.
    pub base_sha:         String,
}

/// Parameters for creating a new PR/MR.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMrParams {
    pub title:          String,
    pub description:    Option<String>,
    pub source_branch:  String,
    pub target_branch:  String,
    pub is_draft:       bool,
    pub labels:         Vec<String>,
    /// Squash commits on merge (applied at merge-time for GitHub; set on GitLab at creation).
    #[serde(default)]
    pub squash:         bool,
    /// Delete the source branch after a successful merge.
    #[serde(default)]
    pub delete_branch:  bool,
    /// Request auto-merge once checks pass (GitHub) / pipeline succeeds (GitLab).
    /// Merge/delete-branch options remain editable later from the detail modal.
    #[serde(default)]
    pub auto_merge:     bool,
}

/// Full detail for the detail modal (MR + comments + activity events + checks).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrDetail {
    pub mr:       MergeRequest,
    pub comments: Vec<MrComment>,
    /// Timeline events (state changes, label edits, assignments, etc.).
    /// Empty when the provider/API doesn't surface them — the frontend
    /// handles that gracefully by hiding the Activity filter chip.
    #[serde(default)]
    pub events:   Vec<MrEvent>,
    pub checks:   Vec<MrCheck>,
}

/// A single changed file in a PR / MR.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrFileDiff {
    pub filename:  String,
    /// "added" | "modified" | "removed" | "renamed"
    pub status:    String,
    pub additions: u32,
    pub deletions: u32,
    pub patch:     Option<String>,
}

/// A commit belonging to a PR / MR.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrCommit {
    pub sha:     String,
    /// First line of the commit message.
    pub message: String,
    pub author:  String,
    pub date:    String,
    pub web_url: Option<String>,
}

/// Capability probe surfaced before MR/PR creation so the UI can grey out
/// or disable options the upstream provider rejects.
///
/// Currently scoped to auto-merge — GitHub gates this on the repo-level
/// `Allow auto-merge` setting, and arming MWPS on GitLab requires the
/// project to have CI jobs enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrCapabilities {
    /// `true` when arming auto-merge / MWPS at PR/MR creation time is
    /// expected to succeed.  Defaults to `true` on any detection failure
    /// (missing token, network error, …) so the user can still try.
    pub auto_merge_supported: bool,
    /// User-facing reason when `auto_merge_supported = false` — used as
    /// the disabled-checkbox tooltip.
    pub auto_merge_reason:    Option<String>,
}

impl Default for MrCapabilities {
    fn default() -> Self {
        Self { auto_merge_supported: true, auto_merge_reason: None }
    }
}

/// Whether the remote repository accepts merge/pull requests at all.
///
/// Drives the sidebar EmptyState + Command-Palette gating so a repo with
/// MRs disabled doesn't surface broken actions or 404s.  Defaults to
/// `enabled = true` on any probe failure (permissive — the user can still
/// try and the failing call will surface a normal error).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrFeatureStatus {
    pub enabled: bool,
    /// User-facing explanation when `enabled = false`.
    pub reason:  Option<String>,
}

impl Default for MrFeatureStatus {
    fn default() -> Self { Self { enabled: true, reason: None } }
}

// ── Trait-vocabulary aliases + request / filter types ────────────────────────
//
// The trait speaks the canonical spec vocabulary (`MrInfo`, `MrFile`,
// `MrCreateRequest`) while serde output stays byte-identical to the legacy
// frontend types.
pub use MergeRequest as MrInfo;
pub use MrFileDiff as MrFile;
pub use CreateMrParams as MrCreateRequest;

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

/// Options for arming auto-merge (GitHub) / merge-when-pipeline-succeeds
/// (GitLab). The provider resolves any provider-native handle it needs
/// internally (e.g. GitHub's GraphQL PR node id).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutoMergeOpts {
    /// Squash commits when the auto-merge fires.
    pub squash:        bool,
    /// Delete the source branch after the auto-merge fires. GitHub honors the
    /// repo's own "delete branch on merge" setting; GitLab takes it here.
    pub delete_branch: bool,
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
