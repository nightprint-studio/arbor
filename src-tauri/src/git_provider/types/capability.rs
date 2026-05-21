use serde::{Deserialize, Serialize};

/// Per-provider capability matrix.
///
/// Each field is `true` when the provider implements the corresponding
/// section of the `GitProvider` trait with real logic, `false` when it
/// returns `ProviderError::Unsupported`. The frontend can hide buttons
/// for unsupported features by reading this struct.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Capabilities {
    pub mr:                 bool,
    pub ci:                 bool,
    pub releases:           bool,
    pub issues:             bool,
    pub webhooks:           bool,
    pub branch_protection:  bool,
    pub oauth:              bool,
    pub search:             bool,
    pub security:           bool,
}

impl Capabilities {
    /// All-false starting point — useful for new stub providers.
    pub const fn none() -> Self {
        Self {
            mr: false,
            ci: false,
            releases: false,
            issues: false,
            webhooks: false,
            branch_protection: false,
            oauth: false,
            search: false,
            security: false,
        }
    }
}
