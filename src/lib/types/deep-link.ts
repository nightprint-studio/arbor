// Mirror of `src-tauri/src/deep_link.rs` types.

export type CrossWorkspaceStrategy = 'switch' | 'open_here';

/** Per-action confirm-modal toggles.  Defaults to true for every entry —
 *  the rule is "ask before doing anything", with per-action opt-out. */
export interface ConfirmConfig {
  repo_open:       boolean;
  commit_jump:     boolean;
  branch_checkout: boolean;
  branch_worktree: boolean;
  mr_open:         boolean;
  pipeline_open:   boolean;
}

/** Per-action enable toggles.  All default to false — even after flipping
 *  the master `enabled`, the user opts in each action kind individually. */
export interface EnableConfig {
  repo_open:       boolean;
  commit_jump:     boolean;
  branch_checkout: boolean;
  branch_worktree: boolean;
  mr_open:         boolean;
  pipeline_open:   boolean;
}

export interface DeepLinkConfig {
  /** Master kill-switch — false short-circuits to the disabled modal. */
  enabled:              boolean;
  enable:               EnableConfig;
  cross_workspace_strategy: CrossWorkspaceStrategy;
  confirm:              ConfirmConfig;
  /** Rewrite `arbor://branch/<name>?checkout=1` to the worktree variant
   *  before dispatch. Default: false (link's literal intent wins). */
  checkout_as_worktree: boolean;
  /** Host (domain only, no scheme / trailing slash) of the HTTPS redirect
   *  worker used when copying shareable deep links — chats like Google
   *  Chat / Slack don't render `arbor://…` as clickable. Empty string
   *  falls back to raw `arbor://` URLs. */
  worker_base_url: string;
}

/**
 * Outcome of `find_repo_by_remote_url` — see `commands/deep_link_commands.rs`.
 *
 *   * `repo_id == null`  → no registry entry matches the incoming URL.  The
 *                          dispatcher should prompt the user to clone.
 *   * `workspace_ids` is in user-defined order (Scratch always last); the
 *                     dispatcher's first choice is the head of this list when
 *                     the active workspace isn't a member.
 */
export interface DeepLinkLookup {
  repo_id:               string | null;
  repo_path:             string | null;
  display_name:          string | null;
  workspace_ids:         string[];
  in_active_workspace:   boolean;
  active_workspace_id:   string | null;
}
