use serde::{Deserialize, Serialize};

// Aliases: JSON stays byte-identical to current frontend types.
pub use crate::git_provider::ci_impl::CiRun;
pub use crate::git_provider::ci_impl::CiJob;
pub use crate::git_provider::ci_impl::CiWorkflow;
pub use crate::git_provider::ci_impl::CiProviderInfo;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CiFilter {
    pub branch:  Option<String>,
    /// "running" | "success" | "failed" | "cancelled" | "pending"
    pub status:  Option<String>,
    /// MR/PR number — when set, returns runs scoped to that MR's source branch.
    pub mr_number: Option<u64>,
    pub page:     Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCreateRequest {
    pub branch:    String,
    pub variables: Vec<(String, String)>,
    /// GitHub: workflow id or filename. None → first `workflow_dispatch` workflow.
    pub workflow_id: Option<String>,
}
