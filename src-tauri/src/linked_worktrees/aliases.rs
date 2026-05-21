// ---------------------------------------------------------------------------
// Alias resolution + branch lifecycle handlers.
//
// resolve_target_branch(): given an initiator (repo_id, branch) and a target
// member's repo_id, returns the branch to check out on the target.
//
// on_branch_deleted(): remove alias entries that point to a deleted branch;
// drop alias groups that fall below 2 members.
//
// on_branch_renamed(): smart-update — rename the matching alias entry.  If
// after the rename all group members share the same branch name, the group
// becomes redundant and is removed.
// ---------------------------------------------------------------------------

use super::{AliasGroup, WorktreeLink};

/// Returns the branch the target member should be switched to.
///
/// 1. Look for an alias group containing `(initiator_repo_id, initiator_branch)`.
/// 2. If found and the group also contains an entry for `target_repo_id`, use
///    that branch.
/// 3. Otherwise fall back to the initiator's branch name.
pub fn resolve_target_branch(
    link: &WorktreeLink,
    initiator_repo_id: &str,
    initiator_branch: &str,
    target_repo_id: &str,
) -> String {
    let group = link.alias_groups.iter().find(|g| {
        g.members.iter().any(|e| e.repo_id == initiator_repo_id && e.branch == initiator_branch)
    });
    if let Some(g) = group {
        if let Some(e) = g.members.iter().find(|e| e.repo_id == target_repo_id) {
            return e.branch.clone();
        }
    }
    initiator_branch.to_string()
}

/// Returns the *expected* branch for a given member according to the link's
/// `last_sync_target`.  None if no sync has happened yet or the member isn't
/// in the link.
#[allow(dead_code)]
pub fn expected_branch_for_member(link: &WorktreeLink, repo_id: &str) -> Option<String> {
    let t = link.last_sync_target.as_ref()?;
    if t.initiator_repo_id == repo_id { return Some(t.branch.clone()); }
    Some(resolve_target_branch(link, &t.initiator_repo_id, &t.branch, repo_id))
}

/// Cleanup alias references when a branch is deleted.  Removes entries that
/// point to `(repo_id, branch)` across all links; groups falling below 2
/// members are dropped.  Returns the number of alias entries removed.
pub fn on_branch_deleted(links: &mut [WorktreeLink], repo_id: &str, branch: &str) -> usize {
    let mut removed = 0usize;
    for l in links.iter_mut() {
        for g in l.alias_groups.iter_mut() {
            let before = g.members.len();
            g.members.retain(|e| !(e.repo_id == repo_id && e.branch == branch));
            removed += before - g.members.len();
        }
        l.alias_groups.retain(|g| g.members.len() >= 2);
    }
    removed
}

/// Smart rename: update alias entries `(repo_id, old)` → `(repo_id, new)`.
/// If after the rename all members of a group share the same branch name the
/// group is redundant and gets removed.  Returns the number of groups
/// affected (renamed-in or collapsed).
pub fn on_branch_renamed(
    links: &mut [WorktreeLink],
    repo_id: &str,
    old_name: &str,
    new_name: &str,
) -> usize {
    let mut affected = 0usize;
    for l in links.iter_mut() {
        let mut to_drop: Vec<String> = Vec::new();
        for g in l.alias_groups.iter_mut() {
            let mut changed = false;
            for e in g.members.iter_mut() {
                if e.repo_id == repo_id && e.branch == old_name {
                    e.branch = new_name.to_string();
                    changed = true;
                }
            }
            if changed {
                affected += 1;
                if is_group_redundant(g) {
                    to_drop.push(g.id.clone());
                }
            }
        }
        l.alias_groups.retain(|g| !to_drop.contains(&g.id));
    }
    affected
}

/// True when every entry in the group references the same branch name —
/// in which case the alias mapping is a no-op and can be removed.
fn is_group_redundant(g: &AliasGroup) -> bool {
    if g.members.is_empty() { return true; }
    let first = &g.members[0].branch;
    g.members.iter().all(|e| &e.branch == first)
}

/// Returns the link name if creating `(repo_id, branch)` would conflict with
/// an existing alias group target.  Used by `create_branch` to refuse names
/// that are reserved by an alias mapping (the user must remove or change the
/// alias first).
pub fn alias_blocks_branch_name(links: &[WorktreeLink], repo_id: &str, branch: &str) -> Option<String> {
    for l in links {
        for g in &l.alias_groups {
            let has_repo = g.members.iter().any(|e| e.repo_id == repo_id);
            let same_name = g.members.iter().any(|e| e.repo_id == repo_id && e.branch == branch);
            let other_targets_this_name = g.members.iter().any(|e| e.repo_id != repo_id && e.branch == branch);
            if has_repo && other_targets_this_name && !same_name {
                return Some(l.name.clone());
            }
        }
    }
    None
}
