//! GitHub MR (Pull Request) operations.
//!
//! Phase 2 transition: every function delegates to the original
//! `crate::git_provider::mr_impl::*` implementation. Phase 5 inlines them and deletes the
//! source.

use crate::git_provider::types::{
    MrInfo, MrId, MrDetail, MrComment, MrFile, MrCreateRequest, MrUpdateRequest,
    MergeOpts, MrConflict, MrFilter, RepoRef, ProviderUser, ProviderKind,
    error::ProviderError,
};

use super::api;

fn token() -> Result<String, ProviderError> {
    api::get_token()
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)
}

fn repo_parts<'a>(repo: &'a RepoRef) -> Result<(&'a str, &'a str), ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name  = repo.name.as_deref().ok_or_else(|| {
        ProviderError::BadRequest("GitHub RepoRef requires name".into())
    })?;
    Ok((owner, name))
}

fn id_parts<'a>(id: &'a MrId) -> Result<(&'a str, &'a str, u64), ProviderError> {
    if !matches!(id.provider, ProviderKind::GitHub) {
        return Err(ProviderError::BadRequest("MrId provider mismatch (expected GitHub)".into()));
    }
    let owner = id.owner_or_path.as_str();
    let name  = id.repo_name.as_deref().ok_or_else(|| {
        ProviderError::BadRequest("GitHub MrId requires repo_name".into())
    })?;
    Ok((owner, name, id.number))
}

fn map_state(s: Option<&str>) -> &'static str {
    match s {
        Some("closed") => "closed",
        Some("merged") => "closed", // GH treats merged as closed; filter client-side
        Some("all")    => "all",
        _              => "open",
    }
}

pub async fn list_mrs(repo: &RepoRef, filter: MrFilter) -> Result<Vec<MrInfo>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let token = token()?;
    let api_state = map_state(filter.state.as_deref());
    let mut prs = crate::git_provider::mr_impl::list_github_prs(owner, name, &token, api_state)
        .await
        .map_err(ProviderError::from)?;
    // GitHub has no native "merged" filter — it returns merged as "closed".
    // When the caller asked for "merged" specifically, drop closed-not-merged.
    if matches!(filter.state.as_deref(), Some("merged")) {
        prs.retain(|p| matches!(p.state, crate::git_provider::mr_impl::MrState::Merged));
    }
    Ok(prs)
}

pub async fn get_mr(id: &MrId) -> Result<MrDetail, ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    let token = token()?;
    crate::git_provider::mr_impl::get_github_pr(owner, name, number, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn create_mr(repo: &RepoRef, req: MrCreateRequest) -> Result<MrInfo, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let token = token()?;
    let (mr, _node_id) = crate::git_provider::mr_impl::create_github_pr(owner, name, &req, &token)
        .await
        .map_err(ProviderError::from)?;
    Ok(mr)
}

pub async fn update_mr(_id: &MrId, _req: MrUpdateRequest) -> Result<MrInfo, ProviderError> {
    Err(ProviderError::Unsupported { feature: "update_mr".into() })
}

pub async fn close_mr(id: &MrId) -> Result<(), ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    let token = token()?;
    crate::git_provider::mr_impl::update_github_pr_state(owner, name, number, "closed", &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn reopen_mr(id: &MrId) -> Result<(), ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    let token = token()?;
    crate::git_provider::mr_impl::update_github_pr_state(owner, name, number, "open", &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn merge_mr(id: &MrId, opts: MergeOpts) -> Result<(), ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    let token = token()?;
    let strategy = opts.strategy.as_deref()
        .map(|s| s.to_lowercase())
        .filter(|s| matches!(s.as_str(), "merge" | "squash" | "rebase"))
        .unwrap_or_else(|| if opts.squash { "squash".into() } else { "merge".into() });
    crate::git_provider::mr_impl::merge_github_pr(owner, name, number, &strategy, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn list_mr_comments(_id: &MrId) -> Result<Vec<MrComment>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_mr_comments (use get_mr)".into() })
}

pub async fn add_mr_comment(id: &MrId, body: &str) -> Result<MrComment, ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    let token = token()?;
    crate::git_provider::mr_impl::add_github_pr_comment(owner, name, number, body, &token)
        .await
        .map_err(ProviderError::from)?;
    // GitHub's add-comment endpoint returns the created comment, but the
    // existing helper discards it — Phase 5 will fix this. For now return a
    // minimal placeholder so the trait surface is honored.
    Ok(MrComment {
        id:         "0".into(),
        author:     crate::git_provider::mr_impl::MrUser {
            login:        "".into(),
            display_name: "".into(),
            avatar_url:   None,
        },
        body:       body.into(),
        created_at: String::new(),
        is_bot:     false,
    })
}

pub async fn list_mr_files(id: &MrId) -> Result<Vec<MrFile>, ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    let token = token()?;
    crate::git_provider::mr_impl::get_github_pr_files(owner, name, number, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn fetch_mr_diff(_id: &MrId) -> Result<String, ProviderError> {
    Err(ProviderError::Unsupported { feature: "fetch_mr_diff (use list_mr_files)".into() })
}

pub async fn check_mr_conflict(_id: &MrId) -> Result<MrConflict, ProviderError> {
    Err(ProviderError::Unsupported { feature: "check_mr_conflict".into() })
}

pub async fn list_mr_reviewers(_id: &MrId) -> Result<Vec<ProviderUser>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_mr_reviewers".into() })
}

pub async fn request_mr_review(_id: &MrId, _user: &str) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "request_mr_review".into() })
}

pub async fn approve_mr(_id: &MrId) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "approve_mr".into() })
}
