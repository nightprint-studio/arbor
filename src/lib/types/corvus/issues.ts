// Mirror of src-tauri/src/integrations/mod.rs

/** How a free-text body should be rendered.
 *  - "markdown": Linear (native), Jira fallback when no rendered HTML available.
 *  - "html":     Jira `renderedFields.*` (server-rendered + sanitized).
 */
export type BodyFormat = 'markdown' | 'html';

export interface IssueUser {
  id:          string;
  name:        string;
  displayName: string;
  avatarUrl:   string | null;
  email:       string | null;
}

export interface IssueStatus {
  id:         string;
  name:       string;
  color:      string;
  /** "backlog" | "unstarted" | "started" | "completed" | "cancelled" */
  statusType: string;
}

export interface IssueLabel {
  id:    string;
  name:  string;
  color: string;
}

export interface IssueTeam {
  id:   string;
  name: string;
  key:  string;
}

export interface IssueProject {
  id:    string;
  name:  string;
  color: string | null;
}

export interface IssueCycle {
  id:     string;
  name:   string;
  number: number;
}

export interface IssueMilestone {
  id:          string;
  name:        string;
  targetDate:  string | null;
  projectId:   string | null;
  projectName: string | null;
}

export interface IssueComment {
  id:         string;
  body:       string;
  bodyFormat: BodyFormat;
  createdAt:  string;
  user:       IssueUser | null;
}

export interface IssueAttachment {
  id:           string;
  filename:     string;
  mimeType:     string | null;
  size:         number | null;
  /** Authenticated download URL — never fetched directly from the frontend;
   *  always pass through `downloadJiraAttachment(...)`. */
  contentUrl:   string;
  thumbnailUrl: string | null;
  createdAt:    string | null;
  author:       IssueUser | null;
}

export interface Issue {
  id:                 string;
  identifier:         string;
  title:              string;
  description:        string | null;
  descriptionFormat:  BodyFormat;
  status:             IssueStatus;
  priority:      number;
  priorityLabel: string;
  assignee:      IssueUser | null;
  labels:        IssueLabel[];
  url:           string;
  createdAt:     string;
  updatedAt:     string;
  dueDate:       string | null;
  estimate:      number | null;
  team:          IssueTeam | null;
  project:       IssueProject | null;
  cycle:         IssueCycle | null;
  comments:      IssueComment[];
  commentCount:  number;
  attachments:   IssueAttachment[];
}

export type IssueSortField = 'ticket_id' | 'updated_at' | 'created_at' | 'priority' | 'title' | 'status';
export type IssueSortDir   = 'asc' | 'desc';

export const SORT_FIELD_LABELS: Record<IssueSortField, string> = {
  ticket_id:  'Ticket ID',
  updated_at: 'Last updated',
  created_at: 'Created',
  priority:   'Priority',
  title:      'Title',
  status:     'Status',
};

export interface IssueFilters {
  query?:         string;
  statusIds?:     string[];
  labelIds?:      string[];
  issueTypeIds?:  string[];
  teamId?:        string;
  projectId?:     string;
  milestoneId?:   string;
  cycleId?:       string;
  assigneeMe?:    boolean;
  limit?:         number;
}

export interface IssueFilterOptions {
  teams:       IssueTeam[];
  statuses:    IssueStatus[];
  labels:      IssueLabel[];
  issueTypes:  IssueLabel[];
  projects:    IssueProject[];
  cycles:      IssueCycle[];
  milestones:  IssueMilestone[];
  me:          IssueUser | null;
}

export interface LinearAuthStatus {
  authenticated: boolean;
  user:          IssueUser | null;
}

export interface JiraAuthStatus {
  authenticated: boolean;
  user:          IssueUser | null;
  /** e.g. "mycompany.atlassian.net" */
  domain:        string | null;
  /** "oauth" | "basic" */
  authMethod:    string | null;
}

/** Provider-neutral auth status shape used by the issues store. */
export interface IssueAuthStatus {
  authenticated: boolean;
  user:          IssueUser | null;
  domain?:       string | null;
  authMethod?:   string | null;
}
