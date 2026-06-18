import { corvus } from '$lib/ipc/rpc';
import type {
  Issue, IssueComment, IssueFilterOptions, IssueFilters,
  LinearAuthStatus, JiraAuthStatus,
} from '$lib/types/issues';

export function linearGetAuthStatus(): Promise<LinearAuthStatus> {
  return corvus('linear_get_auth_status');
}

export function linearSearchIssues(filters: IssueFilters): Promise<Issue[]> {
  return corvus('linear_search_issues', { filters });
}

export function linearGetIssue(id: string): Promise<Issue> {
  return corvus('linear_get_issue', { id });
}

export function linearGetFilterOptions(): Promise<IssueFilterOptions> {
  return corvus('linear_get_filter_options');
}

export function linearTransitionIssue(id: string, statusId: string): Promise<Issue> {
  return corvus('linear_transition_issue', { id, status_id: statusId });
}

export function linearAssignIssue(id: string, userId: string | null): Promise<Issue> {
  return corvus('linear_assign_issue', { id, user_id: userId });
}

export function linearAddComment(issueId: string, body: string): Promise<IssueComment> {
  return corvus('linear_add_comment', { issue_id: issueId, body });
}

export function linearCreateIssue(params: {
  title:        string;
  description?: string;
  teamId:       string;
  statusId?:    string;
  assigneeId?:  string;
  labelIds?:    string[];
  priority?:    number;
  projectId?:   string;
  milestoneId?: string;
  dueDate?:     string;
  estimate?:    number;
}): Promise<Issue> {
  return corvus('linear_create_issue', {
    title:        params.title,
    description:  params.description ?? null,
    team_id:      params.teamId,
    status_id:    params.statusId ?? null,
    assignee_id:  params.assigneeId ?? null,
    label_ids:    params.labelIds ?? [],
    priority:     params.priority ?? null,
    project_id:   params.projectId ?? null,
    milestone_id: params.milestoneId ?? null,
    due_date:     params.dueDate ?? null,
    estimate:     params.estimate ?? null,
  });
}

// ── Jira ─────────────────────────────────────────────────────────────────────

export function jiraGetAuthStatus(): Promise<JiraAuthStatus> {
  return corvus('jira_get_auth_status');
}

export function jiraSearchIssues(filters: IssueFilters): Promise<Issue[]> {
  return corvus('jira_search_issues', { filters });
}

export function jiraGetIssue(id: string): Promise<Issue> {
  return corvus('jira_get_issue', { id });
}

export function jiraGetFilterOptions(): Promise<IssueFilterOptions> {
  return corvus('jira_get_filter_options');
}

export function jiraTransitionIssue(id: string, statusId: string): Promise<Issue> {
  return corvus('jira_transition_issue', { id, status_id: statusId });
}

export function jiraAssignIssue(id: string, userId: string | null): Promise<Issue> {
  return corvus('jira_assign_issue', { id, user_id: userId });
}

export function jiraAddComment(issueId: string, body: string): Promise<IssueComment> {
  return corvus('jira_add_comment', { issue_id: issueId, body });
}

export function jiraCreateIssue(params: {
  title:        string;
  description?: string;
  teamId:       string;
  statusId?:    string;
  assigneeId?:  string;
  labelIds?:    string[];
  priority?:    number;
  projectId?:   string;
  milestoneId?: string;
  dueDate?:     string;
  estimate?:    number;
  issueType?:   string;
}): Promise<Issue> {
  return corvus('jira_create_issue', {
    title:        params.title,
    description:  params.description ?? null,
    team_id:      params.teamId,
    status_id:    params.statusId ?? null,
    assignee_id:  params.assigneeId ?? null,
    label_ids:    params.labelIds ?? [],
    priority:     params.priority ?? null,
    project_id:   params.projectId ?? null,
    milestone_id: params.milestoneId ?? null,
    due_date:     params.dueDate ?? null,
    estimate:     params.estimate ?? null,
    issue_type:   params.issueType ?? null,
  });
}

/** Suggest a git branch name for an issue. Provider-agnostic — the backend
 *  helper produces `{lower-identifier}-{slugified-title}` for any tracker. */
export function branchNameForIssue(issue: Issue): Promise<string> {
  return corvus('branch_name_for_issue', { issue });
}

/** Download a Jira attachment to a path on disk (the user picks via save dialog).
 *  Returns the byte size written. The backend enforces that `contentUrl`'s host
 *  matches the configured Jira instance, so it can't be used as a generic proxy. */
export function jiraDownloadAttachment(contentUrl: string, destPath: string): Promise<number> {
  return corvus('jira_download_attachment', { content_url: contentUrl, dest_path: destPath });
}
