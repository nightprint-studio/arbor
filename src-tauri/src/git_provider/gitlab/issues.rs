//! GitLab repo-native issues — STUB. Every method returns `Unsupported`.

use crate::git_provider::types::{
    RepoIssue, IssueCreateRequest, IssueFilter, RepoRef, error::ProviderError,
};

pub async fn list_repo_issues(
    _repo:   &RepoRef,
    _filter: IssueFilter,
) -> Result<Vec<RepoIssue>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_repo_issues".into() })
}

pub async fn get_repo_issue(_repo: &RepoRef, _id: &str) -> Result<RepoIssue, ProviderError> {
    Err(ProviderError::Unsupported { feature: "get_repo_issue".into() })
}

pub async fn create_repo_issue(
    _repo: &RepoRef,
    _req:  IssueCreateRequest,
) -> Result<RepoIssue, ProviderError> {
    Err(ProviderError::Unsupported { feature: "create_repo_issue".into() })
}

pub async fn comment_repo_issue(
    _repo: &RepoRef,
    _id:   &str,
    _body: &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "comment_repo_issue".into() })
}

pub async fn close_repo_issue(_repo: &RepoRef, _id: &str) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "close_repo_issue".into() })
}
