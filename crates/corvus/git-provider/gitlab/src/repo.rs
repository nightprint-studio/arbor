//! GitLab project (repo) CRUD — create, get, list — keyring-free port.
//!
//! Folds the delegate `git_provider/gitlab/repo.rs` (the trait's repo-CRUD
//! surface: create/get/list/search) together with the GitLab REST bodies it
//! reaches in `git_provider/repo_impl.rs` (the parallel-paged project listing).
//! All HTTP goes through [`GitlabHttp`]; auth comes from the session
//! (`Bearer <token>` baked into `s.auth_header`), URLs use [`GitlabHttp::base`]
//! in place of the old `base_url` param, and project paths are percent-encoded
//! via [`percent_encode_slash`].
//!
//! NOTE on error shapes: like the delegate, `get_repo`/`create_repo` use the
//! native `ProviderError::Http { status, body }` for non-2xx and let JSON parse
//! `?` flow through `From<reqwest::Error>` (→ `Network`). `list_user_repos`
//! wraps the whole listing in `ProviderError::Internal`, and the inner REST
//! error strings are preserved verbatim through [`classify`].

use serde::Deserialize;

use corvus_git_provider_api::prelude::*;

use crate::http::{GitlabHttp, classify, percent_encode_slash};

// ---------------------------------------------------------------------------
// list_user_repos — parallel-paged project listing (ported from
// repo_impl::list_gitlab_repos). The delegate ignored `ListReposOpts` and
// called `repo_impl::list_repos("gitlab")` against "https://gitlab.com",
// wrapping any error in `ProviderError::Internal(e.to_string())`. Here the
// listing runs against `http.base()` (the injected instance) and the same
// `Internal` wrap is preserved.
// ---------------------------------------------------------------------------

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
struct GlNamespace {
    path: String,
    full_path: Option<String>,
}

fn map_gl_project(r: GlProject) -> RemoteRepoInfo {
    let name = r
        .path_with_namespace
        .rsplitn(2, '/')
        .next()
        .unwrap_or(&r.name)
        .to_string();
    let namespace = r
        .namespace
        .full_path
        .unwrap_or_else(|| r.namespace.path.clone());
    RemoteRepoInfo {
        id: r.id.to_string(),
        name,
        namespace,
        full_name: r.path_with_namespace,
        description: r.description,
        private: r.visibility.as_deref() != Some("public"),
        default_branch: r.default_branch.unwrap_or_else(|| "main".into()),
        language: None,
        stars: 0,
        updated_at: r.last_activity_at.unwrap_or_default(),
        clone_url_https: r.http_url_to_repo,
        clone_url_ssh: r.ssh_url_to_repo,
        web_url: r.web_url,
        provider: "gitlab".into(),
        is_fork: r.forked_from_project.is_some(),
        is_archived: r.archived,
        // statistics=true was dropped — it forced GitLab to compute repo
        // size for every project, which on a 200+ project list was the
        // single biggest contributor to the 30s cold-load.  size_kb is
        // nice-to-have only; the list view doesn't display it.
        size_kb: None,
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

/// Fetch a single page of the authenticated user's projects. Returns the parsed
/// page plus GitLab's `X-Total-Pages` header (for last-page discovery on page 1).
///
/// The original took `(client, token, base_url)` and built the request inline.
/// Here we route through `http.send` so the `401`→refresh→retry seam applies;
/// the auth header comes from the session (`Bearer <token>`), all other
/// headers, URL, status check and parse error strings are preserved verbatim.
async fn fetch_gitlab_repos_page(
    http: &GitlabHttp,
    page: u32,
) -> Result<(Vec<GlProject>, Option<u32>), ProviderError> {
    let url = gitlab_repos_url(http.base(), page);
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
        return Err(classify(format!("GitLab repos API {s}: {b}")));
    }

    let total_pages = resp
        .headers()
        .get("x-total-pages")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok());

    let page_repos: Vec<GlProject> = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitLab repos parse: {e}")))?;
    Ok((page_repos, total_pages))
}

/// Port of `repo_impl::list_gitlab_repos`. Fires page 1, reads `X-Total-Pages`
/// to learn the last page, then pages the rest in order so the
/// `order_by=last_activity_at` ordering survives the batch.
async fn list_gitlab_repos(http: &GitlabHttp) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    // First page tells us total page count via X-Total-Pages.  Note that
    // GitLab can return 0/missing total counts on very large instances —
    // in that case we fall back to sequential paging.
    let (first, total_pages) = fetch_gitlab_repos_page(http, 1).await?;
    let mut repos: Vec<RemoteRepoInfo> = first.into_iter().map(map_gl_project).collect();

    let last_page = match total_pages {
        Some(n) if n > 1 => n,
        Some(_) | None => return Ok(repos),
    };

    // The original fanned pages 2..=last_page out with a `JoinSet`, which
    // required `'static` clones of client+token+base_url; the session seam
    // (`&GitlabHttp`) is borrowed, so we page in order instead. Result
    // ordering is identical (pages 2..=last_page appended in order), only the
    // wall-clock fetch is serial.
    for page in 2..=last_page {
        let (batch, _) = fetch_gitlab_repos_page(http, page).await?;
        repos.extend(batch.into_iter().map(map_gl_project));
    }

    Ok(repos)
}

pub(crate) async fn list_user_repos(
    http: &GitlabHttp,
    _opts: ListReposOpts,
) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    // The delegate wrapped the whole listing in `ProviderError::Internal`.
    // Preserve that: any inner error (including the `Unauthenticated` the seam
    // now raises for a missing token) is collapsed to `Internal(<message>)`,
    // matching the old `.map_err(|e| ProviderError::Internal(e.to_string()))`.
    list_gitlab_repos(http)
        .await
        .map_err(|e| ProviderError::Internal(e.to_string()))
}

// ---------------------------------------------------------------------------
// list_org_repos / search_repos — Unsupported in the delegate (verbatim)
// ---------------------------------------------------------------------------

pub(crate) async fn list_org_repos(
    _http: &GitlabHttp,
    _org: &str,
    _opts: ListReposOpts,
) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    Err(ProviderError::Unsupported {
        feature: "list_org_repos".into(),
    })
}

pub(crate) async fn search_repos(
    _http: &GitlabHttp,
    _query: &str,
) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
    Err(ProviderError::Unsupported {
        feature: "search_repos".into(),
    })
}

// ---------------------------------------------------------------------------
// get_repo — single project fetch (ported from gitlab/repo.rs)
// ---------------------------------------------------------------------------

/// `owner` is treated as the GitLab namespace path; `name` is the project
/// slug. The full project path is `{owner}/{name}` (or just `name` when
/// owner is empty).
pub(crate) async fn get_repo(
    http: &GitlabHttp,
    owner: &str,
    name: &str,
) -> Result<RemoteRepoInfo, ProviderError> {
    // NOTE: the delegate used the native `ProviderError::Http`/`Network` shapes
    // here (not the `AppError::Other`→`classify` path), so we preserve those
    // verbatim: non-2xx → `Http { status, body }`, JSON parse → `Network` via
    // `From<reqwest::Error>` (the original `?`).
    let project_path = if owner.is_empty() {
        name.to_string()
    } else {
        format!("{owner}/{name}")
    };
    let encoded = percent_encode_slash(&project_path);
    let url = format!("{}/api/v4/projects/{encoded}", http.base());
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::Http { status, body });
    }

    #[derive(Deserialize)]
    struct GlProject {
        id: i64,
        name: String,
        path_with_namespace: String,
        description: Option<String>,
        visibility: String,
        default_branch: Option<String>,
        star_count: u32,
        last_activity_at: Option<String>,
        http_url_to_repo: String,
        ssh_url_to_repo: Option<String>,
        web_url: String,
        #[serde(default)]
        forked_from_project: Option<serde_json::Value>,
        archived: bool,
        #[serde(default)]
        statistics: Option<GlStats>,
        namespace: GlNamespace,
    }
    #[derive(Deserialize)]
    struct GlStats {
        repository_size: Option<u64>,
    }
    #[derive(Deserialize)]
    struct GlNamespace {
        full_path: String,
    }

    let p: GlProject = resp.json().await?;
    Ok(RemoteRepoInfo {
        id: p.id.to_string(),
        name: p.name,
        namespace: p.namespace.full_path,
        full_name: p.path_with_namespace,
        description: p.description,
        private: p.visibility != "public",
        default_branch: p.default_branch.unwrap_or_default(),
        language: None,
        stars: p.star_count,
        updated_at: p.last_activity_at.unwrap_or_default(),
        clone_url_https: p.http_url_to_repo,
        clone_url_ssh: p.ssh_url_to_repo,
        web_url: p.web_url,
        provider: "gitlab".into(),
        is_fork: p.forked_from_project.is_some(),
        is_archived: p.archived,
        size_kb: p.statistics.and_then(|s| s.repository_size),
    })
}

// ---------------------------------------------------------------------------
// create_repo — create then re-fetch (ported from gitlab/repo.rs)
// ---------------------------------------------------------------------------

pub(crate) async fn create_repo(
    http: &GitlabHttp,
    req: RepoCreateRequest,
) -> Result<RemoteRepoInfo, ProviderError> {
    let visibility = match req.visibility {
        RepoVisibility::Public => "public",
        RepoVisibility::Internal => "internal",
        RepoVisibility::Private => "private",
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

    let url = format!("{}/api/v4/projects", http.base());
    let resp = http
        .send(|s| {
            http.client()
                .post(&url)
                .header("Authorization", &s.auth_header)
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
    let path = v
        .get("path_with_namespace")
        .and_then(|p| p.as_str())
        .ok_or_else(|| {
            ProviderError::Internal("GitLab create_repo: missing path_with_namespace".into())
        })?
        .to_string();
    let (owner, name) = match path.rsplit_once('/') {
        Some((o, n)) => (o.to_string(), n.to_string()),
        None => (String::new(), path),
    };
    get_repo(http, &owner, &name).await
}

// ---------------------------------------------------------------------------
// browse_tree — paginated repository tree listing (ported from
// repo_impl::browse_gitlab_tree). The project path comes from `RepoRef`
// (`owner_or_path` = full project path; `name` unset). URL, query params,
// headers, status handling, error strings and the `tree`→`dir` mapping are
// preserved byte-for-byte; auth/refresh goes through `GitlabHttp::send` and
// `AppError::Other(msg)` became `classify(msg)`.
// ---------------------------------------------------------------------------

pub(crate) async fn browse_tree(
    http: &GitlabHttp,
    repo: &RepoRef,
    path: &str,
    branch: &str,
) -> Result<Vec<RemoteTreeEntry>, ProviderError> {
    let base = http.base();
    let encoded = percent_encode_slash(repo.owner_or_path.as_str());
    let mut all = Vec::new();
    let mut page = 1u32;

    loop {
        let url = format!(
            "{base}/api/v4/projects/{encoded}/repository/tree\
             ?path={path}&ref={branch}&per_page=100&page={page}"
        );
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
            return Err(classify(format!("GitLab tree API {s}: {b}")));
        }

        #[derive(Deserialize)]
        struct GlEntry {
            name: String,
            path: String,
            #[serde(rename = "type")]
            entry_type: String,
        }

        let batch: Vec<GlEntry> = resp
            .json()
            .await
            .map_err(|e| classify(format!("GitLab tree parse: {e}")))?;
        let done = batch.len() < 100;

        for e in batch {
            all.push(RemoteTreeEntry {
                name: e.name,
                path: e.path,
                entry_type: if e.entry_type == "tree" { "dir" } else { "file" }.into(),
                size: None,
            });
        }
        if done {
            break;
        }
        page += 1;
    }

    sort_tree(&mut all);
    Ok(all)
}

// ---------------------------------------------------------------------------
// get_file_content — raw single-file fetch (ported from
// repo_impl::fetch_gitlab_file). The bytes are funneled through the shared pure
// helpers `mime_for_path` + `build_file_content`. URL, query param, headers,
// status handling and error strings are preserved byte-for-byte; the per-segment
// file-path encoding uses `encode_path_component` (ported verbatim below).
// ---------------------------------------------------------------------------

pub(crate) async fn get_file_content(
    http: &GitlabHttp,
    repo: &RepoRef,
    path: &str,
    branch: &str,
) -> Result<RemoteFileContent, ProviderError> {
    let base = http.base();
    let encoded_proj = percent_encode_slash(repo.owner_or_path.as_str());
    let encoded_file = encode_path_component(path);
    let url = format!(
        "{base}/api/v4/projects/{encoded_proj}/repository/files/{encoded_file}/raw?ref={branch}"
    );
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
        return Err(classify(format!("GitLab raw file {s}: {b}")));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| classify(format!("GitLab file read: {e}")))?;
    let mime = mime_for_path(path);
    Ok(build_file_content(path, bytes.to_vec(), &mime))
}

/// Percent-encode a full file path for use in GitLab's single-segment file API.
/// Ported verbatim from `repo_impl::encode_path_component` — the exact encoded
/// set matters, so do NOT substitute a general-purpose encoder.
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
            c => out.push(c),
        }
    }
    out
}
