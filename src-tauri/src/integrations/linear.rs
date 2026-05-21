use serde_json::{json, Value};

use crate::error::{AppError, Result};
use crate::integrations::{
    Issue, IssueComment, IssueCycle, IssueFilterOptions, IssueFilters, IssueLabel,
    IssueProject, IssueStatus, IssueTeam, IssueUser, LinearAuthStatus,
};

const LINEAR_GQL: &str = "https://api.linear.app/graphql";
const KEYRING_HOST: &str = "linear.app";
const KEYRING_USER: &str = "api-key";

// ---------------------------------------------------------------------------
// Token storage (OS keyring via credential_store)
// ---------------------------------------------------------------------------

pub fn save_token(token: &str) -> Result<()> {
    crate::auth::credential_store::save(KEYRING_HOST, KEYRING_USER, token)
}

pub fn get_token() -> Result<Option<String>> {
    crate::auth::credential_store::get(KEYRING_HOST, KEYRING_USER)
}

pub fn delete_token() -> Result<()> {
    crate::auth::credential_store::delete(KEYRING_HOST, KEYRING_USER)
}

// ---------------------------------------------------------------------------
// HTTP / GraphQL helper
// ---------------------------------------------------------------------------

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("Arbor-Git-GUI/1.0")
        .build()
        .unwrap_or_default()
}

/// Raw GraphQL call with an explicit token. Used by the validation flow before
/// the token is saved. All other callers go through `gql_authed`, which resolves
/// the stored token and transparently refreshes it on 401.
async fn gql(token: &str, query: &str, variables: Value) -> Result<Value> {
    let resp = http_client()
        .post(LINEAR_GQL)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Linear request failed: {e}")))?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::AuthFailed("Invalid or expired Linear API key".into()));
    }
    if !status.is_success() {
        return Err(AppError::Other(format!("Linear API error: HTTP {status}")));
    }

    let parsed: Value = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("Linear JSON parse error: {e}")))?;

    if let Some(errors) = parsed.get("errors") {
        let msg = errors[0]["message"].as_str().unwrap_or("GraphQL error");
        return Err(AppError::Other(format!("Linear: {msg}")));
    }

    Ok(parsed["data"].clone())
}

/// GraphQL call against the stored Linear token, with automatic OAuth refresh
/// on `401 Unauthorized`. The Linear OAuth access token has a finite lifetime
/// (typically 24h); the refresh token survives until the user revokes the app.
/// Without this retry, every expiry would force the user to redo the OAuth dance.
async fn gql_authed(query: &str, variables: Value) -> Result<Value> {
    let token = get_token()?
        .ok_or_else(|| AppError::AuthFailed("Not connected to Linear".into()))?;

    match gql(&token, query, variables.clone()).await {
        Err(AppError::AuthFailed(_)) => {
            // Try the silent refresh — only succeeds when an OAuth refresh token
            // is stored (PAT users have nothing to refresh).
            let refreshed = crate::auth::oauth_linear::try_refresh()
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("Linear token refresh error: {e}");
                    false
                });
            if !refreshed {
                return Err(AppError::AuthFailed("Invalid or expired Linear API key".into()));
            }
            let new_token = get_token()?
                .ok_or_else(|| AppError::AuthFailed("Linear token disappeared after refresh".into()))?;
            gql(&new_token, query, variables).await
        }
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Value → struct mapping helpers
// ---------------------------------------------------------------------------

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn opt_s(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
}

fn map_user(v: &Value) -> IssueUser {
    IssueUser {
        id:           s(&v["id"]),
        name:         s(&v["name"]),
        display_name: s(&v["displayName"]),
        avatar_url:   opt_s(&v["avatarUrl"]),
        email:        opt_s(&v["email"]),
    }
}

fn map_status(v: &Value) -> IssueStatus {
    IssueStatus {
        id:          s(&v["id"]),
        name:        s(&v["name"]),
        color:       s(&v["color"]),
        status_type: s(&v["type"]),
    }
}

fn map_label(v: &Value) -> IssueLabel {
    IssueLabel {
        id:    s(&v["id"]),
        name:  s(&v["name"]),
        color: s(&v["color"]),
    }
}

fn map_team(v: &Value) -> IssueTeam {
    IssueTeam {
        id:   s(&v["id"]),
        name: s(&v["name"]),
        key:  s(&v["key"]),
    }
}

fn map_project(v: &Value) -> IssueProject {
    IssueProject {
        id:    s(&v["id"]),
        name:  s(&v["name"]),
        color: opt_s(&v["color"]),
    }
}

fn map_cycle(v: &Value) -> IssueCycle {
    IssueCycle {
        id:     s(&v["id"]),
        name:   s(&v["name"]),
        number: v["number"].as_f64().unwrap_or(0.0),
    }
}

fn map_milestone(v: &Value) -> crate::integrations::IssueMilestone {
    crate::integrations::IssueMilestone {
        id:           s(&v["id"]),
        name:         s(&v["name"]),
        target_date:  opt_s(&v["targetDate"]),
        project_id:   opt_s(&v["project"]["id"]),
        project_name: opt_s(&v["project"]["name"]),
    }
}

fn map_comment(v: &Value) -> IssueComment {
    IssueComment {
        id:         s(&v["id"]),
        body:       s(&v["body"]),
        body_format: crate::integrations::BodyFormat::Markdown,
        created_at: s(&v["createdAt"]),
        user: if v["user"].is_object() && !v["user"]["id"].is_null() {
            Some(map_user(&v["user"]))
        } else {
            None
        },
    }
}

fn obj_present(v: &Value) -> bool {
    v.is_object() && !v["id"].is_null() && !v["id"].as_str().unwrap_or("").is_empty()
}

fn map_issue(v: &Value) -> Issue {
    let comments: Vec<IssueComment> = v["comments"]["nodes"]
        .as_array()
        .map(|a| a.iter().map(map_comment).collect())
        .unwrap_or_default();
    let comment_count = comments.len() as u32;

    Issue {
        id:             s(&v["id"]),
        identifier:     s(&v["identifier"]),
        title:          s(&v["title"]),
        description:    opt_s(&v["description"]),
        description_format: crate::integrations::BodyFormat::Markdown,
        status:         map_status(&v["state"]),
        priority:       v["priority"].as_u64().unwrap_or(0) as u32,
        priority_label: s(&v["priorityLabel"]),
        assignee:       if obj_present(&v["assignee"]) { Some(map_user(&v["assignee"])) } else { None },
        labels:         v["labels"]["nodes"].as_array()
                            .map(|a| a.iter().map(map_label).collect())
                            .unwrap_or_default(),
        url:            s(&v["url"]),
        created_at:     s(&v["createdAt"]),
        updated_at:     s(&v["updatedAt"]),
        due_date:       opt_s(&v["dueDate"]),
        estimate:       v["estimate"].as_f64(),
        team:           if obj_present(&v["team"])    { Some(map_team(&v["team"]))       } else { None },
        project:        if obj_present(&v["project"]) { Some(map_project(&v["project"])) } else { None },
        cycle:          if obj_present(&v["cycle"])   { Some(map_cycle(&v["cycle"]))     } else { None },
        comments,
        comment_count,
        attachments: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Shared GraphQL field fragment
// ---------------------------------------------------------------------------

const ISSUE_FIELDS: &str = r#"
  id identifier title description
  state { id name color type }
  priority priorityLabel
  assignee { id name displayName avatarUrl email }
  labels { nodes { id name color } }
  url createdAt updatedAt dueDate estimate
  team { id name key }
  project { id name color }
  cycle { id name number }
  comments { nodes { id body createdAt user { id name displayName avatarUrl } } }
"#;

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

pub async fn validate_and_save_token(token: &str) -> Result<IssueUser> {
    let data = gql(token, "{ viewer { id name displayName avatarUrl email } }", json!({})).await?;
    let user = map_user(&data["viewer"]);
    save_token(token)?;
    Ok(user)
}

pub async fn get_auth_status() -> Result<LinearAuthStatus> {
    if get_token()?.is_none() {
        return Ok(LinearAuthStatus { authenticated: false, user: None });
    }
    match gql_authed("{ viewer { id name displayName avatarUrl email } }", json!({})).await {
        Ok(data) => Ok(LinearAuthStatus {
            authenticated: true,
            user: Some(map_user(&data["viewer"])),
        }),
        Err(_) => Ok(LinearAuthStatus { authenticated: false, user: None }),
    }
}

// ---------------------------------------------------------------------------
// Issue queries
// ---------------------------------------------------------------------------

/// Decompose a full identifier ("ENG-42") into `(team_key, number)`.
/// Returns `None` for plain numbers / free-text queries.
fn parse_full_identifier(q: &str) -> Option<(String, i64)> {
    let dash = q.rfind('-')?;
    let prefix = &q[..dash];
    let num_part = &q[dash + 1..];
    if prefix.is_empty()
        || !prefix.chars().all(|c| c.is_ascii_alphabetic())
        || num_part.is_empty()
        || !num_part.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let n: i64 = num_part.parse().ok()?;
    Some((prefix.to_ascii_uppercase(), n))
}

pub async fn search_issues(filters: IssueFilters) -> Result<Vec<Issue>> {
    let mut filter = serde_json::Map::new();
    if let Some(ref q) = filters.query {
        if !q.is_empty() {
            // Search strategy:
            //   #<key>     → identifier only       (filter by ticket key, no
            //                                      title fallback — same prefix
            //                                      convention as `~` below)
            //   ~<text>    → title only            (escape hatch when the user
            //                                      explicitly wants text search
            //                                      and not an identifier match)
            //   <text>     → title OR identifier   (default — covers both
            //                                      "ENG-42" and free text)
            if let Some(rest) = q.strip_prefix('#') {
                let trimmed = rest.trim();
                if let Some((team_key, num)) = parse_full_identifier(trimmed) {
                    filter.insert("number".into(), json!({ "eq": num }));
                    filter.insert("team".into(),   json!({ "key": { "eq": team_key } }));
                } else if !trimmed.is_empty() {
                    // Bare number or partial key — match by number across all
                    // teams (Linear has no "key contains" filter, but exact
                    // number is the common typing pattern).
                    if let Ok(n) = trimmed.parse::<i64>() {
                        filter.insert("number".into(), json!({ "eq": n }));
                    } else {
                        // Unparseable — yield no results rather than silently
                        // falling back to text search.
                        filter.insert("number".into(), json!({ "eq": -1 }));
                    }
                }
            } else if let Some(rest) = q.strip_prefix('~') {
                let trimmed = rest.trim();
                if !trimmed.is_empty() {
                    filter.insert("title".into(), json!({ "containsIgnoreCase": trimmed }));
                }
            } else if let Some((team_key, num)) = parse_full_identifier(q) {
                // Exact identifier ("ENG-42") plus title fallback so the
                // user gets both the matching ticket and any titles that
                // mention the code.
                filter.insert("or".into(), json!([
                    { "title":  { "containsIgnoreCase": q } },
                    {
                        "number": { "eq": num },
                        "team":   { "key": { "eq": team_key } }
                    }
                ]));
            } else {
                filter.insert("title".into(), json!({ "containsIgnoreCase": q }));
            }
        }
    }
    if !filters.status_ids.is_empty() {
        filter.insert("state".into(), json!({ "id": { "in": filters.status_ids } }));
    }
    if !filters.label_ids.is_empty() {
        filter.insert("labels".into(), json!({ "id": { "in": filters.label_ids } }));
    }
    if let Some(ref tid) = filters.team_id {
        filter.insert("team".into(), json!({ "id": { "eq": tid } }));
    }
    if let Some(ref pid) = filters.project_id {
        filter.insert("project".into(), json!({ "id": { "eq": pid } }));
    }
    if let Some(ref mid) = filters.milestone_id {
        filter.insert("projectMilestone".into(), json!({ "id": { "eq": mid } }));
    }
    if let Some(ref cid) = filters.cycle_id {
        filter.insert("cycle".into(), json!({ "id": { "eq": cid } }));
    }
    if filters.assignee_me {
        filter.insert("assignee".into(), json!({ "isMe": { "eq": true } }));
    }

    let first = filters.limit.unwrap_or(50).min(250);
    let query = format!(
        "query($filter:IssueFilter,$first:Int){{issues(filter:$filter,first:$first,orderBy:updatedAt){{nodes{{{ISSUE_FIELDS}}}}}}}"
    );

    let data = gql_authed(&query, json!({
        "filter": Value::Object(filter),
        "first": first,
    })).await?;

    Ok(data["issues"]["nodes"]
        .as_array()
        .map(|a| a.iter().map(map_issue).collect())
        .unwrap_or_default())
}

pub async fn get_issue(id: &str) -> Result<Issue> {
    let query = format!("query($id:String!){{issue(id:$id){{{ISSUE_FIELDS}}}}}");
    let data = gql_authed(&query, json!({ "id": id })).await?;
    Ok(map_issue(&data["issue"]))
}

pub async fn get_filter_options() -> Result<IssueFilterOptions> {
    let q = r#"{
        viewer { id name displayName avatarUrl email }
        teams(first: 50) { nodes { id name key } }
        workflowStates(first: 250) { nodes { id name color type } }
        issueLabels(first: 250) { nodes { id name color } }
        projects(first: 100) { nodes { id name color } }
        projectMilestones(first: 100) { nodes { id name targetDate project { id name } } }
    }"#;
    let data = gql_authed(q, json!({})).await?;

    Ok(IssueFilterOptions {
        me:         if data["viewer"].is_object() { Some(map_user(&data["viewer"])) } else { None },
        teams:      data["teams"]["nodes"].as_array().map(|a| a.iter().map(map_team).collect()).unwrap_or_default(),
        statuses:   data["workflowStates"]["nodes"].as_array().map(|a| a.iter().map(map_status).collect()).unwrap_or_default(),
        labels:      data["issueLabels"]["nodes"].as_array().map(|a| a.iter().map(map_label).collect()).unwrap_or_default(),
        projects:    data["projects"]["nodes"].as_array().map(|a| a.iter().map(map_project).collect()).unwrap_or_default(),
        milestones:  data["projectMilestones"]["nodes"].as_array().map(|a| a.iter().map(map_milestone).collect()).unwrap_or_default(),
        cycles:      vec![],
        issue_types: vec![],  // Linear doesn't have issue types
    })
}

pub async fn transition_issue(id: &str, status_id: &str) -> Result<Issue> {
    let mutation = format!(
        "mutation($id:String!,$input:IssueUpdateInput!){{issueUpdate(id:$id,input:$input){{success issue{{{ISSUE_FIELDS}}}}}}}"
    );
    let data = gql_authed(&mutation, json!({
        "id": id, "input": { "stateId": status_id },
    })).await?;
    Ok(map_issue(&data["issueUpdate"]["issue"]))
}

pub async fn assign_issue(id: &str, user_id: Option<&str>) -> Result<Issue> {
    let mutation = format!(
        "mutation($id:String!,$input:IssueUpdateInput!){{issueUpdate(id:$id,input:$input){{success issue{{{ISSUE_FIELDS}}}}}}}"
    );
    let assignee_id = user_id.map(|id| json!(id)).unwrap_or(Value::Null);
    let data = gql_authed(&mutation, json!({
        "id": id, "input": { "assigneeId": assignee_id },
    })).await?;
    Ok(map_issue(&data["issueUpdate"]["issue"]))
}

pub async fn add_comment(issue_id: &str, body: &str) -> Result<IssueComment> {
    let mutation = r#"mutation($issueId:String!,$body:String!){commentCreate(input:{issueId:$issueId,body:$body}){success comment{id body createdAt user{id name displayName avatarUrl}}}}"#;
    let data = gql_authed(mutation, json!({ "issueId": issue_id, "body": body })).await?;
    Ok(map_comment(&data["commentCreate"]["comment"]))
}

pub async fn create_issue_req(
    title: &str,
    description: Option<&str>,
    team_id: &str,
    status_id: Option<&str>,
    assignee_id: Option<&str>,
    label_ids: Vec<String>,
    priority: Option<u32>,
    project_id: Option<&str>,
    milestone_id: Option<&str>,
    due_date: Option<&str>,
    estimate: Option<f64>,
) -> Result<Issue> {
    let mutation = format!(
        "mutation($input:IssueCreateInput!){{issueCreate(input:$input){{success issue{{{ISSUE_FIELDS}}}}}}}"
    );
    let mut input = json!({ "title": title, "teamId": team_id });
    if let Some(d) = description { input["description"] = json!(d); }
    if let Some(s) = status_id   { input["stateId"]     = json!(s); }
    if let Some(a) = assignee_id  { input["assigneeId"]          = json!(a); }
    if !label_ids.is_empty()      { input["labelIds"]             = json!(label_ids); }
    if let Some(p) = priority     { input["priority"]             = json!(p); }
    if let Some(p) = project_id   { input["projectId"]            = json!(p); }
    if let Some(m) = milestone_id { input["projectMilestoneId"]   = json!(m); }
    if let Some(d) = due_date     { input["dueDate"]              = json!(d); }
    if let Some(e) = estimate     { input["estimate"]             = json!(e); }
    let data = gql_authed(&mutation, json!({ "input": input })).await?;
    Ok(map_issue(&data["issueCreate"]["issue"]))
}
