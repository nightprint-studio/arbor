//! GitLab Merge Request operations.
//!
//! Phase 3 transition: every function delegates to the original
//! `crate::git_provider::mr_impl::*` implementation. Phase 5 inlines them and deletes the
//! source.

use crate::git_provider::types::{
    MrInfo, MrId, MrDetail, MrComment, MrFile, MrCreateRequest, MrUpdateRequest,
    MergeOpts, MrConflict, MrFilter, RepoRef, ProviderUser, ProviderKind,
    error::ProviderError,
};

use super::api;

fn token(base_url: &str) -> Result<String, ProviderError> {
    api::get_token(base_url)
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)
}

fn project_path<'a>(repo: &'a RepoRef) -> &'a str {
    // GitLab convention: full project path is in `owner_or_path`; `name` unset.
    repo.owner_or_path.as_str()
}

fn id_parts<'a>(id: &'a MrId) -> Result<(&'a str, u64), ProviderError> {
    if !matches!(id.provider, ProviderKind::GitLab) {
        return Err(ProviderError::BadRequest("MrId provider mismatch (expected GitLab)".into()));
    }
    Ok((id.owner_or_path.as_str(), id.number))
}

fn map_state(s: Option<&str>) -> &'static str {
    match s {
        Some("closed") => "closed",
        Some("merged") => "merged",
        Some("all")    => "all",
        _              => "opened",
    }
}

pub async fn list_mrs(
    base_url: &str,
    repo:     &RepoRef,
    filter:   MrFilter,
) -> Result<Vec<MrInfo>, ProviderError> {
    let path  = project_path(repo);
    let token = token(base_url)?;
    let state = map_state(filter.state.as_deref());
    crate::git_provider::mr_impl::list_gitlab_mrs(path, base_url, &token, state)
        .await
        .map_err(ProviderError::from)
}

pub async fn get_mr(base_url: &str, id: &MrId) -> Result<MrDetail, ProviderError> {
    let (path, iid) = id_parts(id)?;
    let token = token(base_url)?;
    crate::git_provider::mr_impl::get_gitlab_mr(path, base_url, iid, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn create_mr(
    base_url: &str,
    repo:     &RepoRef,
    req:      MrCreateRequest,
) -> Result<MrInfo, ProviderError> {
    let path  = project_path(repo);
    let token = token(base_url)?;
    crate::git_provider::mr_impl::create_gitlab_mr(path, base_url, &req, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn update_mr(
    _base_url: &str,
    _id:       &MrId,
    _req:      MrUpdateRequest,
) -> Result<MrInfo, ProviderError> {
    Err(ProviderError::Unsupported { feature: "update_mr".into() })
}

pub async fn close_mr(base_url: &str, id: &MrId) -> Result<(), ProviderError> {
    let (path, iid) = id_parts(id)?;
    let token = token(base_url)?;
    crate::git_provider::mr_impl::update_gitlab_mr_state(path, base_url, iid, "close", &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn reopen_mr(base_url: &str, id: &MrId) -> Result<(), ProviderError> {
    let (path, iid) = id_parts(id)?;
    let token = token(base_url)?;
    crate::git_provider::mr_impl::update_gitlab_mr_state(path, base_url, iid, "reopen", &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn merge_mr(
    base_url: &str,
    id:       &MrId,
    opts:     MergeOpts,
) -> Result<(), ProviderError> {
    let (path, iid) = id_parts(id)?;
    let token = token(base_url)?;
    crate::git_provider::mr_impl::merge_gitlab_mr(path, base_url, iid, opts.squash, opts.delete_branch, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn list_mr_comments(_base_url: &str, _id: &MrId) -> Result<Vec<MrComment>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_mr_comments (use get_mr)".into() })
}

pub async fn add_mr_comment(
    base_url: &str,
    id:       &MrId,
    body:     &str,
) -> Result<MrComment, ProviderError> {
    let (path, iid) = id_parts(id)?;
    let token = token(base_url)?;
    crate::git_provider::mr_impl::add_gitlab_mr_note(path, base_url, iid, body, &token)
        .await
        .map_err(ProviderError::from)?;
    // GitLab's note POST does return the created note, but the existing
    // helper discards it — Phase 5 will fix this. For now return a minimal
    // placeholder so the trait surface is honored.
    Ok(MrComment {
        id:         "0".into(),
        author:     crate::git_provider::mr_impl::MrUser {
            login:        String::new(),
            display_name: String::new(),
            avatar_url:   None,
        },
        body:       body.into(),
        created_at: String::new(),
        is_bot:     false,
    })
}

pub async fn list_mr_files(base_url: &str, id: &MrId) -> Result<Vec<MrFile>, ProviderError> {
    let (path, iid) = id_parts(id)?;
    let token = token(base_url)?;
    crate::git_provider::mr_impl::get_gitlab_mr_files(path, base_url, iid, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn fetch_mr_diff(_base_url: &str, _id: &MrId) -> Result<String, ProviderError> {
    Err(ProviderError::Unsupported { feature: "fetch_mr_diff (use list_mr_files)".into() })
}

pub async fn check_mr_conflict(_base_url: &str, _id: &MrId) -> Result<MrConflict, ProviderError> {
    Err(ProviderError::Unsupported { feature: "check_mr_conflict".into() })
}

pub async fn list_mr_reviewers(_base_url: &str, _id: &MrId) -> Result<Vec<ProviderUser>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_mr_reviewers".into() })
}

pub async fn request_mr_review(
    _base_url: &str,
    _id:       &MrId,
    _user:     &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "request_mr_review".into() })
}

pub async fn approve_mr(_base_url: &str, _id: &MrId) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "approve_mr".into() })
}
