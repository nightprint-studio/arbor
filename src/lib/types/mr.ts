// TypeScript mirrors of Rust mr/mod.rs types — keep in sync.

export type MrState = 'open' | 'closed' | 'merged';

export interface MrUser {
  login:       string;
  displayName: string;
  avatarUrl:   string | null;
}

export interface MrLabel {
  name:  string;
  color: string; // hex without #
}

export interface MrCheck {
  name:   string;
  status: 'pending' | 'running' | 'success' | 'failed' | 'cancelled' | 'skipped';
  url:    string | null;
}

export interface MrComment {
  id:        string;
  author:    MrUser;
  body:      string;
  createdAt: string;
  /** Heuristic: backend tagged the author as a bot account
   *  (GitHub `[bot]` suffix / GitLab name containing "bot"). */
  isBot:     boolean;
}

/** Timeline event (state change, label edit, assignment, force-push, …).
 *  Empty `events` arrays mean the provider didn't return any — the
 *  frontend hides the Activity filter chip in that case. */
export interface MrEvent {
  id:        string;
  /** Coarse category — drives the icon and filter group on the frontend.
   *  "state" | "label" | "assign" | "review" | "commit" | "rename" | "system" */
  kind:      string;
  actor:     MrUser;
  /** Pre-rendered, human-readable summary. May contain `**bold**` markdown. */
  summary:   string;
  createdAt: string;
}

export interface MergeRequest {
  number:        number;
  title:         string;
  description:   string;
  state:         MrState;
  isDraft:       boolean;
  author:        MrUser;
  sourceBranch:  string;
  targetBranch:  string;
  webUrl:        string;
  createdAt:     string;
  updatedAt:     string;
  labels:        MrLabel[];
  assignees:     MrUser[];
  reviewers:     MrUser[];
  checksStatus:  'pending' | 'success' | 'failed' | 'none';
  mergeable:     boolean | null;
  provider:      'github' | 'gitlab';
  commentsCount: number;
  squash:        boolean;
  deleteBranch:  boolean;
  /** SHA of the commit created on the target branch when this MR/PR was merged. */
  mergeCommitSha?: string;
  /** SHA of the source branch tip at the time of merge. */
  headSha: string;
  /** SHA of the target branch tip just before the merge. */
  baseSha: string;
  /** Auto-merge / merge-when-pipeline-succeeds is currently armed on the
   *  upstream MR. When true, the detail modal hides the manual merge controls
   *  and offers a "Disable auto-merge" action instead. */
  autoMergeEnabled: boolean;
}

export interface MergedMrHint {
  sourceBranch:   string;
  mergeCommitSha: string;
  /** SHA of the source branch tip at the time of merge. */
  headSha:        string;
  /** SHA of the target branch tip just before the merge. */
  baseSha:        string;
}

export interface MrFileDiff {
  filename:  string;
  status:    'added' | 'modified' | 'removed' | 'renamed' | string;
  additions: number;
  deletions: number;
  patch:     string | null;
}

export interface MrCommit {
  sha:     string;
  message: string;
  author:  string;
  date:    string;
  webUrl:  string | null;
}

export interface MrDetail {
  mr:       MergeRequest;
  comments: MrComment[];
  /** Activity timeline (state changes, labels, assignments, …). */
  events:   MrEvent[];
  checks:   MrCheck[];
}

/** Per-repo capability hints surfaced by `get_mr_capabilities`. Used by the
 *  Create MR modal to disable options the upstream provider would reject. */
export interface MrCapabilities {
  /** `true` when arming auto-merge / MWPS at creation is expected to work.
   *  Defaults to `true` on probe failure so the user can still try. */
  autoMergeSupported: boolean;
  /** Tooltip explaining why auto-merge is disabled, when supported = false. */
  autoMergeReason:    string | null;
}

export interface CreateMrParams {
  title:         string;
  description:   string | null;
  sourceBranch:  string;
  targetBranch:  string;
  isDraft:       boolean;
  labels:        string[];
  /** Squash commits on merge — kept for backwards compat, typically false now
   *  since squash/delete-branch are chosen at merge-time from the detail view. */
  squash:        boolean;
  /** Delete source branch after merge — kept for backwards compat (see `squash`). */
  deleteBranch:  boolean;
  /** Arm auto-merge on creation (GitHub auto-merge / GitLab merge-when-pipeline-succeeds).
   *  Failures surface as an error notification; the MR itself is still created. */
  autoMerge:     boolean;
}
