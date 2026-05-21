//! Jira Cloud REST API v3 integration.
//!
//! Uses the shared provider-agnostic types from `integrations/mod.rs`.
//! Auth config is resolved via `auth::oauth_jira::get_config()`.

use serde_json::{json, Value};

use crate::auth::oauth_jira::{self, JiraConfig};
use crate::error::{AppError, Result};
use crate::integrations::{
    BodyFormat, Issue, IssueAttachment, IssueComment, IssueCycle, IssueFilterOptions,
    IssueFilters, IssueLabel, IssueStatus, IssueTeam, IssueUser, IssueMilestone,
};
use crate::integrations::jira_types::JiraAuthStatus;

// ---------------------------------------------------------------------------
// HTML sanitizer for Jira `renderedFields.*` output
// ---------------------------------------------------------------------------

/// Sanitize HTML coming from Jira's pre-rendered `renderedFields`.
///
/// Jira returns HTML produced from either ADF (Cloud) or wiki markup (Server/DC).
/// We strip scripts/iframes/styles and forbid event handlers; we keep `class`
/// attributes (used by Jira for code-highlight wrappers, panels, table styling)
/// and force safe `rel` on links.
fn sanitize_html(input: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(["class"])
        .link_rel(Some("noopener noreferrer nofollow"))
        .clean(input)
        .to_string()
}

// ---------------------------------------------------------------------------
// HTTP helper
// ---------------------------------------------------------------------------

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Arbor-Git-GUI/1.0")
        // Many Jira Data Center / Server installations use self-signed or
        // internal-CA certificates that aren't in the OS trust store.
        // This is a desktop tool connecting to internal infrastructure.
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default()
}

/// Send a Jira API request built by `make_req(cfg) → RequestBuilder` with
/// automatic OAuth refresh on `401 Unauthorized`.
///
/// The Atlassian OAuth access token expires after roughly an hour, so without
/// this the user has to redo the OAuth dance constantly. Basic Auth / PAT have
/// no refresh token and the 401 propagates as-is.
async fn jira_send_with_refresh<F>(make_req: F) -> Result<reqwest::Response>
where
    F: Fn(&JiraConfig) -> reqwest::RequestBuilder,
{
    let cfg = oauth_jira::get_config()?
        .ok_or_else(|| AppError::AuthFailed("Not connected to Jira".into()))?;

    let resp = make_req(&cfg)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Jira request failed: {e}")))?;

    if resp.status().as_u16() == 401 && cfg.auth_method == "oauth" {
        let refreshed = oauth_jira::try_refresh().await.unwrap_or_else(|e| {
            tracing::warn!("Jira token refresh error: {e}");
            false
        });
        if refreshed {
            // Re-resolve so we pick up the freshly-saved access token.
            if let Some(new_cfg) = oauth_jira::get_config()? {
                return make_req(&new_cfg)
                    .send()
                    .await
                    .map_err(|e| AppError::Other(format!("Jira request failed: {e}")));
            }
        }
    }

    Ok(resp)
}

async fn jira_get(cfg: &JiraConfig, path: &str) -> Result<Value> {
    let url = format!("{}{path}", cfg.base_url);
    api_get_url(cfg, &url).await
}

async fn api_get_url(_cfg: &JiraConfig, url: &str) -> Result<Value> {
    let resp = jira_send_with_refresh(|c| {
        http_client()
            .get(url)
            .header("Authorization", &c.auth_header)
            .header("Accept", "application/json")
            .header("X-Atlassian-Token", "no-check")
    }).await?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::AuthFailed("Invalid or expired Jira credentials".into()));
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("Jira API error {status}: {body}")));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Other(format!("Jira JSON parse: {e}")))
}

async fn jira_post(cfg: &JiraConfig, path: &str, body: &Value) -> Result<Value> {
    let url = format!("{}{path}", cfg.base_url);
    let resp = jira_send_with_refresh(|c| {
        http_client()
            .post(&url)
            .header("Authorization", &c.auth_header)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("X-Atlassian-Token", "no-check")
            .json(body)
    }).await?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::AuthFailed("Invalid or expired Jira credentials".into()));
    }
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("Jira POST {status}: {body_text}")));
    }
    // 204 No Content
    if status.as_u16() == 204 {
        return Ok(json!({}));
    }
    resp.json()
        .await
        .map_err(|e| AppError::Other(format!("Jira POST response parse: {e}")))
}

async fn jira_put(cfg: &JiraConfig, path: &str, body: &Value) -> Result<()> {
    let url = format!("{}{path}", cfg.base_url);
    let resp = jira_send_with_refresh(|c| {
        http_client()
            .put(&url)
            .header("Authorization", &c.auth_header)
            .header("Content-Type", "application/json")
            .header("X-Atlassian-Token", "no-check")
            .json(body)
    }).await?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::AuthFailed("Invalid or expired Jira credentials".into()));
    }
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("Jira PUT {status}: {body_text}")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ADF (Atlassian Document Format) → Markdown
// ---------------------------------------------------------------------------

/// Apply a single ADF inline mark to `text`, returning the Markdown-formatted string.
fn apply_mark(text: &str, mark: &Value) -> String {
    match mark["type"].as_str().unwrap_or("") {
        "strong"    => format!("**{text}**"),
        "em"        => format!("*{text}*"),
        "code"      => format!("`{text}`"),
        "strike"    => format!("~~{text}~~"),
        "link"      => {
            let href = mark["attrs"]["href"].as_str().unwrap_or("#");
            format!("[{text}]({href})")
        }
        // underline, subsup, textColor, etc. → keep plain text
        _ => text.to_string(),
    }
}

/// Render a single inline ADF node to its Markdown representation.
fn render_inline(node: &Value) -> String {
    match node["type"].as_str().unwrap_or("") {
        "text" => {
            let raw = node["text"].as_str().unwrap_or("");
            let marks = node["marks"].as_array();
            if marks.map(|m| m.is_empty()).unwrap_or(true) {
                return raw.to_string();
            }
            let mut result = raw.to_string();
            for mark in marks.unwrap() {
                result = apply_mark(&result, mark);
            }
            result
        }
        "hardBreak"  => "\n".to_string(),
        "mention"    => {
            let name = node["attrs"]["text"].as_str()
                .or_else(|| node["attrs"]["displayName"].as_str())
                .unwrap_or("someone");
            format!("@{name}")
        }
        "emoji"      => {
            // shortName is ":smile:" — strip the colons for cleaner output
            node["attrs"]["shortName"].as_str()
                .map(|s| s.trim_matches(':').to_string())
                .unwrap_or_default()
        }
        "inlineCard" => {
            let url = node["attrs"]["url"].as_str().unwrap_or("#");
            format!("[{url}]({url})")
        }
        "date"       => node["attrs"]["timestamp"].as_str().unwrap_or("").to_string(),
        _ => {
            // Fallback: render any nested children
            node["content"].as_array()
                .map(|a| a.iter().map(render_inline).collect::<String>())
                .unwrap_or_default()
        }
    }
}

/// Render all inline children of a block node into a flat string.
fn inline_children(node: &Value) -> String {
    node["content"].as_array()
        .map(|a| a.iter().map(render_inline).collect::<String>())
        .unwrap_or_default()
}

/// Convert an ADF document tree to Markdown text, preserving all structure:
/// headings, lists (with markers), fenced code blocks, blockquotes,
/// and all inline marks (bold, italic, code, links).
fn adf_to_markdown(node: &Value) -> String {
    adf_block(node, 0)
}

fn adf_block(node: &Value, list_depth: usize) -> String {
    match node["type"].as_str().unwrap_or("") {

        "doc" => node["content"].as_array()
            .map(|a| a.iter().map(|n| adf_block(n, 0)).collect::<String>())
            .unwrap_or_default(),

        "paragraph" => {
            let text = inline_children(node);
            if text.trim().is_empty() { "\n".to_string() } else { format!("{text}\n\n") }
        }

        "heading" => {
            let level = node["attrs"]["level"].as_u64().unwrap_or(2).min(6) as usize;
            let hashes = "#".repeat(level);
            format!("{hashes} {}\n\n", inline_children(node))
        }

        "bulletList" => {
            let indent = "  ".repeat(list_depth);
            let items: String = node["content"].as_array()
                .map(|a| a.iter().map(|item| {
                    let content = list_item_content(item, list_depth + 1);
                    format!("{indent}- {content}")
                }).collect())
                .unwrap_or_default();
            if list_depth == 0 { format!("{items}\n") } else { items }
        }

        "orderedList" => {
            let indent  = "  ".repeat(list_depth);
            let start   = node["attrs"]["order"].as_u64().unwrap_or(1);
            let items: String = node["content"].as_array()
                .map(|a| a.iter().enumerate().map(|(i, item)| {
                    let num     = start + i as u64;
                    let content = list_item_content(item, list_depth + 1);
                    format!("{indent}{num}. {content}")
                }).collect())
                .unwrap_or_default();
            if list_depth == 0 { format!("{items}\n") } else { items }
        }

        "listItem" => list_item_content(node, list_depth),

        "blockquote" => {
            let inner = node["content"].as_array()
                .map(|a| a.iter().map(|n| adf_block(n, 0)).collect::<String>())
                .unwrap_or_default();
            let quoted: String = inner.lines()
                .map(|l| format!("> {l}\n"))
                .collect();
            format!("{quoted}\n")
        }

        "codeBlock" => {
            let lang = node["attrs"]["language"].as_str().unwrap_or("");
            let code: String = node["content"].as_array()
                .map(|a| a.iter().filter_map(|n| n["text"].as_str()).collect())
                .unwrap_or_default();
            format!("```{lang}\n{code}\n```\n\n")
        }

        "rule" => "---\n\n".to_string(),

        "panel" => {
            // Jira info/warning/note/error/success panels → styled blockquote
            let panel_type = node["attrs"]["panelType"].as_str().unwrap_or("info");
            let prefix = match panel_type {
                "warning" => "⚠️ ", "error" => "❌ ",
                "success" => "✅ ", "note"  => "📝 ", _ => "ℹ️ ",
            };
            let inner = node["content"].as_array()
                .map(|a| a.iter().map(|n| adf_block(n, 0)).collect::<String>())
                .unwrap_or_default();
            let quoted: String = inner.lines().enumerate().map(|(i, l)| {
                if i == 0 { format!("> {prefix}{l}\n") } else { format!("> {l}\n") }
            }).collect();
            format!("{quoted}\n")
        }

        "table" => adf_table(node),

        "mediaSingle" | "mediaGroup" | "media" => {
            // Images/attachments: emit a readable placeholder
            let alt = node["content"].as_array()
                .and_then(|a| a.first())
                .and_then(|m| {
                    m["attrs"]["alt"].as_str()
                        .or_else(|| m["attrs"]["type"].as_str())
                })
                .unwrap_or("attachment");
            format!("*[{alt}]*\n\n")
        }

        // Inline nodes appearing at block level (defensive fallback)
        "text" | "hardBreak" | "mention" | "emoji" | "inlineCard" => render_inline(node),

        _ => {
            // Unknown block: recurse into children
            node["content"].as_array()
                .map(|a| a.iter().map(|n| adf_block(n, list_depth)).collect::<String>())
                .unwrap_or_default()
        }
    }
}

/// Render the content of a `listItem` node.
fn list_item_content(item: &Value, depth: usize) -> String {
    let children = item["content"].as_array().cloned().unwrap_or_default();
    let mut out = String::new();
    for (i, child) in children.iter().enumerate() {
        match child["type"].as_str().unwrap_or("") {
            "paragraph" => {
                let text = inline_children(child);
                if i == 0 {
                    // First paragraph: inline with the bullet/number
                    out.push_str(text.trim_end_matches('\n'));
                    out.push('\n');
                } else {
                    out.push_str(&text);
                }
            }
            "bulletList" | "orderedList" => {
                out.push_str(&adf_block(child, depth));
            }
            _ => out.push_str(&adf_block(child, depth)),
        }
    }
    if out.is_empty() { out.push('\n'); }
    out
}

/// Render an ADF table to a (basic) Markdown table.
fn adf_table(node: &Value) -> String {
    let rows = node["content"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() { return String::new(); }

    let mut lines: Vec<String> = Vec::new();
    let mut header_sep_added = false;

    for row in &rows {
        let cells: Vec<String> = row["content"].as_array()
            .map(|cols| cols.iter().map(|cell| {
                let text = cell["content"].as_array()
                    .map(|a| a.iter().map(|n| adf_block(n, 0)).collect::<String>())
                    .unwrap_or_default();
                text.trim().replace('\n', " ")
            }).collect())
            .unwrap_or_default();

        if cells.is_empty() { continue; }
        lines.push(format!("| {} |", cells.join(" | ")));
        if !header_sep_added {
            let sep = cells.iter().map(|_| "---").collect::<Vec<_>>().join(" | ");
            lines.push(format!("| {sep} |"));
            header_sep_added = true;
        }
    }
    if lines.is_empty() { return String::new(); }
    format!("{}\n\n", lines.join("\n"))
}

/// Wrap plain text in a minimal ADF document.
fn text_to_adf(text: &str) -> Value {
    if text.trim().is_empty() {
        return json!({ "type": "doc", "version": 1, "content": [] });
    }
    let content: Vec<Value> = text
        .lines()
        .map(|line| {
            if line.is_empty() {
                json!({ "type": "paragraph", "content": [] })
            } else {
                json!({
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": line }]
                })
            }
        })
        .collect();
    json!({ "type": "doc", "version": 1, "content": content })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Percent-encode a JQL string for use in a URL query parameter.
/// Spaces → %20, unreserved chars pass through, everything else is hex-encoded.
fn jql_encode(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' |
            '-' | '_' | '.' | '~' | '*' | ',' => vec![c as u8],
            ' ' => vec![b'%', b'2', b'0'],
            _ => format!("%{:02X}", c as u32).into_bytes(),
        })
        .map(|b| b as char)
        .collect()
}

// ---------------------------------------------------------------------------
// Type mapping helpers
// ---------------------------------------------------------------------------

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn opt_s(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
}

/// Map a Jira status object to our shared `IssueStatus`.
fn map_status(v: &Value) -> IssueStatus {
    let cat_key   = s(&v["statusCategory"]["key"]);
    let cat_color = s(&v["statusCategory"]["colorName"]);
    let status_type = match cat_key.as_str() {
        "new"            => "unstarted",
        "indeterminate"  => "started",
        "done"           => "completed",
        _                => "unstarted",
    };
    let color = match cat_color.as_str() {
        "blue-grey" => "#6b778c",
        "yellow"    => "#ff991f",
        "green"     => "#36b37e",
        "red"       => "#ff5630",
        "blue"      => "#0052cc",
        _           => "#6b7280",
    };
    IssueStatus {
        id:          s(&v["id"]),
        name:        s(&v["name"]),
        color:       color.to_string(),
        status_type: status_type.to_string(),
    }
}

/// Map a Jira priority name to the 0–4 numeric scale used in our shared type.
fn map_priority(name: &str) -> (u32, String) {
    match name {
        "Highest" => (1, "Urgent".into()),
        "High"    => (2, "High".into()),
        "Medium"  => (3, "Medium".into()),
        "Low"     => (4, "Low".into()),
        "Lowest"  => (4, "Low".into()),
        _         => (0, "No priority".into()),
    }
}

/// Map a Jira user object to `IssueUser`.
///
/// Schema differs between Jira Cloud and Server/DC:
/// - **Cloud**: identifies users by `accountId`; carries `displayName`,
///   `emailAddress`, `avatarUrls`.
/// - **Server/Data Center**: identifies users by `key` (legacy) or `name`
///   (username); `accountId` is absent. `displayName` is usually present but
///   we fall back through `name` → `key` so the renderer never has to print
///   "Unknown" for a user that the API actually returned.
fn map_user(v: &Value) -> IssueUser {
    let avatar = v["avatarUrls"]["48x48"]
        .as_str()
        .or_else(|| v["avatarUrls"]["32x32"].as_str())
        .map(|s| s.to_string());

    // ID: accountId (Cloud) → key (DC legacy) → name (username) → "".
    let id = if !v["accountId"].is_null() {
        s(&v["accountId"])
    } else if !v["key"].is_null() {
        s(&v["key"])
    } else {
        s(&v["name"])
    };

    // Display name: displayName → name → key → "Unknown".
    let display = {
        let d = s(&v["displayName"]);
        if !d.is_empty() { d }
        else {
            let n = s(&v["name"]);
            if !n.is_empty() { n }
            else {
                let k = s(&v["key"]);
                if !k.is_empty() { k } else { "Unknown".to_string() }
            }
        }
    };

    IssueUser {
        id,
        name:         display.clone(),
        display_name: display,
        avatar_url:   avatar,
        email:        opt_s(&v["emailAddress"]),
    }
}

/// Derive a deterministic hex color from a string (for Jira labels).
fn label_color(name: &str) -> String {
    let palette = [
        "#f87171", "#fb923c", "#fbbf24", "#a3e635",
        "#34d399", "#22d3ee", "#818cf8", "#e879f9",
    ];
    let idx = name.bytes().fold(0u64, |acc, b| acc.wrapping_add(b as u64)) as usize;
    palette[idx % palette.len()].to_string()
}

/// Assign a deterministic color to a Jira issue type name.
fn issue_type_color(name: &str) -> String {
    match name {
        "Bug"           => "#ef4444".into(),
        "Story"         => "#22c55e".into(),
        "Task"          => "#3b82f6".into(),
        "Epic"          => "#a855f7".into(),
        "Improvement"   => "#06b6d4".into(),
        "New Feature"   => "#10b981".into(),
        "Technical task"=> "#6366f1".into(),
        _               => label_color(name),
    }
}

/// Build the full browse URL for a Jira issue.
fn issue_url(domain: Option<&str>, key: &str) -> String {
    if let Some(d) = domain {
        format!("https://{d}/browse/{key}")
    } else {
        format!("https://jira.atlassian.net/browse/{key}")
    }
}

/// Fields for search/list — description omitted to keep responses small.
const ISSUE_FIELDS: &[&str] = &[
    "summary", "status", "priority", "assignee", "labels", "issuetype",
    "project", "created", "updated", "duedate",
    "customfield_10016", "customfield_10020", "fixVersions", "components",
];

/// Fields for single-issue detail — includes description and full comment thread.
const ISSUE_FIELDS_DETAIL: &[&str] = &[
    "summary", "description", "status", "priority", "assignee", "labels", "issuetype",
    "project", "created", "updated", "duedate",
    "customfield_10016", "customfield_10020", "fixVersions", "comment", "components",
    "attachment",
];

/// Map a full Jira issue API response to our shared `Issue`.
///
/// When the response was fetched with `expand=renderedFields`, Jira returns
/// the description and each comment body pre-rendered as HTML — for both
/// ADF (Cloud) and wiki markup (Server/DC). We prefer that HTML (sanitized)
/// over our local ADF→Markdown converter, since it covers every Jira construct
/// (panels, mentions, status lozenges, color, tables, code highlighting…).
fn map_issue(v: &Value, domain: Option<&str>) -> Issue {
    let key  = s(&v["key"]);
    let f    = &v["fields"];
    let rf   = &v["renderedFields"];

    let priority_name = s(&f["priority"]["name"]);
    let (priority, priority_label) = map_priority(&priority_name);

    // Labels: Jira labels are plain strings — use name as id.
    let labels: Vec<IssueLabel> = f["labels"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l.as_str())
                .map(|name| IssueLabel {
                    id:    name.to_string(),
                    name:  name.to_string(),
                    color: label_color(name),
                })
                .collect()
        })
        .unwrap_or_default();

    // Description: prefer Jira's pre-rendered HTML (`renderedFields.description`,
    // requires `expand=renderedFields`). Falls back to local conversion when not
    // available (older endpoints, search results, edge cases).
    let (description, description_format) = {
        let rendered = rf["description"].as_str().map(|s| s.trim()).filter(|s| !s.is_empty());
        if let Some(html) = rendered {
            (Some(sanitize_html(html)), BodyFormat::Html)
        } else if f["description"].is_null() {
            (None, BodyFormat::Markdown)
        } else if f["description"].is_string() {
            // Wiki markup string (Jira Server/DC, no expand). Pass through —
            // not strictly Markdown, but renderable as plain text.
            (opt_s(&f["description"]), BodyFormat::Markdown)
        } else {
            let md = adf_to_markdown(&f["description"]).trim().to_string();
            if md.is_empty() { (None, BodyFormat::Markdown) } else { (Some(md), BodyFormat::Markdown) }
        }
    };

    // Sprint (customfield_10020) — array, take first active or latest.
    let cycle = f["customfield_10020"]
        .as_array()
        .and_then(|a| a.last())
        .and_then(|sprint| {
            let id = sprint["id"].as_u64().map(|n| n.to_string())
                .or_else(|| opt_s(&sprint["id"]))?;
            Some(IssueCycle {
                id,
                name:   s(&sprint["name"]),
                number: sprint["id"].as_f64().unwrap_or(0.0),
            })
        });

    // Fix version (milestone) — take first.
    let _milestone = f["fixVersions"]
        .as_array()
        .and_then(|a| a.first())
        .map(|v| IssueMilestone {
            id:           s(&v["id"]),
            name:         s(&v["name"]),
            target_date:  opt_s(&v["releaseDate"]),
            project_id:   None,
            project_name: None,
        });

    // Project → IssueTeam.
    let team = if !f["project"]["id"].is_null() {
        Some(IssueTeam {
            id:   s(&f["project"]["key"]),  // use key for JQL
            name: s(&f["project"]["name"]),
            key:  s(&f["project"]["key"]),
        })
    } else {
        None
    };

    // Comments. When `expand=renderedFields` is set, the issue payload mirrors
    // the `comment.comments[]` array under `renderedFields.comment.comments[]`,
    // with each entry's `body` field replaced by its HTML-rendered form.
    // (NOTE: `renderedBody` is the field name used by the standalone comment
    // endpoint with `expand=renderedBody`; the issue endpoint uses `body`.)
    let rendered_comments = rf["comment"]["comments"].as_array();
    let comments: Vec<IssueComment> = f["comment"]["comments"]
        .as_array()
        .map(|a| {
            a.iter()
                .enumerate()
                .map(|(i, c)| {
                    // Match by id when available (defensive; API order should match),
                    // fall back to positional index.
                    let id = s(&c["id"]);
                    let rendered_html = rendered_comments.and_then(|rc| {
                        rc.iter().find(|r| s(&r["id"]) == id).or_else(|| rc.get(i))
                    }).and_then(|r| r["body"].as_str())
                      .map(str::trim)
                      .filter(|s| !s.is_empty());

                    let (body, body_format) = if let Some(html) = rendered_html {
                        (sanitize_html(html), BodyFormat::Html)
                    } else if c["body"].is_string() {
                        (s(&c["body"]), BodyFormat::Markdown)
                    } else {
                        (adf_to_markdown(&c["body"]).trim().to_string(), BodyFormat::Markdown)
                    };

                    IssueComment {
                        id,
                        body,
                        body_format,
                        created_at: s(&c["created"]),
                        // Cloud carries `accountId`; Server/DC carries only
                        // `name`/`key`. Accept any non-empty author object —
                        // `map_user` handles the schema fallbacks itself.
                        user:       if c["author"].is_object() {
                            Some(map_user(&c["author"]))
                        } else {
                            None
                        },
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let comment_count = comments.len() as u32;

    // Attachments
    let attachments: Vec<IssueAttachment> = f["attachment"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|att| IssueAttachment {
                    id:            s(&att["id"]),
                    filename:      s(&att["filename"]),
                    mime_type:     opt_s(&att["mimeType"]),
                    size:          att["size"].as_u64(),
                    content_url:   s(&att["content"]),
                    thumbnail_url: opt_s(&att["thumbnail"]),
                    created_at:    opt_s(&att["created"]),
                    author:        if att["author"].is_object() {
                        Some(map_user(&att["author"]))
                    } else {
                        None
                    },
                })
                .collect()
        })
        .unwrap_or_default();

    Issue {
        id:             s(&v["id"]),
        identifier:     key.clone(),
        title:          s(&f["summary"]),
        description,
        description_format,
        status:         map_status(&f["status"]),
        priority,
        priority_label,
        assignee:       if !f["assignee"].is_null() { Some(map_user(&f["assignee"])) } else { None },
        labels,
        url:            issue_url(domain, &key),
        created_at:     s(&f["created"]),
        updated_at:     s(&f["updated"]),
        due_date:       opt_s(&f["duedate"]),
        estimate:       f["customfield_10016"].as_f64(),
        team,
        project:        None,  // Jira project is mapped to team; leave project empty
        cycle,
        comments,
        comment_count,
        attachments,
    }
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

fn require_config() -> Result<JiraConfig> {
    oauth_jira::get_config()?
        .ok_or_else(|| AppError::AuthFailed("Not connected to Jira".into()))
}

/// Validate Basic Auth credentials (email + API token + domain) and return the user.
pub async fn validate_and_save_basic(
    email: &str,
    api_token: &str,
    domain: &str,
) -> Result<IssueUser> {
    oauth_jira::save_basic_auth(email, api_token, domain)?;
    let cfg = require_config()?;
    let myself_url = format!("{}/myself", cfg.base_url);
    println!("MySelf url: '{myself_url}'");
    let data = api_get_url(&cfg, &myself_url).await
        .map_err(|e| AppError::AuthFailed(format!("Jira /myself failed: {e}")))?;
    Ok(map_user(&data))
}

/// Return current auth status (authenticated flag + user info).
pub async fn get_auth_status() -> Result<JiraAuthStatus> {
    let Some(cfg) = oauth_jira::get_config()? else {
        return Ok(JiraAuthStatus { authenticated: false, user: None, domain: None, auth_method: None });
    };
    match api_get_url(&cfg, &format!("{}/myself", cfg.base_url)).await {
        Ok(data) => Ok(JiraAuthStatus {
            authenticated: true,
            user:          Some(map_user(&data)),
            domain:        cfg.domain.clone(),
            auth_method:   Some(cfg.auth_method.clone()),
        }),
        Err(_) => Ok(JiraAuthStatus { authenticated: false, user: None, domain: None, auth_method: None }),
    }
}

/// Remove stored Jira credentials (delegates to oauth_jira).
pub fn delete_credentials() -> Result<()> {
    oauth_jira::disconnect()
}

// ---------------------------------------------------------------------------
// Issue queries
// ---------------------------------------------------------------------------

/// Returns true if `q` looks like a Jira issue key (e.g. "PROJ-42").
fn is_jira_key(q: &str) -> bool {
    if let Some(dash) = q.rfind('-') {
        let prefix = &q[..dash];
        let suffix = &q[dash + 1..];
        !prefix.is_empty()
            && prefix.chars().all(|c| c.is_ascii_alphabetic())
            && !suffix.is_empty()
            && suffix.chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

pub async fn search_issues(filters: IssueFilters) -> Result<Vec<Issue>> {
    let cfg = require_config()?;

    let mut jql_parts: Vec<String> = Vec::new();

    if let Some(ref q) = filters.query {
        if !q.is_empty() {
            // Search strategy:
            //   #<key>     → key only     (filter by ticket key, no text
            //                              fallback)
            //   ~<text>    → text only    (escape hatch to skip key matching
            //                              when the user explicitly wants
            //                              description/comment search only —
            //                              same `~` operator Jira itself uses)
            //   <text>     → key OR text  (default — covers both "PROJ-42"
            //                              typed directly and free-form text)
            if let Some(rest) = q.strip_prefix('#') {
                let trimmed = rest.trim();
                if !trimmed.is_empty() {
                    if is_jira_key(trimmed) {
                        let escaped = trimmed.replace('"', "\\\"");
                        jql_parts.push(format!("key = \"{escaped}\""));
                    } else {
                        // Partial key (still typing) — JQL has no `key ~ ...`
                        // operator, and any literal that doesn't validate as
                        // a key throws 400. Short-circuit with an empty
                        // result rather than emitting an invalid clause.
                        return Ok(Vec::new());
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
                if is_jira_key(q) {
                    jql_parts.push(format!("(key = \"{escaped}\" OR text ~ \"{escaped}\")"));
                } else {
                    jql_parts.push(format!("text ~ \"{escaped}\""));
                }
            }
        }
    }

    if !filters.status_ids.is_empty() {
        let ids = filters.status_ids.iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        jql_parts.push(format!("status in ({ids})"));
    }

    if !filters.issue_type_ids.is_empty() {
        let ids = filters.issue_type_ids.iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(",");
        jql_parts.push(format!("issuetype in ({ids})"));
    }

    if !filters.label_ids.is_empty() {
        let labels = filters.label_ids.iter()
            .map(|l| format!("\"{l}\""))
            .collect::<Vec<_>>()
            .join(",");
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

    let jql = if jql_parts.is_empty() {
        "ORDER BY updated DESC".to_string()
    } else {
        format!("{} ORDER BY updated DESC", jql_parts.join(" AND "))
    };

    let max_results = filters.limit.unwrap_or(50).min(100);
    let fields_str  = ISSUE_FIELDS.join(",");
    let url = format!(
        "{}/search?jql={}&fields={fields_str}&maxResults={max_results}&startAt=0",
        cfg.base_url,
        jql_encode(&jql),
    );

    let data = api_get_url(&cfg, &url).await?;
    let domain = cfg.domain.as_deref();
    Ok(data["issues"]
        .as_array()
        .map(|a| a.iter().map(|v| map_issue(v, domain)).collect())
        .unwrap_or_default())
}

pub async fn get_issue(key: &str) -> Result<Issue> {
    let cfg = require_config()?;
    let fields_str = ISSUE_FIELDS_DETAIL.join(",");
    // `expand=renderedFields` makes Jira return ADF/wiki markup rendered to HTML
    // for both the description and each comment body — see `map_issue`.
    let data = jira_get(
        &cfg,
        &format!("/issue/{key}?fields={fields_str}&expand=renderedFields"),
    ).await?;
    let domain = cfg.domain.as_deref();
    Ok(map_issue(&data, domain))
}

// ---------------------------------------------------------------------------
// Filter options
// ---------------------------------------------------------------------------

/// Fetch all Jira projects.
/// Tries `/project/search` (Cloud + DC ≥8.4, paginated) first.
/// Falls back to `/project` (older Jira Server, returns all at once) if that endpoint fails.
async fn fetch_all_projects(cfg: &JiraConfig) -> Vec<IssueTeam> {
    let page_size = 50usize;
    let mut all: Vec<IssueTeam> = Vec::new();
    let mut start = 0usize;
    let mut search_endpoint_ok = true;

    loop {
        let path = format!("/project/search?maxResults={page_size}&startAt={start}&expand=");
        let raw = match jira_get(cfg, &path).await {
            Ok(v)  => v,
            Err(_) => {
                search_endpoint_ok = false;
                break; // will fall back below
            }
        };

        // Flat array → some Server/DC builds return plain array from /project/search
        if raw.is_array() {
            let arr = raw.as_array().unwrap();
            for p in arr {
                if !s(&p["key"]).is_empty() {
                    all.push(IssueTeam { id: s(&p["key"]), name: s(&p["name"]), key: s(&p["key"]) });
                }
            }
            break;
        }

        // Paginated Cloud response: { values: [...], isLast: bool }
        let values = raw["values"].as_array().cloned().unwrap_or_default();
        let is_last = raw["isLast"].as_bool().unwrap_or(true);
        for p in &values {
            if !s(&p["key"]).is_empty() {
                all.push(IssueTeam { id: s(&p["key"]), name: s(&p["name"]), key: s(&p["key"]) });
            }
        }
        if is_last || values.is_empty() { break; }
        start += page_size;
    }

    // Fallback: older Jira Server (<8.4) doesn't have /project/search → use /project
    if !search_endpoint_ok {
        if let Ok(raw) = jira_get(cfg, "/project?maxResults=500").await {
            let arr: Vec<Value> = if raw.is_array() {
                raw.as_array().cloned().unwrap_or_default()
            } else {
                raw["values"].as_array().cloned().unwrap_or_default()
            };
            for p in &arr {
                if !s(&p["key"]).is_empty() {
                    all.push(IssueTeam { id: s(&p["key"]), name: s(&p["name"]), key: s(&p["key"]) });
                }
            }
        }
    }

    all.sort_by(|a, b| a.name.cmp(&b.name));
    all
}

pub async fn get_filter_options() -> Result<IssueFilterOptions> {
    let cfg = require_config()?;

    // Fetch concurrently: projects (all pages), statuses, labels, issue types, myself.
    let myself_url = format!("{}/myself", cfg.base_url);
    let (teams, (statuses_res, labels_res, issue_types_res, me_res)) = tokio::join!(
        fetch_all_projects(&cfg),
        async {
            tokio::join!(
                jira_get(&cfg, "/status"),
                jira_get(&cfg, "/label?maxResults=200"),
                jira_get(&cfg, "/issuetype"),
                api_get_url(&cfg, &myself_url),
            )
        },
    );

    let statuses: Vec<IssueStatus> = statuses_res
        .unwrap_or(json!([]))
        .as_array()
        .map(|a| a.iter().map(map_status).collect())
        .unwrap_or_default();

    let labels: Vec<IssueLabel> = labels_res
        .unwrap_or(json!({}))["values"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l.as_str())
                .map(|name| IssueLabel {
                    id:    name.to_string(),
                    name:  name.to_string(),
                    color: label_color(name),
                })
                .collect()
        })
        .unwrap_or_default();

    let issue_types: Vec<IssueLabel> = {
        // /issuetype returns a flat array of issue type objects.
        let raw = issue_types_res.unwrap_or(json!([]));
        raw.as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter(|t| !s(&t["name"]).is_empty() && t["subtask"].as_bool() != Some(true))
            .map(|t| IssueLabel {
                id:    s(&t["name"]),  // JQL uses name for issuetype filter
                name:  s(&t["name"]),
                color: issue_type_color(s(&t["name"]).as_str()),
            })
            .collect()
    };

    let me = me_res.ok().as_ref().map(map_user);

    // Sprints (agile API, best-effort — fails gracefully if Jira Software not available).
    let cycles = fetch_active_sprints(&cfg).await.unwrap_or_default();

    Ok(IssueFilterOptions {
        teams,
        statuses,
        labels,
        issue_types,
        projects: vec![],  // projects are returned as teams in Jira
        cycles,
        milestones: vec![],
        me,
    })
}

async fn fetch_active_sprints(cfg: &JiraConfig) -> Result<Vec<IssueCycle>> {
    // Use agile endpoint: GET /agile/1.0/board?type=scrum&maxResults=5
    // then GET /agile/1.0/board/{id}/sprint?state=active&maxResults=20
    let boards_url = format!("{}/board?type=scrum&maxResults=5", cfg.agile_url);
    let boards = api_get_url(cfg, &boards_url).await?;

    let mut cycles = Vec::new();
    for board in boards["values"].as_array().unwrap_or(&vec![]) {
        let board_id = board["id"].as_u64().unwrap_or(0);
        if board_id == 0 { continue; }
        let sprints_url = format!(
            "{}/board/{board_id}/sprint?state=active&maxResults=10",
            cfg.agile_url
        );
        if let Ok(sprints) = api_get_url(cfg, &sprints_url).await {
            for sprint in sprints["values"].as_array().unwrap_or(&vec![]) {
                let id = sprint["id"].as_u64().map(|n| n.to_string()).unwrap_or_default();
                if id.is_empty() { continue; }
                cycles.push(IssueCycle {
                    id,
                    name:   s(&sprint["name"]),
                    number: sprint["id"].as_f64().unwrap_or(0.0),
                });
            }
        }
    }
    Ok(cycles)
}

// ---------------------------------------------------------------------------
// Issue mutations
// ---------------------------------------------------------------------------

pub async fn transition_issue(key: &str, status_id: &str) -> Result<Issue> {
    let cfg = require_config()?;

    // Fetch available transitions and find the one with matching `to.id`.
    let transitions = jira_get(&cfg, &format!("/issue/{key}/transitions")).await?;
    let transition_id = transitions["transitions"]
        .as_array()
        .and_then(|a| {
            a.iter().find(|t| s(&t["to"]["id"]) == status_id || s(&t["id"]) == status_id)
        })
        .map(|t| s(&t["id"]));

    let tid = transition_id.ok_or_else(|| {
        AppError::Other(format!(
            "No available transition to status '{status_id}' for issue {key}"
        ))
    })?;

    jira_post(&cfg, &format!("/issue/{key}/transitions"), &json!({
        "transition": { "id": tid }
    })).await?;

    get_issue(key).await
}

pub async fn assign_issue(key: &str, account_id: Option<&str>) -> Result<Issue> {
    let cfg = require_config()?;
    let body = match account_id {
        Some(id) => json!({ "accountId": id }),
        None     => json!({ "accountId": null }),
    };
    jira_put(&cfg, &format!("/issue/{key}/assignee"), &body).await?;
    get_issue(key).await
}

pub async fn add_comment(key: &str, body: &str) -> Result<IssueComment> {
    let cfg = require_config()?;
    // Ask Jira to render the freshly created comment to HTML so the UI can
    // show it with full styling immediately, without a second round-trip.
    let resp = jira_post(
        &cfg,
        &format!("/issue/{key}/comment?expand=renderedBody"),
        &json!({ "body": text_to_adf(body) }),
    ).await?;

    let (comment_body, body_format) = {
        let rendered = resp["renderedBody"].as_str().map(|s| s.trim()).filter(|s| !s.is_empty());
        if let Some(html) = rendered {
            (sanitize_html(html), BodyFormat::Html)
        } else if resp["body"].is_string() {
            (s(&resp["body"]), BodyFormat::Markdown)
        } else {
            (adf_to_markdown(&resp["body"]).trim().to_string(), BodyFormat::Markdown)
        }
    };

    Ok(IssueComment {
        id:         s(&resp["id"]),
        body:       comment_body,
        body_format,
        created_at: s(&resp["created"]),
        user:       if resp["author"].is_object() {
            Some(map_user(&resp["author"]))
        } else {
            None
        },
    })
}

/// Download an attachment from a Jira `content` URL using the connected
/// account's credentials, streaming the body chunk-by-chunk straight to disk
/// at `dest_path`. The download runs entirely on the Tokio runtime — both the
/// network read and the file write are async, so the UI thread is never
/// touched and no Tokio worker is blocked on synchronous I/O.
///
/// Only URLs whose host matches the configured Jira base host are accepted —
/// this prevents the IPC command from being abused as a generic authenticated
/// proxy (an attacker who could trick the user into clicking a forged URL).
pub async fn download_attachment(content_url: &str, dest_path: &std::path::Path) -> Result<u64> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let cfg = require_config()?;

    // Host allow-list: only the configured Jira instance.
    let cfg_host = reqwest::Url::parse(&cfg.base_url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_lowercase()))
        .ok_or_else(|| AppError::Other("Jira base URL has no host".into()))?;
    let url_host = reqwest::Url::parse(content_url)
        .map_err(|e| AppError::Other(format!("Invalid attachment URL: {e}")))?
        .host_str()
        .map(|s| s.to_lowercase())
        .ok_or_else(|| AppError::Other("Attachment URL has no host".into()))?;
    if url_host != cfg_host {
        return Err(AppError::Other(format!(
            "Attachment host '{url_host}' does not match Jira host '{cfg_host}' — refusing to download"
        )));
    }

    let resp = jira_send_with_refresh(|c| {
        http_client()
            .get(content_url)
            .header("Authorization", &c.auth_header)
            .header("X-Atlassian-Token", "no-check")
            // Jira returns the binary on the first response when this header is set;
            // omitting it can land on an HTML interstitial in some Server versions.
            .header("Accept", "*/*")
    }).await?;

    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::AuthFailed("Invalid or expired Jira credentials".into()));
    }
    if !status.is_success() {
        return Err(AppError::Other(format!("Attachment HTTP {status}")));
    }

    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Other(format!("Cannot create dir: {e}")))?;
        }
    }

    let mut file = tokio::fs::File::create(dest_path)
        .await
        .map_err(|e| AppError::Other(format!("Cannot create file: {e}")))?;

    let mut stream = resp.bytes_stream();
    let mut total: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| AppError::Other(format!("Network read failed: {e}")))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| AppError::Other(format!("Disk write failed: {e}")))?;
        total += bytes.len() as u64;
    }
    file.flush().await.ok();

    Ok(total)
}

pub async fn create_issue_req(
    title: &str,
    description: Option<&str>,
    team_id: &str,       // Jira project key
    status_id: Option<&str>,
    assignee_id: Option<&str>,
    label_ids: Vec<String>,
    priority: Option<u32>,
    _project_id: Option<&str>,  // unused in Jira (mapped to team)
    milestone_id: Option<&str>,
    due_date: Option<&str>,
    estimate: Option<f64>,
    issue_type: Option<&str>,
) -> Result<Issue> {
    let cfg = require_config()?;

    let mut fields = serde_json::Map::new();
    fields.insert("project".into(),   json!({ "key": team_id }));
    fields.insert("summary".into(),   json!(title));
    fields.insert("issuetype".into(), json!({ "name": issue_type.unwrap_or("Task") }));

    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            fields.insert("description".into(), text_to_adf(desc));
        }
    }

    if let Some(aid) = assignee_id {
        fields.insert("assignee".into(), json!({ "accountId": aid }));
    }

    if !label_ids.is_empty() {
        fields.insert("labels".into(), json!(label_ids));
    }

    if let Some(p) = priority {
        let priority_name = match p {
            1 => "Highest",
            2 => "High",
            3 => "Medium",
            4 => "Low",
            _ => "Medium",
        };
        fields.insert("priority".into(), json!({ "name": priority_name }));
    }

    if let Some(mid) = milestone_id {
        fields.insert("fixVersions".into(), json!([{ "id": mid }]));
    }

    if let Some(dd) = due_date {
        fields.insert("duedate".into(), json!(dd));
    }

    if let Some(est) = estimate {
        fields.insert("customfield_10016".into(), json!(est));
    }

    let resp = jira_post(&cfg, "/issue", &json!({ "fields": Value::Object(fields) })).await?;
    let key  = s(&resp["key"]);

    // If a status was requested, transition to it right after creation.
    if let Some(sid) = status_id {
        if let Err(e) = transition_issue(&key, sid).await {
            tracing::warn!("jira: post-create transition failed: {e}");
        }
    }

    get_issue(&key).await
}

// ---------------------------------------------------------------------------
