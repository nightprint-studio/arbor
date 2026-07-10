//! GitHub repo CRUD — create, get, list — keyring-free port.
//!
//! Folds the delegate `git_provider/github/repo.rs` (the trait's repo-CRUD
//! surface: create/get/list/search) together with the GitHub REST bodies it
//! reaches in `git_provider/repo_impl.rs` (the parallel-paged user-repo fetch).
//! All HTTP goes through [`GithubHttp`]; error strings are preserved verbatim
//! and mapped through [`classify`], except the user-repo listing which the
//! delegate wrapped wholesale in `ProviderError::Internal` — that behavior is
//! preserved (see `list_user_repos`).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Deserialize;

use corvus_git_provider_api::prelude::*;

use crate::http::{GithubHttp, classify};

// ---------------------------------------------------------------------------
// Shared GitHub repo response struct + mapper (verbatim from repo_impl / repo)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GhRepo {
    id: i64,
    name: String,
    full_name: String,
    description: Option<String>,
    private: bool,
    default_branch: String,
    language: Option<String>,
    stargazers_count: u32,
    updated_at: Option<String>,
    clone_url: String,
    ssh_url: Option<String>,
    html_url: String,
    fork: bool,
    archived: bool,
    size: Option<u64>,
    owner: GhOwner,
}
#[derive(Deserialize)]
struct GhOwner {
    login: String,
}

fn map_gh_repo(r: GhRepo) -> RemoteRepoInfo {
    RemoteRepoInfo {
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

// ---------------------------------------------------------------------------
// list_user_repos — parallel-paged fetch (ported from repo_impl::list_github_repos)
// ---------------------------------------------------------------------------
//
// The delegate ignored `ListReposOpts` and called `repo_impl::list_repos("github")`,
// wrapping any error in `ProviderError::Internal(e.to_string())`. We preserve
// both: opts is ignored, and the inner REST errors (which were `AppError::*`
// strings) flow through `Internal` to keep the message identical.

const GH_REPOS_PER_PAGE: u32 = 100;

fn github_repos_url(page: u32) -> String {
    format!(
        "https://api.github.com/user/repos\
         ?per_page={GH_REPOS_PER_PAGE}&page={page}&sort=updated\
         &affiliation=owner,collaborator,organization_member"
    )
}

/// Parse GitHub's RFC 5988 Link header to find `rel="last"` and extract its
/// page number.  Returns None if no last link is present (single page case).
fn github_last_page(link_header: &str) -> Option<u32> {
    for part in link_header.split(',') {
        let part = part.trim();
        if !part.contains("rel=\"last\"") {
            continue;
        }
        let url_start = part.find('<')?;
        let url_end = part.find('>')?;
        let url = &part[url_start + 1..url_end];
        // Look for `&page=N` or `?page=N`
        let page_idx = url.find("page=")?;
        let after = &url[page_idx + 5..];
        let n_str = after.split(|c: char| !c.is_ascii_digit()).next()?;
        return n_str.parse().ok();
    }
    None
}

/// Fetch a single page of the authenticated user's repos. Returns the parsed
/// page plus the raw `Link` header (for last-page discovery on page 1).
///
/// The original took `(client, token)` and built the request inline. Here we
/// route through `http.send` so the `401`→refresh→retry seam applies; the auth
/// header comes from the session (`Bearer <token>`), all other headers, URL,
/// status check and parse error strings are preserved verbatim.
async fn fetch_github_repos_page(
    http: &GithubHttp,
    page: u32,
) -> Result<(Vec<GhRepo>, Option<String>), ProviderError> {
    let url = github_repos_url(page);
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub repos API {s}: {b}")));
    }

    let link = resp
        .headers()
        .get("link")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let page_repos: Vec<GhRepo> = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub repos parse: {e}")))?;
    Ok((page_repos, link))
}

/// Port of `repo_impl::list_github_repos`. Fires page 1, reads the `Link`
/// header to learn the last page, then fans out the rest concurrently and
/// re-sorts by page so `sort=updated` ordering survives the batch.
async fn list_github_repos(http: &GithubHttp) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    // Missing credentials: the original `get_github_token()?.ok_or(...)` raised
    // `AppError::AuthFailed("No GitHub token")`. With the session seam, the
    // missing-token case surfaces as `ProviderError::Unauthenticated` from
    // `http.send`. `list_user_repos` re-wraps everything in `Internal`, so the
    // outward message stays "No GitHub token"-class only if the token is truly
    // absent — see the note there.
    let (first, link) = fetch_github_repos_page(http, 1).await?;
    let last_page = link.as_deref().and_then(github_last_page).unwrap_or(1);

    let mut repos: Vec<RemoteRepoInfo> = first.into_iter().map(map_gh_repo).collect();

    if last_page <= 1 {
        return Ok(repos);
    }

    // Fetch remaining pages sequentially. The original fanned these out with a
    // `JoinSet`, which required `'static` clones of client+token; the session
    // seam (`&GithubHttp`) is borrowed, so we page in order instead. Result
    // ordering is identical (pages 2..=last_page appended in order), only the
    // wall-clock fetch is serial. (Behavior-equivalent output; perf note below.)
    for page in 2..=last_page {
        let (batch, _) = fetch_github_repos_page(http, page).await?;
        repos.extend(batch.into_iter().map(map_gh_repo));
    }

    Ok(repos)
}

pub(crate) async fn list_user_repos(
    http: &GithubHttp,
    _opts: ListReposOpts,
) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    // The delegate wrapped the whole listing in `ProviderError::Internal`.
    // Preserve that: any inner error (including the `Unauthenticated` the seam
    // now raises for a missing token) is collapsed to `Internal(<message>)`,
    // matching the old `.map_err(|e| ProviderError::Internal(e.to_string()))`.
    list_github_repos(http)
        .await
        .map_err(|e| ProviderError::Internal(e.to_string()))
}

// ---------------------------------------------------------------------------
// browse_tree — repo contents listing (ported from repo_impl::browse_github_tree)
// ---------------------------------------------------------------------------
//
// GET /repos/{owner}/{repo}/contents/{path}?ref={branch}. The original took an
// explicit token and built the request inline; here it routes through
// `http.send` so the 401→refresh→retry seam applies. URL, headers, status check
// and parse error strings are preserved verbatim; the result is sorted with the
// shared `sort_tree`.

/// Raw GitHub `/contents` entry — private to the crate.
#[derive(Deserialize)]
struct GhEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
    size: Option<u64>,
}

pub(crate) async fn browse_tree(
    http: &GithubHttp,
    repo: &RepoRef,
    path: &str,
    branch: &str,
) -> Result<Vec<RemoteTreeEntry>, ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name = repo
        .name
        .as_deref()
        .ok_or_else(|| ProviderError::BadRequest("GitHub RepoRef requires name".into()))?;

    let url = if path.is_empty() {
        format!("https://api.github.com/repos/{owner}/{name}/contents?ref={branch}")
    } else {
        format!("https://api.github.com/repos/{owner}/{name}/contents/{path}?ref={branch}")
    };

    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub contents API {s}: {b}")));
    }

    let mut entries: Vec<RemoteTreeEntry> = resp
        .json::<Vec<GhEntry>>()
        .await
        .map_err(|e| classify(format!("GitHub tree parse: {e}")))?
        .into_iter()
        .map(|e| RemoteTreeEntry {
            name: e.name,
            path: e.path,
            entry_type: match e.entry_type.as_str() {
                "dir" => "dir",
                "symlink" => "symlink",
                "submodule" => "submodule",
                _ => "file",
            }
            .into(),
            size: e.size,
        })
        .collect();

    sort_tree(&mut entries);
    Ok(entries)
}

// ---------------------------------------------------------------------------
// get_file_content — raw file fetch (ported from repo_impl::fetch_github_file)
// ---------------------------------------------------------------------------
//
// Bytes come from `https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}`
// — a DIFFERENT host from the api base. `http.send` doesn't hardcode the host
// (the URL lives in the closure), so we build the absolute raw URL with the same
// `http.client()` and the session's `auth_header` (already `Bearer …` for OAuth,
// preserving the original's bearer-token behavior) plus the `User-Agent`. The
// 401→refresh→retry seam therefore still applies. URL, headers, status check and
// error strings are preserved verbatim; bytes funnel through the shared
// `build_file_content` (which here returns the value directly, so we `Ok`-wrap).

pub(crate) async fn get_file_content(
    http: &GithubHttp,
    repo: &RepoRef,
    path: &str,
    branch: &str,
) -> Result<RemoteFileContent, ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name = repo
        .name
        .as_deref()
        .ok_or_else(|| ProviderError::BadRequest("GitHub RepoRef requires name".into()))?;

    let url = format!("https://raw.githubusercontent.com/{owner}/{name}/{branch}/{path}");
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub raw file {s}: {b}")));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| classify(format!("GitHub file read: {e}")))?;
    let mime = mime_for_path(path);
    Ok(build_file_content(path, bytes.to_vec(), &mime))
}

// ---------------------------------------------------------------------------
// get_repo_file / put_repo_file — raw contents API (settings-sync)
// ---------------------------------------------------------------------------
//
// Both go through the contents API (`/repos/{o}/{r}/contents/{path}`) so the
// same token + 401→refresh→retry seam applies. `get_repo_file` asks for the
// `raw` media type (bytes verbatim, no size cap up to the 1 MB contents-API
// limit — fine for the small settings bundle); a 404 is mapped to `None` so a
// first-ever sync reads cleanly. `put_repo_file` resolves the current blob sha
// (absent → create) then PUTs the base64 content, one commit per file.

pub(crate) async fn get_repo_file(
    http: &GithubHttp,
    repo: &RepoRef,
    path: &str,
    branch: &str,
) -> Result<Option<Vec<u8>>, ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name = repo
        .name
        .as_deref()
        .ok_or_else(|| ProviderError::BadRequest("GitHub RepoRef requires name".into()))?;

    let url = format!("https://api.github.com/repos/{owner}/{name}/contents/{path}?ref={branch}");
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github.raw")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub get file {s}: {b}")));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| classify(format!("GitHub file read: {e}")))?;
    Ok(Some(bytes.to_vec()))
}

/// Metadata slice used only to recover the blob `sha` of an existing file.
#[derive(Deserialize)]
struct GhContentMeta {
    sha: String,
}

/// Resolve the current blob sha of a file, or `None` when it doesn't exist yet.
async fn get_file_sha(
    http: &GithubHttp,
    owner: &str,
    name: &str,
    path: &str,
    branch: &str,
) -> Result<Option<String>, ProviderError> {
    let url = format!("https://api.github.com/repos/{owner}/{name}/contents/{path}?ref={branch}");
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub file meta {s}: {b}")));
    }
    let meta: GhContentMeta = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub file meta parse: {e}")))?;
    Ok(Some(meta.sha))
}

pub(crate) async fn put_repo_file(
    http: &GithubHttp,
    repo: &RepoRef,
    path: &str,
    branch: &str,
    content: &[u8],
    message: &str,
) -> Result<(), ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name = repo
        .name
        .as_deref()
        .ok_or_else(|| ProviderError::BadRequest("GitHub RepoRef requires name".into()))?;

    let sha = get_file_sha(http, owner, name, path, branch).await?;
    let url = format!("https://api.github.com/repos/{owner}/{name}/contents/{path}");
    let mut body = serde_json::json!({
        "message": message,
        "content": BASE64.encode(content),
        "branch":  branch,
    });
    if let Some(sha) = sha {
        body["sha"] = serde_json::Value::String(sha);
    }

    let resp = http
        .send(|s| {
            http.client()
                .put(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub put file {s}: {b}")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// list_org_repos / search_repos — Unsupported in the delegate (verbatim)
// ---------------------------------------------------------------------------

pub(crate) async fn list_org_repos(
    _http: &GithubHttp,
    _org: &str,
    _opts: ListReposOpts,
) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    Err(ProviderError::Unsupported {
        feature: "list_org_repos".into(),
    })
}

pub(crate) async fn search_repos(
    _http: &GithubHttp,
    _query: &str,
) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    Err(ProviderError::Unsupported {
        feature: "search_repos".into(),
    })
}

// ---------------------------------------------------------------------------
// get_repo — single repo fetch (ported from github/repo.rs)
// ---------------------------------------------------------------------------

pub(crate) async fn get_repo(
    http: &GithubHttp,
    owner: &str,
    name: &str,
) -> Result<RemoteRepoInfo, ProviderError> {
    // NOTE: the delegate used the native `ProviderError::Http`/`Network` shapes
    // here (not the `AppError::Other`→`classify` path), so we preserve those
    // verbatim: non-2xx → `Http { status, body }`, JSON parse → `Network` via
    // `From<reqwest::Error>` (the original `?`).
    let url = format!("https://api.github.com/repos/{owner}/{name}");
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::Http { status, body });
    }

    let r: GhRepo = resp.json().await?;
    Ok(map_gh_repo(r))
}

// ---------------------------------------------------------------------------
// create_repo — create then re-fetch (ported from github/repo.rs)
// ---------------------------------------------------------------------------

pub(crate) async fn create_repo(
    http: &GithubHttp,
    req: RepoCreateRequest,
) -> Result<RemoteRepoInfo, ProviderError> {
    let url = match &req.org {
        Some(org) => format!("https://api.github.com/orgs/{org}/repos"),
        None => "https://api.github.com/user/repos".to_string(),
    };

    let private = !matches!(req.visibility, RepoVisibility::Public);
    let body = serde_json::json!({
        "name":        req.name,
        "description": req.description.unwrap_or_default(),
        "private":     private,
        "auto_init":   false,
    });

    let resp = http
        .send(|s| {
            http.client()
                .post(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::Http { status, body });
    }

    let v: serde_json::Value = resp.json().await?;
    let owner = v
        .get("owner")
        .and_then(|o| o.get("login"))
        .and_then(|l| l.as_str())
        .unwrap_or("")
        .to_string();
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    get_repo(http, &owner, &name).await
}
