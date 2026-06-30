//! Jira HTTP plumbing: session resolution, the `401`→refresh→retry round-trip,
//! and the GET/POST/PUT helpers with error mapping (messages preserved verbatim).

use arbor_ipc::prelude::{AuthSession, CredentialError};
use corvus_issue_tracker_api::prelude::{IssueTrackerError, Result};
use serde_json::{json, Value};

use crate::JiraTracker;

impl JiraTracker {
    /// Resolve the current session, mapping a missing credential to the
    /// "Not connected" message the UI expects.
    pub(crate) async fn resolve_session(&self) -> Result<AuthSession> {
        self.session.session(&self.account).await.map_err(|e| match e {
            CredentialError::NotFound(_) => IssueTrackerError::NotConnected("Not connected to Jira".into()),
            other => IssueTrackerError::Auth(other.to_string()),
        })
    }

    /// Send a request built by `build(session)`, refreshing once on a `401`.
    /// Basic/PAT have nothing to refresh — `refresh` errors and the `401`
    /// propagates to the caller's status handling (same as before).
    pub(crate) async fn send<F>(&self, build: F) -> Result<reqwest::Response>
    where
        F: Fn(&AuthSession) -> reqwest::RequestBuilder,
    {
        let session = self.resolve_session().await?;
        let resp = build(&session)
            .send()
            .await
            .map_err(|e| IssueTrackerError::Network(format!("Jira request failed: {e}")))?;

        if resp.status().as_u16() == 401 {
            if let Ok(s2) = self.session.refresh(&self.account).await {
                return build(&s2)
                    .send()
                    .await
                    .map_err(|e| IssueTrackerError::Network(format!("Jira request failed: {e}")));
            }
        }
        Ok(resp)
    }

    /// GET an absolute URL → parsed JSON.
    pub(crate) async fn get_abs(&self, url: &str) -> Result<Value> {
        let resp = self
            .send(|s| {
                self.http
                    .get(url)
                    .header("Authorization", &s.auth_header)
                    .header("Accept", "application/json")
                    .header("X-Atlassian-Token", "no-check")
            })
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(IssueTrackerError::Auth("Invalid or expired Jira credentials".into()));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(IssueTrackerError::Api(format!("Jira API error {status}: {body}")));
        }
        resp.json().await.map_err(|e| IssueTrackerError::Api(format!("Jira JSON parse: {e}")))
    }

    /// GET a path relative to the session base URL → parsed JSON.
    pub(crate) async fn get(&self, path: &str) -> Result<Value> {
        let base = self.resolve_session().await?.base_url;
        self.get_abs(&format!("{base}{path}")).await
    }

    /// POST a JSON body to a path relative to the session base URL.
    pub(crate) async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let base = self.resolve_session().await?.base_url;
        let url = format!("{base}{path}");
        let resp = self
            .send(|s| {
                self.http
                    .post(&url)
                    .header("Authorization", &s.auth_header)
                    .header("Content-Type", "application/json")
                    .header("Accept", "application/json")
                    .header("X-Atlassian-Token", "no-check")
                    .json(body)
            })
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(IssueTrackerError::Auth("Invalid or expired Jira credentials".into()));
        }
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(IssueTrackerError::Api(format!("Jira POST {status}: {body_text}")));
        }
        if status.as_u16() == 204 {
            return Ok(json!({}));
        }
        resp.json().await.map_err(|e| IssueTrackerError::Api(format!("Jira POST response parse: {e}")))
    }

    /// PUT a JSON body to a path relative to the session base URL.
    pub(crate) async fn put(&self, path: &str, body: &Value) -> Result<()> {
        let base = self.resolve_session().await?.base_url;
        let url = format!("{base}{path}");
        let resp = self
            .send(|s| {
                self.http
                    .put(&url)
                    .header("Authorization", &s.auth_header)
                    .header("Content-Type", "application/json")
                    .header("X-Atlassian-Token", "no-check")
                    .json(body)
            })
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(IssueTrackerError::Auth("Invalid or expired Jira credentials".into()));
        }
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(IssueTrackerError::Api(format!("Jira PUT {status}: {body_text}")));
        }
        Ok(())
    }
}
