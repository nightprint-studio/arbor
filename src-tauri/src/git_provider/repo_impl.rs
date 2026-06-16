use serde::Deserialize;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use crate::error::{AppError, Result};
use crate::git_provider::ci_impl::{
    get_github_token, get_gitlab_token,
    github_send_with_refresh, gitlab_send_with_refresh,
};

const MAX_PREVIEW_BYTES: u64 = 512 * 1024; // 512 KB text preview limit
const MAX_IMAGE_BYTES:   u64 = 5 * 1024 * 1024; // 5 MB image limit

// ---------------------------------------------------------------------------
// Public types — defined in `corvus-git-provider-api`, re-exported here so the
// REST client code below and external `repo_impl::*` call sites keep resolving.
// ---------------------------------------------------------------------------

pub use corvus_git_provider_api::repo::*;

// ---------------------------------------------------------------------------
// Account listing
// ---------------------------------------------------------------------------

pub async fn list_accounts() -> Vec<RemoteAccount> {
    let mut accounts = Vec::new();
    if let Some(acc) = fetch_github_account().await  { accounts.push(acc); }
    if let Some(acc) = fetch_gitlab_account("https://gitlab.com").await { accounts.push(acc); }
    accounts
}

async fn fetch_github_account() -> Option<RemoteAccount> {
    let token = get_github_token().ok().flatten()?;
    let client = reqwest::Client::new();
    let resp = github_send_with_refresh(
        |tok| client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "arbor-git-gui/1.0"),
        &token,
    )
    .await
    .ok()?;

    if !resp.status().is_success() { return None; }

    #[derive(Deserialize)]
    struct GhUser { login: String, name: Option<String>, avatar_url: Option<String> }
    let u: GhUser = resp.json().await.ok()?;
    Some(RemoteAccount {
        provider:     "github".into(),
        username:     u.login,
        display_name: u.name,
        avatar_url:   u.avatar_url,
    })
}

async fn fetch_gitlab_account(base_url: &str) -> Option<RemoteAccount> {
    let token = get_gitlab_token(base_url).ok().flatten()?;
    let client = reqwest::Client::new();
    let url = format!("{base_url}/api/v4/user");
    let resp = gitlab_send_with_refresh(
        |tok| client
            .get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base_url,
        &token,
    )
    .await
    .ok()?;

    if !resp.status().is_success() { return None; }

    #[derive(Deserialize)]
    struct GlUser { username: String, name: Option<String>, avatar_url: Option<String> }
    let u: GlUser = resp.json().await.ok()?;
    Some(RemoteAccount {
        provider:     "gitlab".into(),
        username:     u.username,
        display_name: u.name,
        avatar_url:   u.avatar_url,
    })
}

// ---------------------------------------------------------------------------
// Repository listing
// ---------------------------------------------------------------------------

pub async fn list_repos(provider: &str) -> Result<Vec<RemoteRepo>> {
    match provider {
        "github" => list_github_repos().await,
        "gitlab" => list_gitlab_repos("https://gitlab.com").await,
        _ => Err(AppError::Other(format!("Unknown provider: {provider}"))),
    }
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

fn map_gh_repo(r: GhRepo) -> RemoteRepo {
    RemoteRepo {
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
    }
}

/// Parse GitHub's RFC 5988 Link header to find `rel="last"` and extract its
/// page number.  Returns None if no last link is present (single page case).
fn github_last_page(link_header: &str) -> Option<u32> {
    for part in link_header.split(',') {
        let part = part.trim();
        if !part.contains("rel=\"last\"") { continue; }
        let url_start = part.find('<')?;
        let url_end   = part.find('>')?;
        let url       = &part[url_start + 1..url_end];
        // Look for `&page=N` or `?page=N`
        let page_idx = url.find("page=")?;
        let after    = &url[page_idx + 5..];
        let n_str    = after.split(|c: char| !c.is_ascii_digit()).next()?;
        return n_str.parse().ok();
    }
    None
}

const GH_REPOS_PER_PAGE: u32 = 100;

fn github_repos_url(page: u32) -> String {
    format!(
        "https://api.github.com/user/repos\
         ?per_page={GH_REPOS_PER_PAGE}&page={page}&sort=updated\
         &affiliation=owner,collaborator,organization_member"
    )
}

async fn fetch_github_repos_page(client: &reqwest::Client, token: &str, page: u32) -> Result<(Vec<GhRepo>, Option<String>)> {
    let resp = client.get(github_repos_url(page))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub repos request: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub repos API {s}: {b}")));
    }

    let link = resp.headers().get("link")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let page_repos: Vec<GhRepo> = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub repos parse: {e}")))?;
    Ok((page_repos, link))
}

async fn list_github_repos() -> Result<Vec<RemoteRepo>> {
    let token = get_github_token()?
        .ok_or_else(|| AppError::AuthFailed("No GitHub token".into()))?;
    let client = reqwest::Client::new();

    // Fire page 1 first — its Link header tells us how many more pages we
    // need to fetch.  Without this, we'd pay the round-trip cost N times in
    // sequence; for 200+ repos that's the bulk of the 30s wait.
    let (first, link) = fetch_github_repos_page(&client, &token, 1).await?;
    let last_page = link.as_deref().and_then(github_last_page).unwrap_or(1);

    let mut repos: Vec<RemoteRepo> = first.into_iter().map(map_gh_repo).collect();

    if last_page <= 1 {
        return Ok(repos);
    }

    // Fetch remaining pages concurrently.  GitHub's primary rate limit is
    // 5000 req/h for authenticated users, so firing ~10 pages at once is
    // well within budget.  Pages arrive in completion order; we tag each
    // task with its page number and re-sort at the end so 'sort=updated'
    // semantics are preserved across batches.
    let mut set = tokio::task::JoinSet::new();
    for page in 2..=last_page {
        let client = client.clone();
        let token  = token.clone();
        set.spawn(async move {
            let r = fetch_github_repos_page(&client, &token, page).await;
            (page, r)
        });
    }
    let mut pages_out: Vec<(u32, Vec<GhRepo>)> = Vec::with_capacity((last_page - 1) as usize);
    while let Some(joined) = set.join_next().await {
        let (page, res) = joined
            .map_err(|e| AppError::Other(format!("GitHub repos task panicked: {e}")))?;
        let (batch, _) = res?;
        pages_out.push((page, batch));
    }
    pages_out.sort_by_key(|(p, _)| *p);
    for (_, batch) in pages_out {
        repos.extend(batch.into_iter().map(map_gh_repo));
    }

    Ok(repos)
}

#[derive(Deserialize)]
struct GlProject {
    id: i64,
    name: String,
    path_with_namespace: String,
    namespace: GlNamespace,
    #[serde(default)]
    description: Option<String>,
    /// Some GitLab views (e.g. response variants for users without
    /// elevated scopes, or fields removed in newer API versions) omit
    /// `visibility`.  Treat absence as "private" — safer than failing the
    /// whole listing on a single missing field.
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    default_branch: Option<String>,
    #[serde(default)]
    last_activity_at: Option<String>,
    http_url_to_repo: String,
    #[serde(default)]
    ssh_url_to_repo: Option<String>,
    web_url: String,
    #[serde(default)]
    forked_from_project: Option<serde_json::Value>,
    #[serde(default)]
    archived: bool,
}
#[derive(Deserialize)]
struct GlNamespace { path: String, full_path: Option<String> }

fn map_gl_project(r: GlProject) -> RemoteRepo {
    let name = r.path_with_namespace
        .rsplitn(2, '/')
        .next()
        .unwrap_or(&r.name)
        .to_string();
    let namespace = r.namespace.full_path
        .unwrap_or_else(|| r.namespace.path.clone());
    RemoteRepo {
        id:              r.id.to_string(),
        name,
        namespace,
        full_name:       r.path_with_namespace,
        description:     r.description,
        private:         r.visibility.as_deref() != Some("public"),
        default_branch:  r.default_branch.unwrap_or_else(|| "main".into()),
        language:        None,
        stars:           0,
        updated_at:      r.last_activity_at.unwrap_or_default(),
        clone_url_https: r.http_url_to_repo,
        clone_url_ssh:   r.ssh_url_to_repo,
        web_url:         r.web_url,
        provider:        "gitlab".into(),
        is_fork:         r.forked_from_project.is_some(),
        is_archived:     r.archived,
        // statistics=true was dropped — it forced GitLab to compute repo
        // size for every project, which on a 200+ project list was the
        // single biggest contributor to the 30s cold-load.  size_kb is
        // nice-to-have only; the list view doesn't display it.
        size_kb:         None,
    }
}

const GL_REPOS_PER_PAGE: u32 = 100;

fn gitlab_repos_url(base_url: &str, page: u32) -> String {
    // NOTE: do NOT add `simple=true` — GitLab strips out `visibility`,
    // `archived`, and `forked_from_project` from the simple response which
    // breaks deserialization (and we genuinely need those fields).  The
    // win from this branch was dropping `statistics=true`; that's what
    // killed the 30s+ wait, not the simple flag.
    format!(
        "{base_url}/api/v4/projects\
         ?membership=true&per_page={GL_REPOS_PER_PAGE}\
         &page={page}&order_by=last_activity_at"
    )
}

async fn fetch_gitlab_repos_page(
    client: &reqwest::Client, token: &str, base_url: &str, page: u32,
) -> Result<(Vec<GlProject>, Option<u32>)> {
    let resp = client.get(gitlab_repos_url(base_url, page))
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitLab repos request: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab repos API {s}: {b}")));
    }

    let total_pages = resp.headers().get("x-total-pages")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok());

    let page_repos: Vec<GlProject> = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab repos parse: {e}")))?;
    Ok((page_repos, total_pages))
}

async fn list_gitlab_repos(base_url: &str) -> Result<Vec<RemoteRepo>> {
    let token = get_gitlab_token(base_url)?
        .ok_or_else(|| AppError::AuthFailed("No GitLab token".into()))?;
    let client = reqwest::Client::new();

    // First page tells us total page count via X-Total-Pages.  Note that
    // GitLab can return 0/missing total counts on very large instances —
    // in that case we fall back to sequential paging.
    let (first, total_pages) = fetch_gitlab_repos_page(&client, &token, base_url, 1).await?;
    let mut repos: Vec<RemoteRepo> = first.into_iter().map(map_gl_project).collect();

    let last_page = match total_pages {
        Some(n) if n > 1 => n,
        Some(_) | None   => return Ok(repos),
    };

    let mut set = tokio::task::JoinSet::new();
    for page in 2..=last_page {
        let client   = client.clone();
        let token    = token.clone();
        let base_url = base_url.to_string();
        set.spawn(async move {
            let r = fetch_gitlab_repos_page(&client, &token, &base_url, page).await;
            (page, r)
        });
    }
    let mut pages_out: Vec<(u32, Vec<GlProject>)> = Vec::with_capacity((last_page - 1) as usize);
    while let Some(joined) = set.join_next().await {
        let (page, res) = joined
            .map_err(|e| AppError::Other(format!("GitLab repos task panicked: {e}")))?;
        let (batch, _) = res?;
        pages_out.push((page, batch));
    }
    pages_out.sort_by_key(|(p, _)| *p);
    for (_, batch) in pages_out {
        repos.extend(batch.into_iter().map(map_gl_project));
    }

    Ok(repos)
}

// ---------------------------------------------------------------------------
// File tree browsing
// ---------------------------------------------------------------------------

pub async fn browse_tree(
    provider:  &str,
    full_name: &str,
    path:      &str,
    branch:    &str,
) -> Result<Vec<RemoteTreeEntry>> {
    match provider {
        "github" => {
            let (owner, repo) = split_full_name(full_name)?;
            browse_github_tree(owner, repo, path, branch).await
        }
        "gitlab" => browse_gitlab_tree(full_name, "https://gitlab.com", path, branch).await,
        _ => Err(AppError::Other(format!("Unknown provider: {provider}"))),
    }
}

async fn browse_github_tree(
    owner:  &str,
    repo:   &str,
    path:   &str,
    branch: &str,
) -> Result<Vec<RemoteTreeEntry>> {
    let token = get_github_token()?
        .ok_or_else(|| AppError::AuthFailed("No GitHub token".into()))?;

    let url = if path.is_empty() {
        format!("https://api.github.com/repos/{owner}/{repo}/contents?ref={branch}")
    } else {
        format!("https://api.github.com/repos/{owner}/{repo}/contents/{path}?ref={branch}")
    };

    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub tree request: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub contents API {s}: {b}")));
    }

    #[derive(Deserialize)]
    struct GhEntry {
        name: String, path: String,
        #[serde(rename = "type")] entry_type: String,
        size: Option<u64>,
    }

    let mut entries: Vec<RemoteTreeEntry> = resp.json::<Vec<GhEntry>>().await
        .map_err(|e| AppError::Other(format!("GitHub tree parse: {e}")))?
        .into_iter()
        .map(|e| RemoteTreeEntry {
            name: e.name,
            path: e.path,
            entry_type: match e.entry_type.as_str() {
                "dir"       => "dir",
                "symlink"   => "symlink",
                "submodule" => "submodule",
                _           => "file",
            }.into(),
            size: e.size,
        })
        .collect();

    sort_tree(&mut entries);
    Ok(entries)
}

async fn browse_gitlab_tree(
    full_name: &str,
    base_url:  &str,
    path:      &str,
    branch:    &str,
) -> Result<Vec<RemoteTreeEntry>> {
    let token = get_gitlab_token(base_url)?
        .ok_or_else(|| AppError::AuthFailed("No GitLab token".into()))?;

    let encoded  = encode_slash(full_name);
    let mut all  = Vec::new();
    let mut page = 1u32;

    loop {
        let url = format!(
            "{base_url}/api/v4/projects/{encoded}/repository/tree\
             ?path={path}&ref={branch}&per_page=100&page={page}"
        );
        let resp = reqwest::Client::new()
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .send().await
            .map_err(|e| AppError::Other(format!("GitLab tree request: {e}")))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(AppError::Other(format!("GitLab tree API {s}: {b}")));
        }

        #[derive(Deserialize)]
        struct GlEntry {
            name: String, path: String,
            #[serde(rename = "type")] entry_type: String,
        }

        let batch: Vec<GlEntry> = resp.json().await
            .map_err(|e| AppError::Other(format!("GitLab tree parse: {e}")))?;
        let done = batch.len() < 100;

        for e in batch {
            all.push(RemoteTreeEntry {
                name: e.name,
                path: e.path,
                entry_type: if e.entry_type == "tree" { "dir" } else { "file" }.into(),
                size: None,
            });
        }
        if done { break; }
        page += 1;
    }

    sort_tree(&mut all);
    Ok(all)
}

// ---------------------------------------------------------------------------
// File content
// ---------------------------------------------------------------------------

pub async fn get_file_content(
    provider:  &str,
    full_name: &str,
    path:      &str,
    branch:    &str,
) -> Result<RemoteFileContent> {
    match provider {
        "github" => {
            let (owner, repo) = split_full_name(full_name)?;
            fetch_github_file(owner, repo, path, branch).await
        }
        "gitlab" => fetch_gitlab_file(full_name, "https://gitlab.com", path, branch).await,
        _ => Err(AppError::Other(format!("Unknown provider: {provider}"))),
    }
}

async fn fetch_github_file(
    owner:  &str,
    repo:   &str,
    path:   &str,
    branch: &str,
) -> Result<RemoteFileContent> {
    let token = get_github_token()?
        .ok_or_else(|| AppError::AuthFailed("No GitHub token".into()))?;
    let url   = format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}");
    let resp  = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub raw file request: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub raw file {s}: {b}")));
    }

    let bytes = resp.bytes().await
        .map_err(|e| AppError::Other(format!("GitHub file read: {e}")))?;
    let mime  = mime_for_path(path);
    build_file_content(path, bytes.to_vec(), &mime)
}

async fn fetch_gitlab_file(
    full_name: &str,
    base_url:  &str,
    path:      &str,
    branch:    &str,
) -> Result<RemoteFileContent> {
    let token          = get_gitlab_token(base_url)?
        .ok_or_else(|| AppError::AuthFailed("No GitLab token".into()))?;
    let encoded_proj   = encode_slash(full_name);
    let encoded_file   = encode_path_component(path);
    let url = format!(
        "{base_url}/api/v4/projects/{encoded_proj}/repository/files/{encoded_file}/raw?ref={branch}"
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitLab raw file request: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab raw file {s}: {b}")));
    }

    let bytes = resp.bytes().await
        .map_err(|e| AppError::Other(format!("GitLab file read: {e}")))?;
    let mime  = mime_for_path(path);
    build_file_content(path, bytes.to_vec(), &mime)
}

fn build_file_content(path: &str, bytes: Vec<u8>, mime: &str) -> Result<RemoteFileContent> {
    let size     = bytes.len() as u64;
    let is_image = mime.starts_with("image/");

    if is_image {
        if size > MAX_IMAGE_BYTES {
            return Ok(RemoteFileContent {
                path: path.into(), content: String::new(), image_data: None,
                size, is_binary: true, is_image: true,
                mime_type: Some(mime.into()),
            });
        }
        return Ok(RemoteFileContent {
            path:       path.into(),
            content:    String::new(),
            image_data: Some(format!("data:{mime};base64,{}", BASE64.encode(&bytes))),
            size,
            is_binary:  false,
            is_image:   true,
            mime_type:  Some(mime.into()),
        });
    }

    if size > MAX_PREVIEW_BYTES {
        return Ok(RemoteFileContent {
            path: path.into(), content: String::new(), image_data: None,
            size, is_binary: true, is_image: false,
            mime_type: Some(mime.into()),
        });
    }

    match String::from_utf8(bytes) {
        Ok(text) => Ok(RemoteFileContent {
            path: path.into(), content: text, image_data: None,
            size, is_binary: false, is_image: false,
            mime_type: Some(mime.into()),
        }),
        Err(_) => Ok(RemoteFileContent {
            path: path.into(), content: String::new(), image_data: None,
            size, is_binary: true, is_image: false,
            mime_type: Some(mime.into()),
        }),
    }
}

// ---------------------------------------------------------------------------
// Inline image fetch (MR/PR body & comment preview)
// ---------------------------------------------------------------------------

/// Fetch an image referenced inline by an MR/PR body or comment for preview.
///
/// The provider token is attached ONLY when the target host belongs to the
/// provider (github.com / the configured GitLab host). Public CDN assets — e.g.
/// `user-images.githubusercontent.com` for GitHub, or anything off-host — are
/// fetched anonymously so the token is never sent to a third party. Relative
/// GitLab `/uploads/...` URLs are resolved against `base_url`.
pub async fn fetch_image_bytes(
    provider: &str,
    base_url: Option<&str>,
    url:      &str,
) -> Result<(Vec<u8>, Option<String>)> {
    match provider {
        "github" => fetch_github_image(url).await,
        "gitlab" => fetch_gitlab_image(base_url.unwrap_or("https://gitlab.com"), url).await,
        _ => Err(AppError::Other(format!("Unknown provider: {provider}"))),
    }
}

async fn fetch_github_image(url: &str) -> Result<(Vec<u8>, Option<String>)> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(AppError::Other("Relative GitHub image URLs are not supported".into()));
    }
    // Only github.com / api.github.com get the bearer token. `*.githubusercontent.com`
    // (user-images, objects/S3) are public and must NOT receive it.
    let is_github = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_lowercase()))
        .map(|h| h == "github.com" || h == "api.github.com")
        .unwrap_or(false);

    let client = reqwest::Client::new();
    let resp = if is_github {
        let token = get_github_token()?
            .ok_or_else(|| AppError::AuthFailed("Not connected to GitHub".into()))?;
        github_send_with_refresh(
            |tok| client
                .get(url)
                .header("Authorization", format!("Bearer {tok}"))
                .header("Accept", "*/*")
                .header("User-Agent", "arbor-git-gui/1.0"),
            &token,
        ).await?
    } else {
        client
            .get(url)
            .header("Accept", "*/*")
            .header("User-Agent", "arbor-git-gui/1.0")
            .send()
            .await
            .map_err(|e| AppError::Other(format!("Image request failed: {e}")))?
    };
    image_resp_to_bytes(resp).await
}

async fn fetch_gitlab_image(base_url: &str, url: &str) -> Result<(Vec<u8>, Option<String>)> {
    let base = base_url.trim_end_matches('/');
    let abs = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if url.starts_with('/') {
        format!("{base}{url}")
    } else {
        format!("{base}/{url}")
    };

    let base_host = reqwest::Url::parse(base)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_lowercase()));
    let url_host = reqwest::Url::parse(&abs)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_lowercase()));
    let same_host = matches!((&base_host, &url_host), (Some(a), Some(b)) if a == b);

    let client = reqwest::Client::new();
    let resp = if same_host {
        let token = get_gitlab_token(base)?
            .ok_or_else(|| AppError::AuthFailed("Not connected to GitLab".into()))?;
        gitlab_send_with_refresh(
            |tok| client
                .get(&abs)
                .header("Authorization", format!("Bearer {tok}"))
                .header("Accept", "*/*")
                .header("User-Agent", "arbor-git-gui/1.0"),
            base,
            &token,
        ).await?
    } else {
        client
            .get(&abs)
            .header("Accept", "*/*")
            .header("User-Agent", "arbor-git-gui/1.0")
            .send()
            .await
            .map_err(|e| AppError::Other(format!("Image request failed: {e}")))?
    };
    image_resp_to_bytes(resp).await
}

async fn image_resp_to_bytes(resp: reqwest::Response) -> Result<(Vec<u8>, Option<String>)> {
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::AuthFailed("Image access denied".into()));
    }
    if !status.is_success() {
        return Err(AppError::Other(format!("Image HTTP {status}")));
    }
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Other(format!("Image read: {e}")))?;
    Ok((bytes.to_vec(), ctype))
}

// ---------------------------------------------------------------------------
// File download to disk
// ---------------------------------------------------------------------------

pub async fn download_file(
    provider:  &str,
    full_name: &str,
    path:      &str,
    branch:    &str,
    dest_path: &str,
) -> Result<()> {
    let file = get_file_content(provider, full_name, path, branch).await?;

    // Re-fetch as raw bytes so we can write any content type correctly.
    // For text files, content is UTF-8; for images, we have base64 data.
    if let Some(data_url) = file.image_data {
        if let Some(b64) = data_url.split(',').nth(1) {
            let bytes = BASE64.decode(b64)
                .map_err(|e| AppError::Other(format!("Base64 decode: {e}")))?;
            std::fs::write(dest_path, bytes)
                .map_err(|e| AppError::Other(format!("Write file: {e}")))?;
            return Ok(());
        }
    }
    if !file.content.is_empty() {
        std::fs::write(dest_path, file.content.as_bytes())
            .map_err(|e| AppError::Other(format!("Write file: {e}")))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn split_full_name(full_name: &str) -> Result<(&str, &str)> {
    let mut parts = full_name.splitn(2, '/');
    let owner = parts.next()
        .ok_or_else(|| AppError::Other("Invalid repo full_name".into()))?;
    let repo = parts.next()
        .ok_or_else(|| AppError::Other("Invalid repo full_name (no slash)".into()))?;
    Ok((owner, repo))
}

fn sort_tree(entries: &mut Vec<RemoteTreeEntry>) {
    entries.sort_by(|a, b| {
        let ad = a.entry_type == "dir";
        let bd = b.entry_type == "dir";
        bd.cmp(&ad).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Percent-encode `/` for use in URL path segments (GitLab project IDs).
fn encode_slash(s: &str) -> String {
    s.replace('/', "%2F")
}

/// Percent-encode a full file path for use in GitLab file API.
fn encode_path_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '/' => out.push_str("%2F"),
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            '&' => out.push_str("%26"),
            '+' => out.push_str("%2B"),
            c   => out.push(c),
        }
    }
    out
}

fn mime_for_path(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png"           => "image/png",
        "jpg" | "jpeg"  => "image/jpeg",
        "gif"           => "image/gif",
        "svg"           => "image/svg+xml",
        "webp"          => "image/webp",
        "ico"           => "image/x-icon",
        "bmp"           => "image/bmp",
        "avif"          => "image/avif",
        "rs"            => "text/x-rust",
        "ts" | "tsx"    => "text/typescript",
        "js" | "jsx"    => "text/javascript",
        "svelte"        => "text/plain",
        "vue"           => "text/plain",
        "py"            => "text/x-python",
        "go"            => "text/x-go",
        "java"          => "text/x-java",
        "kt" | "kts"    => "text/x-kotlin",
        "c" | "h"       => "text/x-c",
        "cpp" | "hpp"   => "text/x-c++",
        "cs"            => "text/x-csharp",
        "rb"            => "text/x-ruby",
        "php"           => "text/x-php",
        "sh" | "bash" | "zsh" | "fish" => "text/x-sh",
        "html" | "htm"  => "text/html",
        "css" | "scss" | "sass" | "less" => "text/css",
        "json"          => "application/json",
        "xml"           => "text/xml",
        "sql"           => "text/x-sql",
        "md" | "mdx"    => "text/markdown",
        "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "env" | "lock" => "text/plain",
        "txt" | "log" | "gitignore" | "gitattributes" | "editorconfig"    => "text/plain",
        "pdf"           => "application/pdf",
        "wasm"          => "application/wasm",
        _               => "application/octet-stream",
    }.to_string()
}
