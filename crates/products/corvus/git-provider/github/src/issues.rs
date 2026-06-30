//! GitHub repo-native issues — STUB. Every method returns `Unsupported`.

use corvus_git_provider_api::prelude::*;

use crate::http::GithubHttp;

pub(crate) async fn list_repo_issues(
    _http:   &GithubHttp,
    _repo:   &RepoRef,
    _filter: IssueFilter,
) -> Result<Vec<RepoIssue>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_repo_issues".into() })
}

pub(crate) async fn get_repo_issue(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _id:   &str,
) -> Result<RepoIssue, ProviderError> {
    Err(ProviderError::Unsupported { feature: "get_repo_issue".into() })
}

pub(crate) async fn create_repo_issue(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _req:  IssueCreateRequest,
) -> Result<RepoIssue, ProviderError> {
    Err(ProviderError::Unsupported { feature: "create_repo_issue".into() })
}

pub(crate) async fn comment_repo_issue(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _id:   &str,
    _body: &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "comment_repo_issue".into() })
}

pub(crate) async fn close_repo_issue(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _id:   &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "close_repo_issue".into() })
}
