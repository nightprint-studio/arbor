//! `impl IssueTracker for JiraTracker` — the Jira REST operations, plus two
//! Jira-specific inherent methods (`download_attachment`, `current_user`).

use async_trait::async_trait;
use serde_json::{json, Value};

use corvus_issue_tracker_api::prelude::{
    AuthField, AuthMethod, AuthMethodKind, AuthStatus, FieldWidget, Issue, IssueComment,
    IssueFilterOptions, IssueFilters, IssueLabel, IssueStatus, IssueTeam, IssueTracker,
    IssueTrackerError, IssueUser, NewIssue, ProviderDescriptor, Result,
};

use crate::{map, JiraTracker};

impl JiraTracker {
    /// The authenticated user (`/myself`) — used by the connect flow to validate
    /// freshly-stored credentials.
    pub async fn current_user(&self) -> Result<IssueUser> {
        let base = self.resolve_session().await?.base_url;
        let data = self.get_abs(&format!("{base}/myself")).await?;
        Ok(map::map_user(&data))
    }

    /// Stream an attachment from a Jira `content` URL to `dest_path`. Only the
    /// configured Jira host is accepted (no open proxy). Async end-to-end.
    pub async fn download_attachment(
        &self,
        content_url: &str,
        dest_path: &std::path::Path,
    ) -> Result<u64> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let base = self.resolve_session().await?.base_url;
        let cfg_host = reqwest::Url::parse(&base)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_lowercase()))
            .ok_or_else(|| IssueTrackerError::Api("Jira base URL has no host".into()))?;
        let url_host = reqwest::Url::parse(content_url)
            .map_err(|e| IssueTrackerError::Api(format!("Invalid attachment URL: {e}")))?
            .host_str()
            .map(|s| s.to_lowercase())
            .ok_or_else(|| IssueTrackerError::Api("Attachment URL has no host".into()))?;
        if url_host != cfg_host {
            return Err(IssueTrackerError::Api(format!(
                "Attachment host '{url_host}' does not match Jira host '{cfg_host}' — refusing to download"
            )));
        }

        let resp = self
            .send(|s| {
                self.http
                    .get(content_url)
                    .header("Authorization", &s.auth_header)
                    .header("X-Atlassian-Token", "no-check")
                    .header("Accept", "*/*")
            })
            .await?;

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(IssueTrackerError::Auth("Invalid or expired Jira credentials".into()));
        }
        if !status.is_success() {
            return Err(IssueTrackerError::Api(format!("Attachment HTTP {status}")));
        }

        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| IssueTrackerError::Api(format!("Cannot create dir: {e}")))?;
            }
        }
        let mut file = tokio::fs::File::create(dest_path)
            .await
            .map_err(|e| IssueTrackerError::Api(format!("Cannot create file: {e}")))?;

        let mut stream = resp.bytes_stream();
        let mut total: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| IssueTrackerError::Api(format!("Network read failed: {e}")))?;
            file.write_all(&bytes)
                .await
                .map_err(|e| IssueTrackerError::Api(format!("Disk write failed: {e}")))?;
            total += bytes.len() as u64;
        }
        file.flush().await.ok();
        Ok(total)
    }

    /// Fetch all Jira projects → `IssueTeam`s. `/project/search` (Cloud + DC ≥8.4,
    /// paginated) first, falling back to `/project` for older Server.
    async fn fetch_all_projects(&self) -> Vec<IssueTeam> {
        let page_size = 50usize;
        let mut all: Vec<IssueTeam> = Vec::new();
        let mut start = 0usize;
        let mut search_endpoint_ok = true;

        loop {
            let path = format!("/project/search?maxResults={page_size}&startAt={start}&expand=");
            let raw = match self.get(&path).await {
                Ok(v) => v,
                Err(_) => {
                    search_endpoint_ok = false;
                    break;
                }
            };
            if raw.is_array() {
                for p in raw.as_array().unwrap() {
                    if !map::s(&p["key"]).is_empty() {
                        all.push(IssueTeam { id: map::s(&p["key"]), name: map::s(&p["name"]), key: map::s(&p["key"]) });
                    }
                }
                break;
            }
            let values = raw["values"].as_array().cloned().unwrap_or_default();
            let is_last = raw["isLast"].as_bool().unwrap_or(true);
            for p in &values {
                if !map::s(&p["key"]).is_empty() {
                    all.push(IssueTeam { id: map::s(&p["key"]), name: map::s(&p["name"]), key: map::s(&p["key"]) });
                }
            }
            if is_last || values.is_empty() {
                break;
            }
            start += page_size;
        }

        if !search_endpoint_ok {
            if let Ok(raw) = self.get("/project?maxResults=500").await {
                let arr: Vec<Value> = if raw.is_array() {
                    raw.as_array().cloned().unwrap_or_default()
                } else {
                    raw["values"].as_array().cloned().unwrap_or_default()
                };
                for p in &arr {
                    if !map::s(&p["key"]).is_empty() {
                        all.push(IssueTeam { id: map::s(&p["key"]), name: map::s(&p["name"]), key: map::s(&p["key"]) });
                    }
                }
            }
        }

        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    }

    /// Active sprints across scrum boards (agile API, best-effort).
    async fn fetch_active_sprints(&self) -> Result<Vec<corvus_issue_tracker_api::prelude::IssueCycle>> {
        use corvus_issue_tracker_api::prelude::IssueCycle;

        let base = self.resolve_session().await?.base_url;
        let agile = map::agile_url_from(&base);
        let boards = self.get_abs(&format!("{agile}/board?type=scrum&maxResults=5")).await?;

        let mut cycles = Vec::new();
        for board in boards["values"].as_array().unwrap_or(&vec![]) {
            let board_id = board["id"].as_u64().unwrap_or(0);
            if board_id == 0 {
                continue;
            }
            let sprints_url = format!("{agile}/board/{board_id}/sprint?state=active&maxResults=10");
            if let Ok(sprints) = self.get_abs(&sprints_url).await {
                for sprint in sprints["values"].as_array().unwrap_or(&vec![]) {
                    let id = sprint["id"].as_u64().map(|n| n.to_string()).unwrap_or_default();
                    if id.is_empty() {
                        continue;
                    }
                    cycles.push(IssueCycle {
                        id,
                        name: map::s(&sprint["name"]),
                        number: sprint["id"].as_f64().unwrap_or(0.0),
                    });
                }
            }
        }
        Ok(cycles)
    }

    /// Build the JQL search query from the cross-provider filters.
    fn build_jql(filters: &IssueFilters) -> std::result::Result<String, Vec<Issue>> {
        let mut jql_parts: Vec<String> = Vec::new();

        if let Some(ref q) = filters.query {
            if !q.is_empty() {
                if let Some(rest) = q.strip_prefix('#') {
                    let trimmed = rest.trim();
                    if !trimmed.is_empty() {
                        if map::is_jira_key(trimmed) {
                            let escaped = trimmed.replace('"', "\\\"");
                            jql_parts.push(format!("key = \"{escaped}\""));
                        } else {
                            // Partial key — no valid JQL clause; empty result.
                            return Err(Vec::new());
                        }
                    }
                } else if let Some(rest) = q.strip_prefix('~') {
                    let trimmed = rest.trim();
                    if !trimmed.is_empty() {
                        let escaped = trimmed.replace('"', "\\\"");
                        jql_parts.push(format!("text ~ \"{escaped}\""));
                    }
                } else {
                    let escaped = q.replace('"', "\\\"");
                    if map::is_jira_key(q) {
                        jql_parts.push(format!("(key = \"{escaped}\" OR text ~ \"{escaped}\")"));
                    } else {
                        jql_parts.push(format!("text ~ \"{escaped}\""));
                    }
                }
            }
        }
        if !filters.status_ids.is_empty() {
            let ids = filters.status_ids.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(",");
            jql_parts.push(format!("status in ({ids})"));
        }
        if !filters.issue_type_ids.is_empty() {
            let ids = filters.issue_type_ids.iter().map(|id| format!("\"{id}\"")).collect::<Vec<_>>().join(",");
            jql_parts.push(format!("issuetype in ({ids})"));
        }
        if !filters.label_ids.is_empty() {
            let labels = filters.label_ids.iter().map(|l| format!("\"{l}\"")).collect::<Vec<_>>().join(",");
            jql_parts.push(format!("labels in ({labels})"));
        }
        if let Some(ref tid) = filters.team_id {
            jql_parts.push(format!("project = \"{tid}\""));
        }
        if let Some(ref mid) = filters.milestone_id {
            jql_parts.push(format!("fixVersion = \"{mid}\""));
        }
        if let Some(ref cid) = filters.cycle_id {
            jql_parts.push(format!("sprint = {cid}"));
        }
        if filters.assignee_me {
            jql_parts.push("assignee = currentUser()".to_string());
        }

        Ok(if jql_parts.is_empty() {
            "ORDER BY updated DESC".to_string()
        } else {
            format!("{} ORDER BY updated DESC", jql_parts.join(" AND "))
        })
    }
}

#[async_trait]
impl IssueTracker for JiraTracker {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "jira".into(),
            display_name: "Jira".into(),
            description: Some("Atlassian issue tracker — API Token & OAuth".into()),
            icon: "jira".into(),
            auth_methods: vec![
                AuthMethod {
                    id: "basic".into(),
                    label: "API Token (recommended)".into(),
                    kind: AuthMethodKind::Fields {
                        fields: vec![
                            AuthField {
                                key: "domain".into(),
                                label: "Site".into(),
                                widget: FieldWidget::Url,
                                required: true,
                                placeholder: Some("mycompany.atlassian.net".into()),
                            },
                            AuthField {
                                key: "email".into(),
                                label: "Email".into(),
                                widget: FieldWidget::Text,
                                required: false,
                                placeholder: Some("you@company.com".into()),
                            },
                            AuthField {
                                key: "api_token".into(),
                                label: "API token".into(),
                                widget: FieldWidget::Secret,
                                required: true,
                                placeholder: None,
                            },
                        ],
                    },
                },
                AuthMethod {
                    id: "oauth".into(),
                    label: "OAuth 2.0 (requires Atlassian app)".into(),
                    kind: AuthMethodKind::OAuth,
                },
            ],
        }
    }

    async fn auth_status(&self) -> Result<AuthStatus> {
        let unauth = || AuthStatus { authenticated: false, user: None, domain: None, auth_method: None };
        let session = match self.resolve_session().await {
            Ok(s) => s,
            Err(_) => return Ok(unauth()),
        };
        match self.get_abs(&format!("{}/myself", session.base_url)).await {
            Ok(data) => Ok(AuthStatus {
                authenticated: true,
                user: Some(map::map_user(&data)),
                domain: session.web_base.clone(),
                auth_method: None,
            }),
            Err(_) => Ok(unauth()),
        }
    }

    async fn search_issues(&self, filters: IssueFilters) -> Result<Vec<Issue>> {
        let jql = match Self::build_jql(&filters) {
            Ok(jql) => jql,
            Err(empty) => return Ok(empty),
        };
        let session = self.resolve_session().await?;
        let domain = session.web_base.as_deref();
        let max_results = filters.limit.unwrap_or(50).min(100);
        let fields_str = map::ISSUE_FIELDS.join(",");
        let url = format!(
            "{}/search?jql={}&fields={fields_str}&maxResults={max_results}&startAt=0",
            session.base_url,
            map::jql_encode(&jql),
        );
        let data = self.get_abs(&url).await?;
        Ok(data["issues"]
            .as_array()
            .map(|a| a.iter().map(|v| map::map_issue(v, domain)).collect())
            .unwrap_or_default())
    }

    async fn get_issue(&self, id: &str) -> Result<Issue> {
        let session = self.resolve_session().await?;
        let fields_str = map::ISSUE_FIELDS_DETAIL.join(",");
        let data = self
            .get_abs(&format!("{}/issue/{id}?fields={fields_str}&expand=renderedFields", session.base_url))
            .await?;
        Ok(map::map_issue(&data, session.web_base.as_deref()))
    }

    async fn lookup_by_identifier(&self, identifier: &str) -> Result<Option<Issue>> {
        let id = identifier.trim();
        if id.is_empty() {
            return Ok(None);
        }
        // Jira's get_issue raises on missing keys; swallow → Ok(None) so a caller
        // can render the bare key without dropping the whole operation.
        match self.get_issue(id).await {
            Ok(issue) => Ok(Some(issue)),
            Err(_) => Ok(None),
        }
    }

    async fn get_filter_options(&self) -> Result<IssueFilterOptions> {
        let base = self.resolve_session().await?.base_url;
        let myself_url = format!("{base}/myself");

        let (teams, (statuses_res, labels_res, issue_types_res, me_res)) = tokio::join!(
            self.fetch_all_projects(),
            async {
                tokio::join!(
                    self.get("/status"),
                    self.get("/label?maxResults=200"),
                    self.get("/issuetype"),
                    self.get_abs(&myself_url),
                )
            },
        );

        let statuses: Vec<IssueStatus> = statuses_res
            .unwrap_or(json!([]))
            .as_array()
            .map(|a| a.iter().map(map::map_status).collect())
            .unwrap_or_default();

        let labels: Vec<IssueLabel> = labels_res.unwrap_or(json!({}))["values"]
            .as_array()
            .map(|a| a.iter().filter_map(|l| l.as_str()).map(map::label_from).collect())
            .unwrap_or_default();

        let issue_types: Vec<IssueLabel> = {
            let raw = issue_types_res.unwrap_or(json!([]));
            raw.as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|t| !map::s(&t["name"]).is_empty() && t["subtask"].as_bool() != Some(true))
                .map(|t| IssueLabel {
                    id: map::s(&t["name"]),
                    name: map::s(&t["name"]),
                    color: map::issue_type_color(map::s(&t["name"]).as_str()),
                })
                .collect()
        };

        let me = me_res.ok().as_ref().map(map::map_user);
        let cycles = self.fetch_active_sprints().await.unwrap_or_default();

        Ok(IssueFilterOptions {
            teams,
            statuses,
            labels,
            issue_types,
            projects: vec![],
            cycles,
            milestones: vec![],
            me,
        })
    }

    async fn transition_issue(&self, id: &str, status_id: &str) -> Result<Issue> {
        let transitions = self.get(&format!("/issue/{id}/transitions")).await?;
        let transition_id = transitions["transitions"]
            .as_array()
            .and_then(|a| a.iter().find(|t| map::s(&t["to"]["id"]) == status_id || map::s(&t["id"]) == status_id))
            .map(|t| map::s(&t["id"]));

        let tid = transition_id.ok_or_else(|| {
            IssueTrackerError::Api(format!("No available transition to status '{status_id}' for issue {id}"))
        })?;

        self.post(&format!("/issue/{id}/transitions"), &json!({ "transition": { "id": tid } })).await?;
        self.get_issue(id).await
    }

    async fn assign_issue(&self, id: &str, user_id: Option<&str>) -> Result<Issue> {
        let body = match user_id {
            Some(uid) => json!({ "accountId": uid }),
            None => json!({ "accountId": null }),
        };
        self.put(&format!("/issue/{id}/assignee"), &body).await?;
        self.get_issue(id).await
    }

    async fn add_comment(&self, issue_id: &str, body: &str) -> Result<IssueComment> {
        let resp = self
            .post(
                &format!("/issue/{issue_id}/comment?expand=renderedBody"),
                &json!({ "body": map::text_to_adf(body) }),
            )
            .await?;

        let (comment_body, body_format) = {
            use corvus_issue_tracker_api::prelude::BodyFormat;
            let rendered = resp["renderedBody"].as_str().map(|s| s.trim()).filter(|s| !s.is_empty());
            if let Some(html) = rendered {
                (map::sanitize_html(html), BodyFormat::Html)
            } else if resp["body"].is_string() {
                (map::s(&resp["body"]), BodyFormat::Markdown)
            } else {
                (map::adf_to_markdown(&resp["body"]).trim().to_string(), BodyFormat::Markdown)
            }
        };

        Ok(IssueComment {
            id: map::s(&resp["id"]),
            body: comment_body,
            body_format,
            created_at: map::s(&resp["created"]),
            user: if resp["author"].is_object() { Some(map::map_user(&resp["author"])) } else { None },
        })
    }

    async fn create_issue(&self, req: NewIssue) -> Result<Issue> {
        let team_id = req
            .team_id
            .ok_or_else(|| IssueTrackerError::Api("Jira requires a project to create an issue".into()))?;

        let mut fields = serde_json::Map::new();
        fields.insert("project".into(), json!({ "key": team_id }));
        fields.insert("summary".into(), json!(req.title));
        fields.insert("issuetype".into(), json!({ "name": req.issue_type.as_deref().unwrap_or("Task") }));

        if let Some(desc) = req.description.as_deref() {
            if !desc.trim().is_empty() {
                fields.insert("description".into(), map::text_to_adf(desc));
            }
        }
        if let Some(aid) = req.assignee_id.as_deref() {
            fields.insert("assignee".into(), json!({ "accountId": aid }));
        }
        if !req.label_ids.is_empty() {
            fields.insert("labels".into(), json!(req.label_ids));
        }
        if let Some(p) = req.priority {
            let priority_name = match p {
                1 => "Highest",
                2 => "High",
                3 => "Medium",
                4 => "Low",
                _ => "Medium",
            };
            fields.insert("priority".into(), json!({ "name": priority_name }));
        }
        if let Some(mid) = req.milestone_id.as_deref() {
            fields.insert("fixVersions".into(), json!([{ "id": mid }]));
        }
        if let Some(dd) = req.due_date.as_deref() {
            fields.insert("duedate".into(), json!(dd));
        }
        if let Some(est) = req.estimate {
            fields.insert("customfield_10016".into(), json!(est));
        }

        let resp = self.post("/issue", &json!({ "fields": Value::Object(fields) })).await?;
        let key = map::s(&resp["key"]);

        if let Some(sid) = req.status_id.as_deref() {
            if let Err(e) = self.transition_issue(&key, sid).await {
                tracing::warn!("jira: post-create transition failed: {e}");
            }
        }
        self.get_issue(&key).await
    }

    async fn fetch_image_bytes(&self, url: &str) -> Result<(Vec<u8>, Option<String>)> {
        let session = self.resolve_session().await?;
        let base = session.base_url.clone();

        let abs = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            let b = base.trim_end_matches('/');
            if url.starts_with('/') { format!("{b}{url}") } else { format!("{b}/{url}") }
        };

        let cfg_host = reqwest::Url::parse(&base).ok().and_then(|u| u.host_str().map(|s| s.to_lowercase()));
        let url_host = reqwest::Url::parse(&abs).ok().and_then(|u| u.host_str().map(|s| s.to_lowercase()));
        let same_host = matches!((&cfg_host, &url_host), (Some(a), Some(b)) if a == b);

        let resp = if same_host {
            self.send(|s| {
                self.http
                    .get(&abs)
                    .header("Authorization", &s.auth_header)
                    .header("X-Atlassian-Token", "no-check")
                    .header("Accept", "*/*")
            })
            .await?
        } else {
            self.http
                .get(&abs)
                .header("Accept", "*/*")
                .send()
                .await
                .map_err(|e| IssueTrackerError::Network(format!("Image request failed: {e}")))?
        };

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(IssueTrackerError::Auth("Invalid or expired Jira credentials".into()));
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
