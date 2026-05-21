import { invoke } from '@tauri-apps/api/core';
import type {
  Issue, IssueComment, IssueFilterOptions, IssueFilters,
  IssueUser, LinearAuthStatus, JiraAuthStatus,
} from '$lib/types/issues';

export function linearGetAuthStatus(): Promise<LinearAuthStatus> {
  return invoke('linear_get_auth_status');
}

export function linearSaveToken(token: string): Promise<IssueUser> {
  return invoke('linear_save_token', { token });
}

export function linearLogout(): Promise<void> {
  return invoke('linear_logout');
}

export function linearSearchIssues(filters: IssueFilters): Promise<Issue[]> {
  return invoke('linear_search_issues', { filters });
}

export function linearGetIssue(id: string): Promise<Issue> {
  return invoke('linear_get_issue', { id });
}

export function linearGetFilterOptions(): Promise<IssueFilterOptions> {
  return invoke('linear_get_filter_options');
}

export function linearTransitionIssue(id: string, statusId: string): Promise<Issue> {
  return invoke('linear_transition_issue', { id, statusId });
}

export function linearAssignIssue(id: string, userId: string | null): Promise<Issue> {
  return invoke('linear_assign_issue', { id, userId });
}

export function linearAddComment(issueId: string, body: string): Promise<IssueComment> {
  return invoke('linear_add_comment', { issueId, body });
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
  return invoke('linear_create_issue', {
    title:       params.title,
    description: params.description ?? null,
    teamId:      params.teamId,
    statusId:    params.statusId ?? null,
    assigneeId:  params.assigneeId ?? null,
    labelIds:    params.labelIds ?? [],
    priority:    params.priority ?? null,
    projectId:   params.projectId ?? null,
    milestoneId: params.milestoneId ?? null,
    dueDate:     params.dueDate ?? null,
    estimate:    params.estimate ?? null,
  });
}

export function linearBranchNameForIssue(issue: Issue): Promise<string> {
  return invoke('linear_branch_name_for_issue', { issue });
}

// ── Jira ─────────────────────────────────────────────────────────────────────

export function jiraGetAuthStatus(): Promise<JiraAuthStatus> {
  return invoke('jira_get_auth_status');
}

export function jiraSaveBasicAuth(email: string, apiToken: string, domain: string): Promise<IssueUser> {
  return invoke('jira_save_basic_auth', { email, apiToken, domain });
}

export function jiraLogout(): Promise<void> {
  return invoke('jira_logout');
}

export function jiraSearchIssues(filters: IssueFilters): Promise<Issue[]> {
  return invoke('jira_search_issues', { filters });
}

export function jiraGetIssue(id: string): Promise<Issue> {
  return invoke('jira_get_issue', { id });
}

export function jiraGetFilterOptions(): Promise<IssueFilterOptions> {
  return invoke('jira_get_filter_options');
}

export function jiraTransitionIssue(id: string, statusId: string): Promise<Issue> {
  return invoke('jira_transition_issue', { id, statusId });
}

export function jiraAssignIssue(id: string, userId: string | null): Promise<Issue> {
  return invoke('jira_assign_issue', { id, userId });
}

export function jiraAddComment(issueId: string, body: string): Promise<IssueComment> {
  return invoke('jira_add_comment', { issueId, body });
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
  return invoke('jira_create_issue', {
    title:       params.title,
    description: params.description ?? null,
    teamId:      params.teamId,
    statusId:    params.statusId ?? null,
    assigneeId:  params.assigneeId ?? null,
    labelIds:    params.labelIds ?? [],
    priority:    params.priority ?? null,
    projectId:   params.projectId ?? null,
    milestoneId: params.milestoneId ?? null,
    dueDate:     params.dueDate ?? null,
    estimate:    params.estimate ?? null,
    issueType:   params.issueType ?? null,
  });
}

export function jiraBranchNameForIssue(issue: Issue): Promise<string> {
  return invoke('jira_branch_name_for_issue', { issue });
}

/** Download a Jira attachment to a path on disk (the user picks via save dialog).
 *  Returns the byte size written. The backend enforces that `contentUrl`'s host
 *  matches the configured Jira instance, so it can't be used as a generic proxy. */
export function jiraDownloadAttachment(contentUrl: string, destPath: string): Promise<number> {
  return invoke('jira_download_attachment', { contentUrl, destPath });
}
