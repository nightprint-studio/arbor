// ---------------------------------------------------------------------------
// Linked Worktrees — cross-project sync of branch checkouts.
// Mirrors src-tauri/src/linked_worktrees/mod.rs.
// ---------------------------------------------------------------------------

export interface AliasEntry {
  repo_id: string;
  branch:  string;
}

export interface AliasGroup {
  id:      string;
  members: AliasEntry[];
}

export interface LinkMember {
  repo_id:      string;
  sync_enabled: boolean;
}

export interface SyncTarget {
  initiator_repo_id: string;
  branch:            string;
  timestamp:         number;
}

export interface WorktreeLink {
  id:                string;
  name:              string;
  sync_enabled:      boolean;
  members:           LinkMember[];
  alias_groups:      AliasGroup[];
  last_sync_target:  SyncTarget | null;
  created_at:        number;
}

// ── Sync results (from arbor://worktree-link-sync-done) ───────────────────────

export type MemberStatus =
  | { kind: 'updated';         branch: string }
  | { kind: 'skipped_missing'; branch: string }
  | { kind: 'conflict';        branch: string; files: string[] }
  | { kind: 'error';           message: string }
  | { kind: 'skipped';         reason: string };

export interface MemberResult {
  repo_id: string;
  status:  MemberStatus;
}

export interface SyncSummary {
  link_id:            string;
  link_name:          string;
  target_branch:      string;
  initiator_repo_id:  string;
  results:            MemberResult[];
}
