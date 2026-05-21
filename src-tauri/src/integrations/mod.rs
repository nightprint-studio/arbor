pub mod linear;
pub mod jira;
pub mod jira_types;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared provider-agnostic types
// ---------------------------------------------------------------------------

/// Format of a free-text body coming from an issue tracker.
///
/// - `Markdown` — Linear (native), Jira fallback when no rendered HTML available.
/// - `Html`     — Jira `renderedFields.*` (already rendered + sanitized server-side).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BodyFormat {
    #[default]
    Markdown,
    Html,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueUser {
    pub id:           String,
    pub name:         String,
    pub display_name: String,
    pub avatar_url:   Option<String>,
    pub email:        Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueStatus {
    pub id:          String,
    pub name:        String,
    pub color:       String,
    /// "backlog" | "unstarted" | "started" | "completed" | "cancelled"
    pub status_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueLabel {
    pub id:    String,
    pub name:  String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTeam {
    pub id:   String,
    pub name: String,
    pub key:  String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueProject {
    pub id:    String,
    pub name:  String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueCycle {
    pub id:     String,
    pub name:   String,
    pub number: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueMilestone {
    pub id:           String,
    pub name:         String,
    pub target_date:  Option<String>,
    pub project_id:   Option<String>,
    pub project_name: Option<String>,
}

/// File attached to an issue. `content_url` is the authenticated download URL
/// (Jira: the `content` field; Linear: the `url` field). The frontend never
/// fetches it directly — it goes through `download_issue_attachment` so the
/// backend can attach the right credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueAttachment {
    pub id:            String,
    pub filename:      String,
    pub mime_type:     Option<String>,
    pub size:          Option<u64>,
    pub content_url:   String,
    pub thumbnail_url: Option<String>,
    pub created_at:    Option<String>,
    pub author:        Option<IssueUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueComment {
    pub id:         String,
    pub body:       String,
    /// How `body` should be rendered by the frontend.
    #[serde(default)]
    pub body_format: BodyFormat,
    pub created_at: String,
    pub user:       Option<IssueUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id:             String,
    /// Human-readable identifier, e.g. "ARB-123"
    pub identifier:     String,
    pub title:          String,
    pub description:    Option<String>,
    /// How `description` should be rendered by the frontend.
    #[serde(default)]
    pub description_format: BodyFormat,
    pub status:         IssueStatus,
    /// 0=No priority, 1=Urgent, 2=High, 3=Medium, 4=Low
    pub priority:       u32,
    pub priority_label: String,
    pub assignee:       Option<IssueUser>,
    pub labels:         Vec<IssueLabel>,
    pub url:            String,
    pub created_at:     String,
    pub updated_at:     String,
    pub due_date:       Option<String>,
    pub estimate:       Option<f64>,
    pub team:           Option<IssueTeam>,
    pub project:        Option<IssueProject>,
    pub cycle:          Option<IssueCycle>,
    pub comments:       Vec<IssueComment>,
    pub comment_count:  u32,
    #[serde(default)]
    pub attachments:    Vec<IssueAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IssueFilters {
    pub query:                    Option<String>,
    #[serde(default)] pub status_ids:       Vec<String>,
    #[serde(default)] pub label_ids:        Vec<String>,
    #[serde(default)] pub issue_type_ids:   Vec<String>,
    pub team_id:      Option<String>,
    pub project_id:   Option<String>,
    pub cycle_id:     Option<String>,
    pub milestone_id: Option<String>,
    #[serde(default)] pub assignee_me:  bool,
    pub limit:        Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueFilterOptions {
    pub teams:       Vec<IssueTeam>,
    pub statuses:    Vec<IssueStatus>,
    pub labels:      Vec<IssueLabel>,
    pub issue_types: Vec<IssueLabel>,
    pub projects:    Vec<IssueProject>,
    pub cycles:      Vec<IssueCycle>,
    pub milestones:  Vec<IssueMilestone>,
    pub me:          Option<IssueUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearAuthStatus {
    pub authenticated: bool,
    pub user:          Option<IssueUser>,
}

/// Resolve the active issue tracker for a repo: per-repo `issue_tracker`
/// (with the legacy `ticket_links.tracker` override) — None if neither is set.
fn tracker_for_repo(repo_path: &str) -> Option<String> {
    let cfg = crate::config::repo_config::load(repo_path).ok()?;
    cfg.ticket_links.as_ref()
        .and_then(|c| c.tracker.clone())
        .or(cfg.issue_tracker)
}

/// Look up a single issue by its human identifier (e.g. `"ENG-42"`,
/// `"PROJ-123"`), routing to the tracker configured for the repo.
///
/// Returns:
///   · `Ok(Some(issue))` — found
///   · `Ok(None)`        — repo has no tracker configured, or no match
///   · `Err(_)`          — network / auth failure on the chosen tracker
///
/// Linear: performs `search_issues(query=identifier, limit=10)` and
/// filters by exact identifier match (Linear's search returns up to N
/// candidates whose number matches across teams). The first exact-match
/// row wins.
/// Jira: hands the key straight to `get_issue` — Jira's REST resolves
/// keys natively.
pub async fn lookup_by_identifier(
    repo_path:  &str,
    identifier: &str,
) -> crate::error::Result<Option<Issue>> {
    let id = identifier.trim();
    if id.is_empty() { return Ok(None); }
    let Some(tracker) = tracker_for_repo(repo_path) else { return Ok(None); };
    match tracker.as_str() {
        "linear" => {
            let candidates = linear::search_issues(IssueFilters {
                query: Some(id.to_string()),
                limit: Some(10),
                ..Default::default()
            }).await?;
            // Linear's number-only match across teams can return multiple
            // hits — pick the one whose human identifier matches verbatim.
            Ok(candidates.into_iter()
                .find(|i| i.identifier.eq_ignore_ascii_case(id)))
        }
        "jira" => {
            // Jira's get_issue raises on missing keys. We swallow that and
            // surface it as Ok(None) so the caller can render the bare key
            // without dropping the whole release-notes generation.
            match jira::get_issue(id).await {
                Ok(issue) => Ok(Some(issue)),
                Err(_)    => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

/// Suggest a git branch name for an issue: "{lower-identifier}-{slugified-title}"
pub fn branch_name_for_issue(issue: &Issue) -> String {
    let id_lower = issue.identifier.to_lowercase();
    let slug: String = issue
        .title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.len() > 40 { slug[..40].to_string() } else { slug };
    let slug = slug.trim_end_matches('-').to_string();
    format!("{id_lower}-{slug}")
}
