//! GitHub repo CRUD — create, get, list.
//!
//! Most listing logic lives in `crate::git_provider::repo_impl` (returns
//! `RemoteRepo`, the same type aliased as `RemoteRepoInfo`). We delegate
//! there to avoid duplicating the parallel-paged fetch.

use serde::Deserialize;

use crate::git_provider::types::{
    RemoteRepoInfo, RepoCreateRequest, ListReposOpts, RepoVisibility,
    error::ProviderError,
};

use super::api;

pub async fn list_user_repos(_opts: ListReposOpts) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    crate::git_provider::repo_impl::list_repos("github")
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

pub async fn get_repo(owner: &str, name: &str) -> Result<RemoteRepoInfo, ProviderError> {
    let token = api::get_token()
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)?;

    let url = format!("{}/repos/{owner}/{name}", api::GITHUB_API_BASE);
    let client = reqwest::Client::new();
    let resp = api::github_send_with_refresh(
        |tok| client
            .get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", api::ACCEPT_JSON)
            .header("X-GitHub-Api-Version", api::API_VERSION)
            .header("User-Agent", api::USER_AGENT),
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
    struct GhRepo {
        id: i64, name: String, full_name: String,
        description: Option<String>, private: bool,
        default_branch: String, language: Option<String>,
        stargazers_count: u32, updated_at: Option<String>,
        clone_url: String, ssh_url: Option<String>, html_url: String,
        fork: bool, archived: bool, size: Option<u64>,
        owner: GhOwner,
    }
    #[derive(Deserialize)]
    struct GhOwner { login: String }

    let r: GhRepo = resp.json().await?;
    Ok(RemoteRepoInfo {
        id:              r.id.to_string(),
        name:            r.name,
        namespace:       r.owner.login,
        full_name:       r.full_name,
        description:     r.description,
        private:         r.private,
        default_branch:  r.default_branch,
        language:        r.language,
        stars:           r.stargazers_count,
        updated_at:      r.updated_at.unwrap_or_default(),
        clone_url_https: r.clone_url,
        clone_url_ssh:   r.ssh_url,
        web_url:         r.html_url,
        provider:        "github".into(),
        is_fork:         r.fork,
        is_archived:     r.archived,
        size_kb:         r.size,
    })
}

pub async fn create_repo(req: RepoCreateRequest) -> Result<RemoteRepoInfo, ProviderError> {
    let token = api::get_token()
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)?;

    let url = match &req.org {
        Some(org) => format!("{}/orgs/{org}/repos", api::GITHUB_API_BASE),
        None      => format!("{}/user/repos",       api::GITHUB_API_BASE),
    };

    let private = !matches!(req.visibility, RepoVisibility::Public);
    let body = serde_json::json!({
        "name":        req.name,
        "description": req.description.unwrap_or_default(),
        "private":     private,
        "auto_init":   false,
    });

    let client = reqwest::Client::new();
    let resp = api::github_send_with_refresh(
        |tok| client
            .post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", api::ACCEPT_JSON)
            .header("X-GitHub-Api-Version", api::API_VERSION)
            .header("User-Agent", api::USER_AGENT)
            .json(&body),
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
    let owner = v.get("owner")
        .and_then(|o| o.get("login"))
        .and_then(|l| l.as_str())
        .unwrap_or("")
        .to_string();
    let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
    get_repo(&owner, &name).await
}
