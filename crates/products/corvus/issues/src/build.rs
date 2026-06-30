//! `NewIssue` assembly — shared by the shell `create_issue_req` shims and the
//! `corvus-be` create handlers, so the two never drift.

use corvus_issue_tracker_api::prelude::NewIssue;

/// Assemble a Linear create-issue request. `issue_type` is always `None` (Linear
/// has no issue types); `project_id` is carried through.
#[allow(clippy::too_many_arguments)]
pub fn linear_new_issue(
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
) -> NewIssue {
    NewIssue {
        title:        title.to_string(),
        description:  description.map(str::to_string),
        team_id:      Some(team_id.to_string()),
        status_id:    status_id.map(str::to_string),
        assignee_id:  assignee_id.map(str::to_string),
        label_ids,
        priority,
        project_id:   project_id.map(str::to_string),
        milestone_id: milestone_id.map(str::to_string),
        due_date:     due_date.map(str::to_string),
        estimate,
        issue_type:   None,
    }
}

/// Assemble a Jira create-issue request. `issue_type` is carried through;
/// `project_id` is unused in Jira (mapped to team/project) and forced to `None`.
#[allow(clippy::too_many_arguments)]
pub fn jira_new_issue(
    title: &str,
    description: Option<&str>,
    team_id: &str,
    status_id: Option<&str>,
    assignee_id: Option<&str>,
    label_ids: Vec<String>,
    priority: Option<u32>,
    milestone_id: Option<&str>,
    due_date: Option<&str>,
    estimate: Option<f64>,
    issue_type: Option<&str>,
) -> NewIssue {
    NewIssue {
        title:        title.to_string(),
        description:  description.map(str::to_string),
        team_id:      Some(team_id.to_string()),
        status_id:    status_id.map(str::to_string),
        assignee_id:  assignee_id.map(str::to_string),
        label_ids,
        priority,
        project_id:   None,
        milestone_id: milestone_id.map(str::to_string),
        due_date:     due_date.map(str::to_string),
        estimate,
        issue_type:   issue_type.map(str::to_string),
    }
}
