use tauri::State;

use crate::error::AppError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Commit
// ---------------------------------------------------------------------------
//
// DEFERRED from the corvus `stage` migration: `commit` fires the **vetoable**
// `on_pre_commit` hook (a non-empty plugin return aborts the commit) and the
// `on_commit` hook. The plugin-veto round-trip seam for the broker is not yet
// built, so this stays an inline `#[tauri::command]` until that pass. Every
// pure index op (stage/unstage/discard/patch, cherry-pick/revert, commit
// template) has moved to `crate::ipc::corvus::stage`.

#[tauri::command]
pub fn commit(
    state: State<'_, AppState>,
    tab_id: String,
    message: String,
    amend: bool,
) -> Result<String, AppError> {
    // ── Pre-commit veto ────────────────────────────────────────────────
    // Plugins subscribed to `on_pre_commit` may reject the commit by
    // returning a non-empty string from their handler. The dispatcher
    // short-circuits at the first plugin that vetoes and hands back a
    // `"<plugin>: <reason>"` string (or `"<plugin>: blocked"` for an
    // empty reason), which we surface to the user.
    if let Some(reason) = state.hook_dispatcher.fire_vetoable_blocking(
        "on_pre_commit",
        arbor_plugin_api::prelude::PluginValue::from_json(serde_json::json!({
            "tab_id":  &tab_id,
            "message": &message,
            "amend":   amend,
        })),
    ) {
        return Err(AppError::Other(format!("Commit blocked by plugin:\n{reason}")));
    }

    // Scope the repos lock so it is released before firing plugin hooks
    // (Lua hooks may call git operations which would deadlock if the lock is held).
    let oid = {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let r = repo.inner_mut();

    let sig = r.signature()?;
    let mut index = r.index()?;
    let tree_oid = index.write_tree()?;
    let tree = r.find_tree(tree_oid)?;

    if amend {
        // Use find_commit(revparse id) to avoid the peel_to_commit libgit2 bug.
        let head_oid = r.revparse_single("HEAD")
            .map_err(|_| AppError::Other("amend failed: no HEAD commit found".into()))?
            .id();
        let head_commit = r.find_commit(head_oid)?;
        let oid = head_commit.amend(
            Some("HEAD"),
            Some(&sig),
            Some(&sig),
            None,
            Some(&message),
            Some(&tree),
        )?;
        oid.to_string()
    } else {
        let parent_commits: Vec<git2::Commit<'_>> = match r.revparse_single("HEAD") {
            Ok(obj) => vec![r.find_commit(obj.id())?],
            Err(_) => vec![], // initial commit — no parent
        };
        let parents: Vec<&git2::Commit<'_>> = parent_commits.iter().collect();
        let oid = r.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)?;
        oid.to_string()
    }
    }; // repos lock released here

    state.fire_hook(
        "on_commit",
        serde_json::json!({
            "tab_id":  &tab_id,
            "oid":     &oid,
            "message": &message,
            "amend":   amend,
        }),
    );
    Ok(oid)
}
