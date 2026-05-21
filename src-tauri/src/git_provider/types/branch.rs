use serde::{Deserialize, Serialize};

/// Branch-protection settings (set via REST, not git2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BranchProtection {
    pub require_pull_request:    bool,
    pub required_approvals:      u32,
    pub require_status_checks:   bool,
    pub required_status_checks:  Vec<String>,
    pub enforce_admins:          bool,
    pub allow_force_push:        bool,
    pub allow_deletions:         bool,
}
