//! GitLab project (repo) CRUD — create, get, list.
//!
//! Listing delegates to `crate::git_provider::repo_impl::list_repos("gitlab")` so we
//! don't duplicate the parallel-paged fetch. `create_repo` reproduces the
//! POST /projects flow used by `git/init.rs` (Phase 4 will collapse the
//! duplicate).

use serde::Deserialize;

use crate::git_provider::types::{
    RemoteRepoInfo, RepoCreateRequest, ListReposOpts, RepoVisibility,
    error::ProviderError,
};

use super::api;

pub async fn list_user_repos(_opts: ListReposOpts) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    crate::git_provider::repo_impl::list_repos("gitlab")
        .await
        .map_err(|e| ProviderError::Internal(e.to_string()))
}

pub async fn list_org_repos(
    _org:   &str,
    _opts:  ListReposOpts,
) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    Err(ProviderError::Unsupported {
        feature: "list_org_repos".into(),
    })
}

pub async fn search_repos(_query: &str) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "search_repos".into() })
}

/// `owner` is treated as the GitLab namespace path; `name` is the project
/// slug. The full project path is `{owner}/{name}` (or just `name` when
/// owner is empty).
pub async fn get_repo(
    base_url: &str,
    owner:    &str,
    name:     &str,
) -> Result<RemoteRepoInfo, ProviderError> {
    let token = api::get_token(base_url)
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)?;

    let project_path = if owner.is_empty() {
        name.to_string()
    } else {
        format!("{owner}/{name}")
    };
    let encoded = api::percent_encode_slash(&project_path);
    let url = format!("{base_url}/api/v4/projects/{encoded}");
    let client = reqwest::Client::new();
    let resp = api::gitlab_send_with_refresh(
        |tok| client
            .get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", api::USER_AGENT),
        base_url,
        &token,
    )
    .await
    .map_err(|e| ProviderError::Network(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::Http { status, body });
    }

    #[derive(Deserialize)]
    struct GlProject {
        id:                       i64,
        name:                     String,
        path_with_namespace:      String,
        description:              Option<String>,
        visibility:               String,
        default_branch:           Option<String>,
        star_count:               u32,
        last_activity_at:         Option<String>,
        http_url_to_repo:         String,
        ssh_url_to_repo:          Option<String>,
        web_url:                  String,
        #[serde(default)]
        forked_from_project:      Option<serde_json::Value>,
        archived:                 bool,
        #[serde(default)]
        statistics:               Option<GlStats>,
        namespace:                GlNamespace,
    }
    #[derive(Deserialize)]
    struct GlStats { repository_size: Option<u64> }
    #[derive(Deserialize)]
    struct GlNamespace { full_path: String }

    let p: GlProject = resp.json().await?;
    Ok(RemoteRepoInfo {
        id:              p.id.to_string(),
        name:            p.name,
        namespace:       p.namespace.full_path,
        full_name:       p.path_with_namespace,
        description:     p.description,
        private:         p.visibility != "public",
        default_branch:  p.default_branch.unwrap_or_default(),
        language:        None,
        stars:           p.star_count,
        updated_at:      p.last_activity_at.unwrap_or_default(),
        clone_url_https: p.http_url_to_repo,
        clone_url_ssh:   p.ssh_url_to_repo,
        web_url:         p.web_url,
        provider:        "gitlab".into(),
        is_fork:         p.forked_from_project.is_some(),
        is_archived:     p.archived,
        size_kb:         p.statistics.and_then(|s| s.repository_size),
    })
}

pub async fn create_repo(
    base_url: &str,
    req:      RepoCreateRequest,
) -> Result<RemoteRepoInfo, ProviderError> {
    let token = api::get_token(base_url)
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)?;

    let visibility = match req.visibility {
        RepoVisibility::Public   => "public",
        RepoVisibility::Internal => "internal",
        RepoVisibility::Private  => "private",
    };

    let mut body = serde_json::json!({
        "name":                   req.name,
        "description":            req.description.clone().unwrap_or_default(),
        "visibility":             visibility,
        "initialize_with_readme": false,
    });
    if let Some(ns_id) = req.namespace_id {
        body["namespace_id"] = serde_json::json!(ns_id);
    }

    let url = format!("{base_url}/api/v4/projects");
    let client = reqwest::Client::new();
    let resp = api::gitlab_send_with_refresh(
        |tok| client
            .post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", api::USER_AGENT)
            .json(&body),
        base_url,
        &token,
    )
    .await
    .map_err(|e| ProviderError::Network(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::Http { status, body });
    }

    let v: serde_json::Value = resp.json().await?;
    let path = v.get("path_with_namespace")
        .and_then(|p| p.as_str())
        .ok_or_else(|| ProviderError::Internal(
            "GitLab create_repo: missing path_with_namespace".into(),
        ))?
        .to_string();
    let (owner, name) = match path.rsplit_once('/') {
        Some((o, n)) => (o.to_string(), n.to_string()),
        None         => (String::new(), path),
    };
    get_repo(base_url, &owner, &name).await
}
