// ---------------------------------------------------------------------------
// Linked Worktrees management.
//
// All registry queries and mutations are served through the corvus broker —
// see `ipc/corvus/linked_worktree.rs`. Mutations save the registry to disk
// (~/.config/arbor/linked_worktrees.toml) and push
// `arbor://worktree-links-changed` through the event sink so any open
// WorktreeLinkManagerModal refreshes; the member add/remove mutations fire the
// `on_worktree_link_member_added` / `_removed` hooks from `post_hooks.rs`.
//
// Nothing routed through `#[tauri::command]` remains here.
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

use crate::linked_worktrees::AliasEntry;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasGroupInput {
    pub members: Vec<AliasEntry>,
}
