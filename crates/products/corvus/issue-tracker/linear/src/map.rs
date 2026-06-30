//! Pure JSON → DTO mappers, the shared GraphQL field fragment, and the search
//! filter builder. No network, no auth — unit-tested.

use serde_json::{json, Map, Value};

use corvus_issue_tracker_api::prelude::{
    BodyFormat, Issue, IssueComment, IssueCycle, IssueFilters, IssueLabel, IssueMilestone,
    IssueProject, IssueStatus, IssueTeam, IssueUser,
};

/// GraphQL field selection shared by every issue query/mutation.
pub(crate) const ISSUE_FIELDS: &str = r#"
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

pub(crate) fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

pub(crate) fn opt_s(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
}

pub(crate) fn map_user(v: &Value) -> IssueUser {
    IssueUser {
        id:           s(&v["id"]),
        name:         s(&v["name"]),
        display_name: s(&v["displayName"]),
        avatar_url:   opt_s(&v["avatarUrl"]),
        email:        opt_s(&v["email"]),
    }
}

pub(crate) fn map_status(v: &Value) -> IssueStatus {
    IssueStatus {
        id:          s(&v["id"]),
        name:        s(&v["name"]),
        color:       s(&v["color"]),
        status_type: s(&v["type"]),
    }
}

pub(crate) fn map_label(v: &Value) -> IssueLabel {
    IssueLabel { id: s(&v["id"]), name: s(&v["name"]), color: s(&v["color"]) }
}

pub(crate) fn map_team(v: &Value) -> IssueTeam {
    IssueTeam { id: s(&v["id"]), name: s(&v["name"]), key: s(&v["key"]) }
}

pub(crate) fn map_project(v: &Value) -> IssueProject {
    IssueProject { id: s(&v["id"]), name: s(&v["name"]), color: opt_s(&v["color"]) }
}

pub(crate) fn map_cycle(v: &Value) -> IssueCycle {
    IssueCycle { id: s(&v["id"]), name: s(&v["name"]), number: v["number"].as_f64().unwrap_or(0.0) }
}

pub(crate) fn map_milestone(v: &Value) -> IssueMilestone {
    IssueMilestone {
        id:           s(&v["id"]),
        name:         s(&v["name"]),
        target_date:  opt_s(&v["targetDate"]),
        project_id:   opt_s(&v["project"]["id"]),
        project_name: opt_s(&v["project"]["name"]),
    }
}

pub(crate) fn map_comment(v: &Value) -> IssueComment {
    IssueComment {
        id:          s(&v["id"]),
        body:        s(&v["body"]),
        body_format: BodyFormat::Markdown,
        created_at:  s(&v["createdAt"]),
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

pub(crate) fn map_issue(v: &Value) -> Issue {
    let comments: Vec<IssueComment> = v["comments"]["nodes"]
        .as_array()
        .map(|a| a.iter().map(map_comment).collect())
        .unwrap_or_default();
    let comment_count = comments.len() as u32;

    Issue {
        id:                 s(&v["id"]),
        identifier:         s(&v["identifier"]),
        title:              s(&v["title"]),
        description:        opt_s(&v["description"]),
        description_format: BodyFormat::Markdown,
        status:             map_status(&v["state"]),
        priority:           v["priority"].as_u64().unwrap_or(0) as u32,
        priority_label:     s(&v["priorityLabel"]),
        assignee:           if obj_present(&v["assignee"]) { Some(map_user(&v["assignee"])) } else { None },
        labels:             v["labels"]["nodes"].as_array()
                                .map(|a| a.iter().map(map_label).collect())
                                .unwrap_or_default(),
        url:                s(&v["url"]),
        created_at:         s(&v["createdAt"]),
        updated_at:         s(&v["updatedAt"]),
        due_date:           opt_s(&v["dueDate"]),
        estimate:           v["estimate"].as_f64(),
        team:               if obj_present(&v["team"])    { Some(map_team(&v["team"]))       } else { None },
        project:            if obj_present(&v["project"]) { Some(map_project(&v["project"])) } else { None },
        cycle:              if obj_present(&v["cycle"])   { Some(map_cycle(&v["cycle"]))     } else { None },
        comments,
        comment_count,
        attachments: Vec::new(),
    }
}

/// Decompose a full identifier ("ENG-42") into `(team_key, number)`.
/// Returns `None` for plain numbers / free-text queries.
pub(crate) fn parse_full_identifier(q: &str) -> Option<(String, i64)> {
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

/// Build the `IssueFilter` GraphQL object from the cross-provider [`IssueFilters`].
///
/// Query prefix convention:
///   · `#<key>` → identifier only · `~<text>` → title only · `<text>` → both.
pub(crate) fn build_search_filter(filters: &IssueFilters) -> Map<String, Value> {
    let mut filter = Map::new();
    if let Some(ref q) = filters.query {
        if !q.is_empty() {
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
                // Exact identifier ("ENG-42") plus title fallback so the user
                // gets both the matching ticket and any titles that mention it.
                filter.insert("or".into(), json!([
                    { "title":  { "containsIgnoreCase": q } },
                    { "number": { "eq": num }, "team": { "key": { "eq": team_key } } }
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
    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_identifier() {
        assert_eq!(parse_full_identifier("ENG-42"), Some(("ENG".into(), 42)));
        assert_eq!(parse_full_identifier("eng-7"), Some(("ENG".into(), 7)));
        assert_eq!(parse_full_identifier("42"), None);
        assert_eq!(parse_full_identifier("ENG-"), None);
        assert_eq!(parse_full_identifier("ENG-4x"), None);
    }

    #[test]
    fn search_filter_hash_prefix_is_identifier_only() {
        let f = IssueFilters { query: Some("#ENG-42".into()), ..Default::default() };
        let m = build_search_filter(&f);
        assert_eq!(m["number"], json!({ "eq": 42 }));
        assert_eq!(m["team"], json!({ "key": { "eq": "ENG" } }));
        assert!(m.get("title").is_none());
    }

    #[test]
    fn search_filter_tilde_prefix_is_title_only() {
        let f = IssueFilters { query: Some("~login bug".into()), ..Default::default() };
        let m = build_search_filter(&f);
        assert_eq!(m["title"], json!({ "containsIgnoreCase": "login bug" }));
        assert!(m.get("number").is_none());
    }

    #[test]
    fn search_filter_plain_identifier_matches_both() {
        let f = IssueFilters { query: Some("ENG-42".into()), ..Default::default() };
        let m = build_search_filter(&f);
        assert!(m.get("or").is_some());
    }

    #[test]
    fn maps_issue_from_graphql_json() {
        let v = json!({
            "id": "abc", "identifier": "ENG-1", "title": "Hello",
            "state": { "id": "s", "name": "Todo", "color": "#fff", "type": "unstarted" },
            "priority": 2, "priorityLabel": "High",
            "assignee": { "id": "u1", "name": "neo", "displayName": "Neo" },
            "labels": { "nodes": [ { "id": "l1", "name": "bug", "color": "#f00" } ] },
            "url": "https://linear.app/x", "createdAt": "t0", "updatedAt": "t1",
            "comments": { "nodes": [ { "id": "c1", "body": "hi", "createdAt": "t2" } ] }
        });
        let issue = map_issue(&v);
        assert_eq!(issue.identifier, "ENG-1");
        assert_eq!(issue.priority, 2);
        assert_eq!(issue.labels.len(), 1);
        assert_eq!(issue.comment_count, 1);
        assert!(issue.assignee.is_some());
        assert!(issue.cycle.is_none());
    }
}
