// ---------------------------------------------------------------------------
// Linked worktrees — **shell-side residue** after the full-move to `corvus-be`.
//
// The live registry, the CRUD handlers, the cross-repo checkout-sync
// orchestrator and the branch worktree-link handlers all moved out-of-process
// (`crates/corvus/be/src/{linked_worktree.rs, worktree_links/}`). corvus-be owns
// the registry, its serde types, and persists it to `linked_worktrees.toml`.
//
// What remains here is the single thing the **shell** still needs: the
// profile-aware path to that file, which the shell computes and pushes to
// corvus-be (a separate process that can't derive it) via the
// `worktree_links_path` config section.
// ---------------------------------------------------------------------------

use std::path::PathBuf;

pub fn links_file_path() -> PathBuf {
    arbor_core::prelude::product_path(arbor_core::prelude::PRODUCT_CORVUS, "linked_worktrees.toml")
}
