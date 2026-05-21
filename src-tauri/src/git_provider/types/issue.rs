use serde::{Deserialize, Serialize};

/// Repo-native issue (GitHub Issues / GitLab Issues).
///
/// **Distinct** from `integrations/{linear,jira}` — those are external
/// trackers and are NOT part of this trait.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIssue {
    pub id:         String,
    pub number:     u64,
    pub title:      String,
    pub body:       Option<String>,
    /// "open" | "closed"
    pub state:      String,
    pub author:     Option<String>,
    pub assignees:  Vec<String>,
    pub labels:     Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub web_url:    String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueCreateRequest {
    pub title:     String,
    pub body:      Option<String>,
    pub assignees: Vec<String>,
    pub labels:    Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IssueFilter {
    /// "open" | "closed" | "all"
    pub state:    Option<String>,
    pub author:   Option<String>,
    pub assignee: Option<String>,
    pub labels:   Option<Vec<String>>,
    pub query:    Option<String>,
    pub page:     Option<u32>,
    pub per_page: Option<u32>,
}
