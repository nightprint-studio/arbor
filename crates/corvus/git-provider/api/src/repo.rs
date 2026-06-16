use serde::{Deserialize, Serialize};

// ── Remote repository payloads ───────────────────────────────────────────────
// `RemoteRepo` is the canonical "remote repository" struct; the trait surface
// speaks the spec name `RemoteRepoInfo` (alias below) while serde stays
// byte-identical to the current frontend types.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAccount {
    pub provider:      String,         // "github" | "gitlab"
    pub username:      String,
    pub display_name:  Option<String>,
    pub avatar_url:    Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRepo {
    pub id:               String,      // numeric ID string from API
    pub name:             String,
    pub namespace:        String,      // org or user login
    pub full_name:        String,      // "namespace/name"
    pub description:      Option<String>,
    pub private:          bool,
    pub default_branch:   String,
    pub language:         Option<String>,
    pub stars:            u32,
    pub updated_at:       String,      // ISO 8601
    pub clone_url_https:  String,
    pub clone_url_ssh:    Option<String>,
    pub web_url:          String,
    pub provider:         String,
    pub is_fork:          bool,
    pub is_archived:      bool,
    pub size_kb:          Option<u64>,
}

/// Spec name used by the `GitProvider` trait surface.
pub type RemoteRepoInfo = RemoteRepo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTreeEntry {
    pub name:       String,
    pub path:       String,
    pub entry_type: String,    // "file" | "dir" | "submodule" | "symlink"
    pub size:       Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFileContent {
    pub path:       String,
    pub content:    String,            // UTF-8 text (empty for binary/image)
    pub image_data: Option<String>,    // data:<mime>;base64,<data>
    pub size:       u64,
    pub is_binary:  bool,
    pub is_image:   bool,
    pub mime_type:  Option<String>,
}

// ── Trait-vocabulary request / filter types ──────────────────────────────────

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
