//! `impl IssueTracker for LinearTracker` — the Linear GraphQL operations.

use async_trait::async_trait;
use serde_json::{json, Value};

use corvus_issue_tracker_api::prelude::{
    AuthField, AuthMethod, AuthMethodKind, AuthStatus, FieldWidget, Issue, IssueComment,
    IssueFilterOptions, IssueFilters, IssueTracker, IssueTrackerError, NewIssue, ProviderDescriptor,
    Result,
};

use crate::{map, LinearTracker};

#[async_trait]
impl IssueTracker for LinearTracker {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id:           "linear".into(),
            display_name: "Linear".into(),
            icon:         "linear".into(),
            auth_methods: vec![
                AuthMethod {
                    id:    "oauth".into(),
                    label: "Connect with Linear".into(),
                    kind:  AuthMethodKind::OAuth,
                },
                AuthMethod {
                    id:    "pat".into(),
                    label: "API key".into(),
                    kind:  AuthMethodKind::Fields {
                        fields: vec![AuthField {
                            key:         "token".into(),
                            label:       "Personal API key".into(),
                            widget:      FieldWidget::Secret,
                            required:    true,
                            placeholder: Some("lin_api_…".into()),
                        }],
                    },
                },
            ],
        }
    }

    async fn auth_status(&self) -> Result<AuthStatus> {
        let unauth = || AuthStatus { authenticated: false, user: None, domain: None, auth_method: None };
        if self.session.session(&self.account).await.is_err() {
            return Ok(unauth());
        }
        match self.gql_authed("{ viewer { id name displayName avatarUrl email } }", json!({})).await {
            Ok(data) => Ok(AuthStatus {
                authenticated: true,
                user: Some(map::map_user(&data["viewer"])),
                domain: None,
                auth_method: None,
            }),
            Err(_) => Ok(unauth()),
        }
    }

    async fn search_issues(&self, filters: IssueFilters) -> Result<Vec<Issue>> {
        let filter = map::build_search_filter(&filters);
        let first = filters.limit.unwrap_or(50).min(250);
        let query = format!(
            "query($filter:IssueFilter,$first:Int){{issues(filter:$filter,first:$first,orderBy:updatedAt){{nodes{{{}}}}}}}",
            map::ISSUE_FIELDS
        );
        let data = self
            .gql_authed(&query, json!({ "filter": Value::Object(filter), "first": first }))
            .await?;
        Ok(data["issues"]["nodes"]
            .as_array()
            .map(|a| a.iter().map(map::map_issue).collect())
            .unwrap_or_default())
    }

    async fn get_issue(&self, id: &str) -> Result<Issue> {
        let query = format!("query($id:String!){{issue(id:$id){{{}}}}}", map::ISSUE_FIELDS);
        let data = self.gql_authed(&query, json!({ "id": id })).await?;
        Ok(map::map_issue(&data["issue"]))
    }

    async fn lookup_by_identifier(&self, identifier: &str) -> Result<Option<Issue>> {
        let id = identifier.trim();
        if id.is_empty() {
            return Ok(None);
        }
        // Linear's number-only match across teams can return multiple hits —
        // pick the one whose human identifier matches verbatim.
        let candidates = self
            .search_issues(IssueFilters { query: Some(id.to_string()), limit: Some(10), ..Default::default() })
            .await?;
        Ok(candidates.into_iter().find(|i| i.identifier.eq_ignore_ascii_case(id)))
    }

    async fn get_filter_options(&self) -> Result<IssueFilterOptions> {
        let q = r#"{
            viewer { id name displayName avatarUrl email }
            teams(first: 50) { nodes { id name key } }
            workflowStates(first: 250) { nodes { id name color type } }
            issueLabels(first: 250) { nodes { id name color } }
            projects(first: 100) { nodes { id name color } }
            projectMilestones(first: 100) { nodes { id name targetDate project { id name } } }
        }"#;
        let data = self.gql_authed(q, json!({})).await?;
        Ok(IssueFilterOptions {
            me:          if data["viewer"].is_object() { Some(map::map_user(&data["viewer"])) } else { None },
            teams:       data["teams"]["nodes"].as_array().map(|a| a.iter().map(map::map_team).collect()).unwrap_or_default(),
            statuses:    data["workflowStates"]["nodes"].as_array().map(|a| a.iter().map(map::map_status).collect()).unwrap_or_default(),
            labels:      data["issueLabels"]["nodes"].as_array().map(|a| a.iter().map(map::map_label).collect()).unwrap_or_default(),
            projects:    data["projects"]["nodes"].as_array().map(|a| a.iter().map(map::map_project).collect()).unwrap_or_default(),
            milestones:  data["projectMilestones"]["nodes"].as_array().map(|a| a.iter().map(map::map_milestone).collect()).unwrap_or_default(),
            cycles:      vec![],
            issue_types: vec![], // Linear doesn't have issue types
        })
    }

    async fn transition_issue(&self, id: &str, status_id: &str) -> Result<Issue> {
        let mutation = format!(
            "mutation($id:String!,$input:IssueUpdateInput!){{issueUpdate(id:$id,input:$input){{success issue{{{}}}}}}}",
            map::ISSUE_FIELDS
        );
        let data = self.gql_authed(&mutation, json!({ "id": id, "input": { "stateId": status_id } })).await?;
        Ok(map::map_issue(&data["issueUpdate"]["issue"]))
    }

    async fn assign_issue(&self, id: &str, user_id: Option<&str>) -> Result<Issue> {
        let mutation = format!(
            "mutation($id:String!,$input:IssueUpdateInput!){{issueUpdate(id:$id,input:$input){{success issue{{{}}}}}}}",
            map::ISSUE_FIELDS
        );
        let assignee_id = user_id.map(|id| json!(id)).unwrap_or(Value::Null);
        let data = self.gql_authed(&mutation, json!({ "id": id, "input": { "assigneeId": assignee_id } })).await?;
        Ok(map::map_issue(&data["issueUpdate"]["issue"]))
    }

    async fn add_comment(&self, issue_id: &str, body: &str) -> Result<IssueComment> {
        let mutation = r#"mutation($issueId:String!,$body:String!){commentCreate(input:{issueId:$issueId,body:$body}){success comment{id body createdAt user{id name displayName avatarUrl}}}}"#;
        let data = self.gql_authed(mutation, json!({ "issueId": issue_id, "body": body })).await?;
        Ok(map::map_comment(&data["commentCreate"]["comment"]))
    }

    async fn create_issue(&self, req: NewIssue) -> Result<Issue> {
        let team_id = req
            .team_id
            .ok_or_else(|| IssueTrackerError::Api("Linear requires a team to create an issue".into()))?;
        let mutation = format!(
            "mutation($input:IssueCreateInput!){{issueCreate(input:$input){{success issue{{{}}}}}}}",
            map::ISSUE_FIELDS
        );
        let mut input = json!({ "title": req.title, "teamId": team_id });
        if let Some(d) = req.description  { input["description"]        = json!(d); }
        if let Some(s) = req.status_id    { input["stateId"]            = json!(s); }
        if let Some(a) = req.assignee_id  { input["assigneeId"]         = json!(a); }
        if !req.label_ids.is_empty()      { input["labelIds"]           = json!(req.label_ids); }
        if let Some(p) = req.priority     { input["priority"]           = json!(p); }
        if let Some(p) = req.project_id   { input["projectId"]          = json!(p); }
        if let Some(m) = req.milestone_id { input["projectMilestoneId"] = json!(m); }
        if let Some(d) = req.due_date     { input["dueDate"]            = json!(d); }
        if let Some(e) = req.estimate     { input["estimate"]           = json!(e); }
        let data = self.gql_authed(&mutation, json!({ "input": input })).await?;
        Ok(map::map_issue(&data["issueCreate"]["issue"]))
    }

    async fn fetch_image_bytes(&self, url: &str) -> Result<(Vec<u8>, Option<String>)> {
        // Linear uploads live under `*.linear.app` and need the bearer token; any
        // other host (public CDN) is fetched anonymously so the token never
        // leaves Linear's hosts.
        let is_linear = reqwest::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_lowercase()))
            .map(|h| h == "linear.app" || h.ends_with(".linear.app"))
            .unwrap_or(false);

        let mut req = self.http.get(url).header("Accept", "*/*");
        if is_linear {
            if let Ok(session) = self.session.session(&self.account).await {
                req = req.header("Authorization", session.auth_header);
            }
        }

        let resp = req
            .send()
            .await
            .map_err(|e| IssueTrackerError::Network(format!("Image request failed: {e}")))?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(IssueTrackerError::Auth("Invalid or expired Linear API key".into()));
        }
        if !status.is_success() {
            return Err(IssueTrackerError::Api(format!("Image HTTP {status}")));
        }
        let ctype = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let bytes = resp.bytes().await.map_err(|e| IssueTrackerError::Api(format!("Image read: {e}")))?;
        Ok((bytes.to_vec(), ctype))
    }
}
