use serde::{Deserialize, Serialize};

/// Re-export the existing remote browser type as the canonical
/// "remote repository" struct so the trait surface speaks the spec name
/// while serde stays byte-identical to current frontend types.
pub use crate::git_provider::repo_impl::RemoteRepo as RemoteRepoInfo;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoVisibility {
    Public,
    Private,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoCreateRequest {
    pub name:        String,
    pub description: Option<String>,
    pub visibility:  RepoVisibility,
    /// GitHub-only: organization to create the repo under (None → user account).
    pub org:         Option<String>,
    /// GitLab-only: numeric namespace ID (None → user namespace).
    pub namespace_id: Option<u64>,
}

/// Pagination + filter knobs for `list_user_repos` / `list_org_repos`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListReposOpts {
    pub page:     Option<u32>,
    pub per_page: Option<u32>,
    /// Free-text query (provider-specific behavior).
    pub query:    Option<String>,
}

/// Lightweight repo identifier passed to repo-scoped trait methods.
///
/// GitHub: `owner_or_path` = owner login, `name` = repo name.
/// GitLab: `owner_or_path` = full project path (e.g. `myorg/sub/myrepo`),
/// `name` = `None` (the path is self-contained).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRef {
    pub owner_or_path: String,
    pub name:          Option<String>,
}

impl RepoRef {
    pub fn github(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self { owner_or_path: owner.into(), name: Some(name.into()) }
    }

    pub fn gitlab(project_path: impl Into<String>) -> Self {
        Self { owner_or_path: project_path.into(), name: None }
    }
}
