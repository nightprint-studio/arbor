//! Pure Jira helpers: HTML sanitizer, ADF → Markdown, JQL encoding, JSON → DTO
//! mappers, and the field selections. No network, no auth — unit-tested.

use serde_json::{json, Value};

use corvus_issue_tracker_api::prelude::{
    BodyFormat, Issue, IssueAttachment, IssueComment, IssueCycle, IssueLabel, IssueMilestone,
    IssueStatus, IssueTeam, IssueUser,
};

// ── HTML sanitizer for Jira `renderedFields.*` ────────────────────────────────

/// Sanitize HTML coming from Jira's pre-rendered `renderedFields`.
///
/// Jira returns HTML produced from either ADF (Cloud) or wiki markup (Server/DC).
/// We strip scripts/iframes/styles and forbid event handlers; we keep `class`
/// attributes (used by Jira for code-highlight wrappers, panels, table styling)
/// and force safe `rel` on links.
pub(crate) fn sanitize_html(input: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(["class"])
        .link_rel(Some("noopener noreferrer nofollow"))
        .clean(input)
        .to_string()
}

// ── ADF (Atlassian Document Format) → Markdown ────────────────────────────────

fn apply_mark(text: &str, mark: &Value) -> String {
    match mark["type"].as_str().unwrap_or("") {
        "strong" => format!("**{text}**"),
        "em"     => format!("*{text}*"),
        "code"   => format!("`{text}`"),
        "strike" => format!("~~{text}~~"),
        "link"   => {
            let href = mark["attrs"]["href"].as_str().unwrap_or("#");
            format!("[{text}]({href})")
        }
        _ => text.to_string(),
    }
}

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
        "hardBreak" => "\n".to_string(),
        "mention" => {
            let name = node["attrs"]["text"]
                .as_str()
                .or_else(|| node["attrs"]["displayName"].as_str())
                .unwrap_or("someone");
            format!("@{name}")
        }
        "emoji" => node["attrs"]["shortName"]
            .as_str()
            .map(|s| s.trim_matches(':').to_string())
            .unwrap_or_default(),
        "inlineCard" => {
            let url = node["attrs"]["url"].as_str().unwrap_or("#");
            format!("[{url}]({url})")
        }
        "date" => node["attrs"]["timestamp"].as_str().unwrap_or("").to_string(),
        _ => node["content"]
            .as_array()
            .map(|a| a.iter().map(render_inline).collect::<String>())
            .unwrap_or_default(),
    }
}

fn inline_children(node: &Value) -> String {
    node["content"]
        .as_array()
        .map(|a| a.iter().map(render_inline).collect::<String>())
        .unwrap_or_default()
}

/// Convert an ADF document tree to Markdown, preserving structure (headings,
/// lists, code blocks, blockquotes, panels, tables) and inline marks.
pub(crate) fn adf_to_markdown(node: &Value) -> String {
    adf_block(node, 0)
}

fn adf_block(node: &Value, list_depth: usize) -> String {
    match node["type"].as_str().unwrap_or("") {
        "doc" => node["content"]
            .as_array()
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
            let items: String = node["content"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|item| {
                            let content = list_item_content(item, list_depth + 1);
                            format!("{indent}- {content}")
                        })
                        .collect()
                })
                .unwrap_or_default();
            if list_depth == 0 { format!("{items}\n") } else { items }
        }

        "orderedList" => {
            let indent = "  ".repeat(list_depth);
            let start = node["attrs"]["order"].as_u64().unwrap_or(1);
            let items: String = node["content"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .enumerate()
                        .map(|(i, item)| {
                            let num = start + i as u64;
                            let content = list_item_content(item, list_depth + 1);
                            format!("{indent}{num}. {content}")
                        })
                        .collect()
                })
                .unwrap_or_default();
            if list_depth == 0 { format!("{items}\n") } else { items }
        }

        "listItem" => list_item_content(node, list_depth),

        "blockquote" => {
            let inner = node["content"]
                .as_array()
                .map(|a| a.iter().map(|n| adf_block(n, 0)).collect::<String>())
                .unwrap_or_default();
            let quoted: String = inner.lines().map(|l| format!("> {l}\n")).collect();
            format!("{quoted}\n")
        }

        "codeBlock" => {
            let lang = node["attrs"]["language"].as_str().unwrap_or("");
            let code: String = node["content"]
                .as_array()
                .map(|a| a.iter().filter_map(|n| n["text"].as_str()).collect())
                .unwrap_or_default();
            format!("```{lang}\n{code}\n```\n\n")
        }

        "rule" => "---\n\n".to_string(),

        "panel" => {
            let panel_type = node["attrs"]["panelType"].as_str().unwrap_or("info");
            let prefix = match panel_type {
                "warning" => "⚠️ ",
                "error" => "❌ ",
                "success" => "✅ ",
                "note" => "📝 ",
                _ => "ℹ️ ",
            };
            let inner = node["content"]
                .as_array()
                .map(|a| a.iter().map(|n| adf_block(n, 0)).collect::<String>())
                .unwrap_or_default();
            let quoted: String = inner
                .lines()
                .enumerate()
                .map(|(i, l)| if i == 0 { format!("> {prefix}{l}\n") } else { format!("> {l}\n") })
                .collect();
            format!("{quoted}\n")
        }

        "table" => adf_table(node),

        "mediaSingle" | "mediaGroup" | "media" => {
            let alt = node["content"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|m| m["attrs"]["alt"].as_str().or_else(|| m["attrs"]["type"].as_str()))
                .unwrap_or("attachment");
            format!("*[{alt}]*\n\n")
        }

        "text" | "hardBreak" | "mention" | "emoji" | "inlineCard" => render_inline(node),

        _ => node["content"]
            .as_array()
            .map(|a| a.iter().map(|n| adf_block(n, list_depth)).collect::<String>())
            .unwrap_or_default(),
    }
}

fn list_item_content(item: &Value, depth: usize) -> String {
    let children = item["content"].as_array().cloned().unwrap_or_default();
    let mut out = String::new();
    for (i, child) in children.iter().enumerate() {
        match child["type"].as_str().unwrap_or("") {
            "paragraph" => {
                let text = inline_children(child);
                if i == 0 {
                    out.push_str(text.trim_end_matches('\n'));
                    out.push('\n');
                } else {
                    out.push_str(&text);
                }
            }
            "bulletList" | "orderedList" => out.push_str(&adf_block(child, depth)),
            _ => out.push_str(&adf_block(child, depth)),
        }
    }
    if out.is_empty() {
        out.push('\n');
    }
    out
}

fn adf_table(node: &Value) -> String {
    let rows = node["content"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut header_sep_added = false;
    for row in &rows {
        let cells: Vec<String> = row["content"]
            .as_array()
            .map(|cols| {
                cols.iter()
                    .map(|cell| {
                        let text = cell["content"]
                            .as_array()
                            .map(|a| a.iter().map(|n| adf_block(n, 0)).collect::<String>())
                            .unwrap_or_default();
                        text.trim().replace('\n', " ")
                    })
                    .collect()
            })
            .unwrap_or_default();
        if cells.is_empty() {
            continue;
        }
        lines.push(format!("| {} |", cells.join(" | ")));
        if !header_sep_added {
            let sep = cells.iter().map(|_| "---").collect::<Vec<_>>().join(" | ");
            lines.push(format!("| {sep} |"));
            header_sep_added = true;
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    format!("{}\n\n", lines.join("\n"))
}

/// Wrap plain text in a minimal ADF document.
pub(crate) fn text_to_adf(text: &str) -> Value {
    if text.trim().is_empty() {
        return json!({ "type": "doc", "version": 1, "content": [] });
    }
    let content: Vec<Value> = text
        .lines()
        .map(|line| {
            if line.is_empty() {
                json!({ "type": "paragraph", "content": [] })
            } else {
                json!({ "type": "paragraph", "content": [{ "type": "text", "text": line }] })
            }
        })
        .collect();
    json!({ "type": "doc", "version": 1, "content": content })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Percent-encode a JQL string for use in a URL query parameter.
pub(crate) fn jql_encode(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '*' | ',' => vec![c as u8],
            ' ' => vec![b'%', b'2', b'0'],
            _ => format!("%{:02X}", c as u32).into_bytes(),
        })
        .map(|b| b as char)
        .collect()
}

/// Returns true if `q` looks like a Jira issue key (e.g. "PROJ-42").
pub(crate) fn is_jira_key(q: &str) -> bool {
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

/// Derive the Jira Agile REST base from the main REST base URL.
pub(crate) fn agile_url_from(base: &str) -> String {
    if let Some(pos) = base.rfind("/rest/api/") {
        format!("{}/rest/agile/1.0", &base[..pos])
    } else {
        base.to_string()
    }
}

// ── Type mapping ──────────────────────────────────────────────────────────────

pub(crate) fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

pub(crate) fn opt_s(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
}

pub(crate) fn map_status(v: &Value) -> IssueStatus {
    let cat_key = s(&v["statusCategory"]["key"]);
    let cat_color = s(&v["statusCategory"]["colorName"]);
    let status_type = match cat_key.as_str() {
        "new" => "unstarted",
        "indeterminate" => "started",
        "done" => "completed",
        _ => "unstarted",
    };
    let color = match cat_color.as_str() {
        "blue-grey" => "#6b778c",
        "yellow" => "#ff991f",
        "green" => "#36b37e",
        "red" => "#ff5630",
        "blue" => "#0052cc",
        _ => "#6b7280",
    };
    IssueStatus {
        id: s(&v["id"]),
        name: s(&v["name"]),
        color: color.to_string(),
        status_type: status_type.to_string(),
    }
}

/// Map a Jira priority name to the 0–4 numeric scale used in our shared type.
pub(crate) fn map_priority(name: &str) -> (u32, String) {
    match name {
        "Highest" => (1, "Urgent".into()),
        "High" => (2, "High".into()),
        "Medium" => (3, "Medium".into()),
        "Low" => (4, "Low".into()),
        "Lowest" => (4, "Low".into()),
        _ => (0, "No priority".into()),
    }
}

/// Map a Jira user object to `IssueUser` (Cloud `accountId` / Server-DC `key`/`name`).
pub(crate) fn map_user(v: &Value) -> IssueUser {
    let avatar = v["avatarUrls"]["48x48"]
        .as_str()
        .or_else(|| v["avatarUrls"]["32x32"].as_str())
        .map(|s| s.to_string());

    let id = if !v["accountId"].is_null() {
        s(&v["accountId"])
    } else if !v["key"].is_null() {
        s(&v["key"])
    } else {
        s(&v["name"])
    };

    let display = {
        let d = s(&v["displayName"]);
        if !d.is_empty() {
            d
        } else {
            let n = s(&v["name"]);
            if !n.is_empty() {
                n
            } else {
                let k = s(&v["key"]);
                if !k.is_empty() { k } else { "Unknown".to_string() }
            }
        }
    };

    IssueUser {
        id,
        name: display.clone(),
        display_name: display,
        avatar_url: avatar,
        email: opt_s(&v["emailAddress"]),
    }
}

/// Derive a deterministic hex color from a string (for Jira labels).
fn label_color(name: &str) -> String {
    let palette = [
        "#f87171", "#fb923c", "#fbbf24", "#a3e635", "#34d399", "#22d3ee", "#818cf8", "#e879f9",
    ];
    let idx = name.bytes().fold(0u64, |acc, b| acc.wrapping_add(b as u64)) as usize;
    palette[idx % palette.len()].to_string()
}

/// Assign a deterministic color to a Jira issue type name.
pub(crate) fn issue_type_color(name: &str) -> String {
    match name {
        "Bug" => "#ef4444".into(),
        "Story" => "#22c55e".into(),
        "Task" => "#3b82f6".into(),
        "Epic" => "#a855f7".into(),
        "Improvement" => "#06b6d4".into(),
        "New Feature" => "#10b981".into(),
        "Technical task" => "#6366f1".into(),
        _ => label_color(name),
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

/// Build an `IssueLabel` from a plain Jira label string.
pub(crate) fn label_from(name: &str) -> IssueLabel {
    IssueLabel { id: name.to_string(), name: name.to_string(), color: label_color(name) }
}

/// Fields for search/list — description omitted to keep responses small.
pub(crate) const ISSUE_FIELDS: &[&str] = &[
    "summary", "status", "priority", "assignee", "labels", "issuetype", "project", "created",
    "updated", "duedate", "customfield_10016", "customfield_10020", "fixVersions", "components",
];

/// Fields for single-issue detail — includes description and full comment thread.
pub(crate) const ISSUE_FIELDS_DETAIL: &[&str] = &[
    "summary", "description", "status", "priority", "assignee", "labels", "issuetype", "project",
    "created", "updated", "duedate", "customfield_10016", "customfield_10020", "fixVersions",
    "comment", "components", "attachment",
];

/// Map a full Jira issue API response to our shared `Issue`. Prefers Jira's
/// pre-rendered HTML (`renderedFields`, sanitized) over local ADF conversion.
pub(crate) fn map_issue(v: &Value, domain: Option<&str>) -> Issue {
    let key = s(&v["key"]);
    let f = &v["fields"];
    let rf = &v["renderedFields"];

    let priority_name = s(&f["priority"]["name"]);
    let (priority, priority_label) = map_priority(&priority_name);

    let labels: Vec<IssueLabel> = f["labels"]
        .as_array()
        .map(|a| a.iter().filter_map(|l| l.as_str()).map(label_from).collect())
        .unwrap_or_default();

    let (description, description_format) = {
        let rendered = rf["description"].as_str().map(|s| s.trim()).filter(|s| !s.is_empty());
        if let Some(html) = rendered {
            (Some(sanitize_html(html)), BodyFormat::Html)
        } else if f["description"].is_null() {
            (None, BodyFormat::Markdown)
        } else if f["description"].is_string() {
            (opt_s(&f["description"]), BodyFormat::Markdown)
        } else {
            let md = adf_to_markdown(&f["description"]).trim().to_string();
            if md.is_empty() { (None, BodyFormat::Markdown) } else { (Some(md), BodyFormat::Markdown) }
        }
    };

    let cycle = f["customfield_10020"].as_array().and_then(|a| a.last()).and_then(|sprint| {
        let id = sprint["id"].as_u64().map(|n| n.to_string()).or_else(|| opt_s(&sprint["id"]))?;
        Some(IssueCycle { id, name: s(&sprint["name"]), number: sprint["id"].as_f64().unwrap_or(0.0) })
    });

    let _milestone = f["fixVersions"].as_array().and_then(|a| a.first()).map(|v| IssueMilestone {
        id: s(&v["id"]),
        name: s(&v["name"]),
        target_date: opt_s(&v["releaseDate"]),
        project_id: None,
        project_name: None,
    });

    let team = if !f["project"]["id"].is_null() {
        Some(IssueTeam { id: s(&f["project"]["key"]), name: s(&f["project"]["name"]), key: s(&f["project"]["key"]) })
    } else {
        None
    };

    let rendered_comments = rf["comment"]["comments"].as_array();
    let comments: Vec<IssueComment> = f["comment"]["comments"]
        .as_array()
        .map(|a| {
            a.iter()
                .enumerate()
                .map(|(i, c)| {
                    let id = s(&c["id"]);
                    let rendered_html = rendered_comments
                        .and_then(|rc| rc.iter().find(|r| s(&r["id"]) == id).or_else(|| rc.get(i)))
                        .and_then(|r| r["body"].as_str())
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
                        user: if c["author"].is_object() { Some(map_user(&c["author"])) } else { None },
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let comment_count = comments.len() as u32;

    let attachments: Vec<IssueAttachment> = f["attachment"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|att| IssueAttachment {
                    id: s(&att["id"]),
                    filename: s(&att["filename"]),
                    mime_type: opt_s(&att["mimeType"]),
                    size: att["size"].as_u64(),
                    content_url: s(&att["content"]),
                    thumbnail_url: opt_s(&att["thumbnail"]),
                    created_at: opt_s(&att["created"]),
                    author: if att["author"].is_object() { Some(map_user(&att["author"])) } else { None },
                })
                .collect()
        })
        .unwrap_or_default();

    Issue {
        id: s(&v["id"]),
        identifier: key.clone(),
        title: s(&f["summary"]),
        description,
        description_format,
        status: map_status(&f["status"]),
        priority,
        priority_label,
        assignee: if !f["assignee"].is_null() { Some(map_user(&f["assignee"])) } else { None },
        labels,
        url: issue_url(domain, &key),
        created_at: s(&f["created"]),
        updated_at: s(&f["updated"]),
        due_date: opt_s(&f["duedate"]),
        estimate: f["customfield_10016"].as_f64(),
        team,
        project: None,
        cycle,
        comments,
        comment_count,
        attachments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_jira_keys() {
        assert!(is_jira_key("PROJ-42"));
        assert!(!is_jira_key("PROJ"));
        assert!(!is_jira_key("42"));
        assert!(!is_jira_key("PROJ-4x"));
    }

    #[test]
    fn jql_encodes_spaces_and_specials() {
        assert_eq!(jql_encode("a b"), "a%20b");
        assert_eq!(jql_encode("key=1"), "key%3D1");
        assert_eq!(jql_encode("ABC-1,x"), "ABC-1,x");
    }

    #[test]
    fn priority_maps_to_scale() {
        assert_eq!(map_priority("Highest"), (1, "Urgent".into()));
        assert_eq!(map_priority("Whatever"), (0, "No priority".into()));
    }

    #[test]
    fn agile_url_derives_from_rest_base() {
        assert_eq!(
            agile_url_from("https://x.atlassian.net/rest/api/3"),
            "https://x.atlassian.net/rest/agile/1.0"
        );
        assert_eq!(agile_url_from("https://x/no-rest"), "https://x/no-rest");
    }

    #[test]
    fn adf_renders_headings_and_marks() {
        let doc = json!({
            "type": "doc", "content": [
                { "type": "heading", "attrs": { "level": 2 }, "content": [{ "type": "text", "text": "Title" }] },
                { "type": "paragraph", "content": [{ "type": "text", "text": "bold", "marks": [{ "type": "strong" }] }] }
            ]
        });
        let md = adf_to_markdown(&doc);
        assert!(md.contains("## Title"));
        assert!(md.contains("**bold**"));
    }

    #[test]
    fn maps_issue_url_from_domain() {
        let v = json!({ "id": "1", "key": "ENG-7", "fields": { "summary": "x", "status": {}, "priority": {} } });
        let issue = map_issue(&v, Some("acme.atlassian.net"));
        assert_eq!(issue.url, "https://acme.atlassian.net/browse/ENG-7");
        assert_eq!(issue.identifier, "ENG-7");
    }
}
