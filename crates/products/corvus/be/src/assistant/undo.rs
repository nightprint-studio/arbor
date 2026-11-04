//! What Imber did, and taking it back where git lets us.
//!
//! Every change Imber makes to a repository is written to a log (`imber-actions.json`, newest last). Each
//! entry says what it did and — when it can be reversed — *how*, as a small [`UndoOp`] worked out from the
//! repository's state **before** the action. "Undo Imber's last action" runs the newest reversible entry's
//! operation, after checking that the repository is still the way the action left it (HEAD where it left
//! it, a clean tree where a hard reset needs one): undoing on top of something the user did since would
//! destroy that.
//!
//! **What can be undone** — a commit (soft reset to where HEAD was), a branch created or switched to (back to
//! the previous branch, then the new one deleted), a stage / unstage (the index put back), a merge or pull
//! that moved HEAD (hard reset to where it was, with the safety snapshot every hard reset takes), a deleted
//! branch (recreated at its old tip).
//!
//! **What cannot** — a push, a merge request opened, a plan run: they left the machine. They are logged
//! all the same, with the reason, so the list is honest about what "undo" does not cover. Undoing a commit
//! that was pushed afterwards works locally and says the remote still has it.
//!
//! The decisions here are pure ([`check`], [`pick_target`], [`staged_delta`], [`op_for`]); only
//! [`facts`] and [`perform`] touch the repository.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{get_status, ResetMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ephemeral::tab_for;
use crate::repo::open;

/// Entries kept; the oldest go first.
const MAX_ACTIONS: usize = 200;

/// How to take an action back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum UndoOp {
    /// A commit: move HEAD back to `to`, keeping the changes staged. Only while HEAD is still `expect_head`.
    ResetSoft { to: String, expect_head: String },
    /// A merge or pull that moved HEAD: hard reset to `to`. Needs a clean tree and HEAD still at `expect_head`.
    ResetHard { to: String, expect_head: String },
    /// A branch Imber created: delete it (after going back to `return_to` if Imber also switched to it).
    DeleteBranch { name: String, return_to: Option<String> },
    /// A branch switch: go back.
    Checkout { branch: String },
    /// A branch Imber deleted: recreate it at its old tip.
    RecreateBranch { name: String, oid: String },
    /// A stage or unstage: put the index back to exactly `before`.
    RestoreStaged { before: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub at_ms: i64,
    pub root: String,
    pub tool: String,
    /// One line for the list: `Committed "fix: totals"`.
    pub summary: String,
    pub undo: Option<UndoOp>,
    /// Why it cannot be undone, when `undo` is `None`.
    pub irreversible: Option<String>,
    pub undone_ms: Option<i64>,
}

/// The repository as it is, for deciding whether an undo is still safe.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Facts {
    pub head_oid: Option<String>,
    pub head_branch: Option<String>,
    /// No staged, unstaged or conflicted changes (untracked files do not count: a hard reset leaves them).
    pub clean: bool,
    pub staged: Vec<String>,
    /// `(name, tip)` of every local branch.
    pub branches: Vec<(String, String)>,
}

// ── Pure decisions ───────────────────────────────────────────────────────────────────────────────

/// Why an action cannot be undone, for the tools that leave the machine (or `None` for the ones that do not).
pub fn irreversible_reason(tool: &str, input: &Value) -> Option<String> {
    let remote = input.get("remote").and_then(Value::as_str).unwrap_or("origin");
    match tool {
        "git_push" => Some(format!("It was pushed to {remote}: others may already have it. Undoing it would need a force push, which Imber does not do on its own.")),
        "git_mr_create" => Some("The merge request was opened on the hosting provider. Close it there.".into()),
        "plan_run" => Some("A plan's steps run on the remote (push, merge requests, tags, pipelines). Stop it with plan_stop; what already ran stays.".into()),
        _ => None,
    }
}

/// Whether a tool's effect is written to the log (reversible or not): the writes that change the repository
/// or leave the machine. Reads, fetches, and the bookkeeping tools (schedules, plan edits) are not.
pub fn tracks(tool: &str) -> bool {
    matches!(
        tool,
        "git_commit" | "git_stage" | "git_unstage" | "git_checkout" | "git_branch_create" | "git_branch_delete" | "git_merge" | "git_pull" | "git_push" | "git_mr_create" | "plan_run"
    )
}

/// The undo for `tool` given the repository `before` and `after` it ran. `None` when nothing changed or
/// git offers no way back (the first commit of a repository, a detached HEAD).
pub fn op_for(tool: &str, input: &Value, before: &Facts, after: &Facts) -> Option<UndoOp> {
    let str_of = |k: &str| input.get(k).and_then(Value::as_str).map(str::to_string);
    let moved = before.head_oid != after.head_oid;
    match tool {
        "git_commit" if moved => Some(UndoOp::ResetSoft { to: before.head_oid.clone()?, expect_head: after.head_oid.clone()? }),
        "git_merge" | "git_pull" if moved => Some(UndoOp::ResetHard { to: before.head_oid.clone()?, expect_head: after.head_oid.clone()? }),
        "git_stage" | "git_unstage" if before.staged != after.staged => Some(UndoOp::RestoreStaged { before: before.staged.clone() }),
        "git_checkout" if before.head_branch != after.head_branch => Some(UndoOp::Checkout { branch: before.head_branch.clone()? }),
        "git_branch_create" => {
            let name = str_of("name")?;
            let switched = input.get("checkout").and_then(Value::as_bool).unwrap_or(false);
            Some(UndoOp::DeleteBranch { name, return_to: if switched { before.head_branch.clone() } else { None } })
        }
        "git_branch_delete" => {
            let name = str_of("name")?;
            let oid = before.branches.iter().find(|(n, _)| *n == name)?.1.clone();
            Some(UndoOp::RecreateBranch { name, oid })
        }
        _ => None,
    }
}

/// What the entry says in the list.
pub fn summarize(tool: &str, input: &Value) -> String {
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let first_line = |t: String| t.lines().next().unwrap_or("").chars().take(70).collect::<String>();
    match tool {
        "git_commit" => format!("Committed “{}”", first_line(s("message"))),
        "git_stage" => "Staged files".into(),
        "git_unstage" => "Unstaged files".into(),
        "git_checkout" => format!("Switched to {}", s("branch")),
        "git_branch_create" => format!("Created branch {}", s("name")),
        "git_branch_delete" => format!("Deleted branch {}", s("name")),
        "git_merge" => format!("Merged {}", s("branch")),
        "git_pull" => "Pulled".into(),
        "git_push" => format!("Pushed to {}", input.get("remote").and_then(Value::as_str).unwrap_or("origin")),
        "git_mr_create" => format!("Opened merge request “{}”", first_line(s("title"))),
        "plan_run" => "Started a plan".into(),
        other => other.to_string(),
    }
}

/// What an undo does, in a line — shown before it is done.
pub fn describe(op: &UndoOp) -> String {
    let short = |o: &str| o.chars().take(8).collect::<String>();
    match op {
        UndoOp::ResetSoft { to, .. } => format!("move HEAD back to {} (the changes stay staged)", short(to)),
        UndoOp::ResetHard { to, .. } => format!("reset the branch to {} (a recovery snapshot is taken first)", short(to)),
        UndoOp::DeleteBranch { name, return_to: Some(back) } => format!("switch back to {back}, then delete the branch {name}"),
        UndoOp::DeleteBranch { name, return_to: None } => format!("delete the branch {name}"),
        UndoOp::Checkout { branch } => format!("switch back to {branch}"),
        UndoOp::RecreateBranch { name, oid } => format!("recreate the branch {name} at {}", short(oid)),
        UndoOp::RestoreStaged { before } => format!("put the index back ({} file(s) staged)", before.len()),
    }
}

/// Whether `op` is still safe to run against the repository as it is now.
pub fn check(op: &UndoOp, now: &Facts) -> Result<(), String> {
    let head_is = |oid: &str| now.head_oid.as_deref() == Some(oid);
    match op {
        UndoOp::ResetSoft { expect_head, .. } => {
            if head_is(expect_head) { Ok(()) } else { Err("HEAD has moved since Imber committed, so undoing would also drop what came after. Undo it by hand in Corvus.".into()) }
        }
        UndoOp::ResetHard { expect_head, .. } => {
            if !head_is(expect_head) {
                Err("HEAD has moved since that merge or pull, so undoing would also drop what came after.".into())
            } else if !now.clean {
                Err("There are uncommitted changes, and a hard reset would throw them away. Commit or stash them first.".into())
            } else {
                Ok(())
            }
        }
        UndoOp::DeleteBranch { name, return_to } => {
            if !now.branches.iter().any(|(n, _)| n == name) {
                Err(format!("The branch {name} is already gone."))
            } else if now.head_branch.as_deref() == Some(name) && return_to.is_none() {
                Err(format!("You are on {name}; switch away from it before undoing its creation."))
            } else {
                Ok(())
            }
        }
        UndoOp::Checkout { branch } => {
            if now.branches.iter().any(|(n, _)| n == branch) { Ok(()) } else { Err(format!("The branch {branch} no longer exists.")) }
        }
        UndoOp::RecreateBranch { name, .. } => {
            if now.branches.iter().any(|(n, _)| n == name) { Err(format!("A branch called {name} exists again.")) } else { Ok(()) }
        }
        UndoOp::RestoreStaged { .. } => Ok(()),
    }
}

/// `(to_unstage, to_stage)` that turn the index from `now` into `before`.
pub fn staged_delta(before: &[String], now: &[String]) -> (Vec<String>, Vec<String>) {
    let unstage = now.iter().filter(|p| !before.contains(p)).cloned().collect();
    let stage = before.iter().filter(|p| !now.contains(p)).cloned().collect();
    (unstage, stage)
}

/// The entry an undo applies to: the given one, or the newest reversible one not yet undone for `root`. The
/// target must be the newest such entry — taking back an older action while a newer reversible one stands
/// would undo it out of order.
pub fn pick_target<'a>(actions: &'a [Action], root: &str, id: Option<&str>) -> Result<&'a Action, String> {
    let live = |a: &&Action| a.root == root && a.undone_ms.is_none() && a.undo.is_some();
    let newest = actions.iter().rev().find(live).ok_or("Imber has nothing to undo in this repository.")?;
    match id {
        None => Ok(newest),
        Some(id) => {
            let wanted = actions.iter().find(|a| a.id == id && a.root == root).ok_or_else(|| format!("No Imber action '{id}' in this repository."))?;
            if wanted.undone_ms.is_some() {
                Err("That action was already undone.".into())
            } else if wanted.undo.is_none() {
                Err(wanted.irreversible.clone().unwrap_or_else(|| "That action cannot be undone.".into()))
            } else if wanted.id != newest.id {
                Err(format!("A later action has to be undone first: {}.", newest.summary))
            } else {
                Ok(wanted)
            }
        }
    }
}

/// Irreversible actions that came *after* `target` in the same repository — what the undo cannot reach.
pub fn later_irreversible<'a>(actions: &'a [Action], target: &Action) -> Vec<&'a Action> {
    actions.iter().filter(|a| a.root == target.root && a.at_ms >= target.at_ms && a.id != target.id && a.undo.is_none() && a.irreversible.is_some() && a.undone_ms.is_none()).collect()
}

// ── The log ──────────────────────────────────────────────────────────────────────────────────────

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[derive(Default)]
struct Log {
    loaded: bool,
    actions: Vec<Action>,
}

fn log() -> MutexGuard<'static, Log> {
    static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
    LOG.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner())
}

fn file(state: &CorvusState) -> Result<PathBuf, String> {
    Ok(PathBuf::from(crate::corvus_config::corvus_config_dir(state)?).join("imber-actions.json"))
}

pub fn parse(text: &str) -> Vec<Action> {
    serde_json::from_str(text).unwrap_or_default()
}

fn load(state: &CorvusState, l: &mut Log) {
    if !l.loaded {
        l.loaded = true;
        if let Some(text) = file(state).ok().and_then(|p| std::fs::read_to_string(p).ok()) {
            l.actions = parse(&text);
        }
    }
}

fn save(state: &CorvusState, l: &Log) {
    let Ok(path) = file(state) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(&l.actions) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// Append an entry; the log is capped and saved.
pub fn record(state: &CorvusState, root: &str, tool: &str, input: &Value, op: Option<UndoOp>) {
    let irreversible = if op.is_none() { irreversible_reason(tool, input) } else { None };
    // An action that changed nothing and has no reason to be remembered (a no-op stage) is not logged.
    if op.is_none() && irreversible.is_none() {
        return;
    }
    let mut l = log();
    load(state, &mut l);
    let at_ms = now_ms();
    let id = format!("act-{at_ms:x}-{}", l.actions.len());
    l.actions.push(Action { id, at_ms, root: root.to_string(), tool: tool.to_string(), summary: summarize(tool, input), undo: op, irreversible, undone_ms: None });
    let excess = l.actions.len().saturating_sub(MAX_ACTIONS);
    l.actions.drain(..excess);
    save(state, &l);
}

/// This repository's actions, newest first.
pub fn list(state: &CorvusState, root: &str) -> Vec<Action> {
    let mut l = log();
    load(state, &mut l);
    l.actions.iter().rev().filter(|a| a.root == root).cloned().collect()
}

// ── The repository ───────────────────────────────────────────────────────────────────────────────

pub fn facts(state: &CorvusState, root: &str) -> Result<Facts, String> {
    let tab = tab_for(state, root)?;
    let repo = open(state, &tab)?;
    let status = get_status(&repo).map_err(|e| e.to_string())?;
    let branches = crate::branch::list_local_branches(state, tab)?.into_iter().map(|b| (b.name, b.head_oid)).collect();
    Ok(Facts {
        head_oid: status.head_oid,
        head_branch: status.current_branch,
        clean: status.staged.is_empty() && status.unstaged.is_empty() && status.conflicted.is_empty(),
        staged: status.staged.into_iter().map(|e| e.path).collect(),
        branches,
    })
}

fn perform(state: &CorvusState, root: &str, op: &UndoOp, now: &Facts) -> Result<(), String> {
    let tab = tab_for(state, root)?;
    match op {
        UndoOp::ResetSoft { to, .. } => crate::reset::reset_to_commit(state, tab, to.clone(), ResetMode::Soft),
        UndoOp::ResetHard { to, .. } => crate::reset::reset_to_commit(state, tab, to.clone(), ResetMode::Hard),
        UndoOp::DeleteBranch { name, return_to } => {
            if let Some(back) = return_to {
                crate::branch::checkout_branch_safe(state, tab.clone(), back.clone())?;
            }
            crate::branch::delete_branch(state, tab, name.clone())
        }
        UndoOp::Checkout { branch } => crate::branch::checkout_branch_safe(state, tab, branch.clone()).map(|_| ()),
        UndoOp::RecreateBranch { name, oid } => crate::branch::create_branch(state, tab, name.clone(), oid.clone()).map(|_| ()),
        UndoOp::RestoreStaged { before } => {
            let (unstage, stage) = staged_delta(before, &now.staged);
            if !unstage.is_empty() {
                crate::mcp_tools::corvus_unstage(state, crate::mcp_tools::StageArgs { root: root.into(), paths: unstage })?;
            }
            if !stage.is_empty() {
                crate::mcp_tools::corvus_stage(state, crate::mcp_tools::StageArgs { root: root.into(), paths: stage })?;
            }
            Ok(())
        }
    }
}

/// Undo `id`, or the newest reversible action of `root`. Returns what was done, with a note about anything
/// later that cannot be taken back.
pub fn undo(state: &CorvusState, root: &str, id: Option<&str>) -> Result<String, String> {
    let (target, later) = {
        let mut l = log();
        load(state, &mut l);
        let target = pick_target(&l.actions, root, id)?.clone();
        let later: Vec<String> = later_irreversible(&l.actions, &target).iter().map(|a| a.summary.clone()).collect();
        (target, later)
    };
    let op = target.undo.clone().ok_or("That action cannot be undone.")?;
    let now = facts(state, root)?;
    check(&op, &now)?;
    perform(state, root, &op, &now)?;
    {
        let mut l = log();
        if let Some(a) = l.actions.iter_mut().find(|a| a.id == target.id) {
            a.undone_ms = Some(now_ms());
        }
        save(state, &l);
    }
    let mut message = format!("Undone: {} — {}.", target.summary, describe(&op));
    if !later.is_empty() {
        message.push_str(&format!(" Not undone, because it left the machine: {}.", later.join("; ")));
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn facts_at(head: &str, branch: &str, staged: &[&str]) -> Facts {
        Facts {
            head_oid: Some(head.into()),
            head_branch: Some(branch.into()),
            clean: staged.is_empty(),
            staged: staged.iter().map(|s| s.to_string()).collect(),
            branches: vec![("main".into(), "a1".into()), (branch.into(), head.into())],
        }
    }

    fn action(id: &str, at: i64, tool: &str, undo: Option<UndoOp>) -> Action {
        let irreversible = if undo.is_none() { Some("left the machine".to_string()) } else { None };
        Action { id: id.into(), at_ms: at, root: "/r".into(), tool: tool.into(), summary: id.into(), undo, irreversible, undone_ms: None }
    }

    #[test]
    fn a_commit_is_undone_by_moving_head_back() {
        let op = op_for("git_commit", &json!({"message": "x"}), &facts_at("c1", "main", &["a.rs"]), &facts_at("c2", "main", &[]));
        assert_eq!(op, Some(UndoOp::ResetSoft { to: "c1".into(), expect_head: "c2".into() }));
        let nothing = op_for("git_commit", &json!({}), &facts_at("c1", "main", &[]), &facts_at("c1", "main", &[]));
        assert_eq!(nothing, None, "HEAD did not move: nothing to undo");
        let first = Facts { head_oid: None, ..facts_at("x", "main", &[]) };
        assert_eq!(op_for("git_commit", &json!({}), &first, &facts_at("c1", "main", &[])), None, "the first commit has nothing to go back to");
    }

    #[test]
    fn a_branch_created_and_switched_to_is_undone_by_going_back_and_deleting() {
        let before = facts_at("c1", "main", &[]);
        let after = facts_at("c1", "feature/x", &[]);
        assert_eq!(op_for("git_branch_create", &json!({"name": "feature/x", "checkout": true}), &before, &after), Some(UndoOp::DeleteBranch { name: "feature/x".into(), return_to: Some("main".into()) }));
        assert_eq!(op_for("git_branch_create", &json!({"name": "feature/x"}), &before, &before), Some(UndoOp::DeleteBranch { name: "feature/x".into(), return_to: None }));
        assert_eq!(op_for("git_checkout", &json!({"branch": "dev"}), &before, &facts_at("c1", "dev", &[])), Some(UndoOp::Checkout { branch: "main".into() }));
    }

    #[test]
    fn a_deleted_branch_is_recreated_at_its_old_tip() {
        let before = facts_at("c9", "main", &[]);
        let mut with = before.clone();
        with.branches.push(("old".into(), "dead".into()));
        assert_eq!(op_for("git_branch_delete", &json!({"name": "old"}), &with, &before), Some(UndoOp::RecreateBranch { name: "old".into(), oid: "dead".into() }));
        assert_eq!(op_for("git_branch_delete", &json!({"name": "ghost"}), &before, &before), None);
    }

    #[test]
    fn merge_and_pull_that_moved_head_are_undone_by_a_hard_reset_to_where_it_was() {
        let op = op_for("git_merge", &json!({"branch": "dev"}), &facts_at("c1", "main", &[]), &facts_at("m1", "main", &[]));
        assert_eq!(op, Some(UndoOp::ResetHard { to: "c1".into(), expect_head: "m1".into() }));
        assert_eq!(op_for("git_pull", &json!({}), &facts_at("c1", "main", &[]), &facts_at("c1", "main", &[])), None, "already up to date");
    }

    #[test]
    fn staging_is_undone_by_putting_the_index_back() {
        let op = op_for("git_stage", &json!({}), &facts_at("c1", "main", &["a.rs"]), &facts_at("c1", "main", &["a.rs", "b.rs"]));
        assert_eq!(op, Some(UndoOp::RestoreStaged { before: vec!["a.rs".into()] }));
        assert_eq!(staged_delta(&["a.rs".into()], &["a.rs".into(), "b.rs".into()]), (vec!["b.rs".to_string()], vec![]));
        assert_eq!(staged_delta(&["a.rs".into(), "c.rs".into()], &["a.rs".into()]), (vec![], vec!["c.rs".to_string()]));
    }

    #[test]
    fn what_leaves_the_machine_says_why_it_cannot_be_undone() {
        assert!(irreversible_reason("git_push", &json!({"remote": "upstream"})).unwrap().contains("upstream"));
        assert!(irreversible_reason("git_mr_create", &json!({})).unwrap().contains("hosting provider"));
        assert!(irreversible_reason("plan_run", &json!({})).unwrap().contains("plan_stop"));
        assert_eq!(irreversible_reason("git_commit", &json!({})), None);
    }

    #[test]
    fn undo_is_refused_when_the_repository_moved_on() {
        let soft = UndoOp::ResetSoft { to: "c1".into(), expect_head: "c2".into() };
        assert!(check(&soft, &facts_at("c2", "main", &[])).is_ok());
        assert!(check(&soft, &facts_at("c3", "main", &[])).unwrap_err().contains("HEAD has moved"));

        let hard = UndoOp::ResetHard { to: "c1".into(), expect_head: "m1".into() };
        assert!(check(&hard, &facts_at("m1", "main", &[])).is_ok());
        assert!(check(&hard, &facts_at("m1", "main", &["dirty.rs"])).unwrap_err().contains("uncommitted"), "a hard reset would eat them");
        assert!(check(&hard, &facts_at("zz", "main", &[])).is_err());

        let del = UndoOp::DeleteBranch { name: "feature/x".into(), return_to: None };
        assert!(check(&del, &facts_at("c1", "feature/x", &[])).unwrap_err().contains("switch away"));
        assert!(check(&del, &facts_at("c1", "main", &[])).is_err(), "already gone");
        let recreate = UndoOp::RecreateBranch { name: "main".into(), oid: "x".into() };
        assert!(check(&recreate, &facts_at("c1", "main", &[])).unwrap_err().contains("exists again"));
    }

    #[test]
    fn the_newest_reversible_action_is_the_one_undone_and_order_is_kept() {
        let soft = UndoOp::ResetSoft { to: "a".into(), expect_head: "b".into() };
        let actions = vec![action("one", 1, "git_commit", Some(soft.clone())), action("push", 2, "git_push", None), action("two", 3, "git_commit", Some(soft))];
        assert_eq!(pick_target(&actions, "/r", None).unwrap().id, "two", "the push is skipped: it cannot be undone");
        assert_eq!(pick_target(&actions, "/r", Some("two")).unwrap().id, "two");
        assert!(pick_target(&actions, "/r", Some("one")).unwrap_err().contains("later action has to be undone first"));
        assert!(pick_target(&actions, "/r", Some("push")).unwrap_err().contains("left the machine"));
        assert!(pick_target(&actions, "/other", None).unwrap_err().contains("nothing to undo"));
        assert!(pick_target(&actions, "/r", Some("nope")).is_err());
    }

    #[test]
    fn an_undone_action_is_not_undone_twice_and_the_next_one_becomes_the_newest() {
        let soft = UndoOp::ResetSoft { to: "a".into(), expect_head: "b".into() };
        let mut actions = vec![action("one", 1, "git_commit", Some(soft.clone())), action("two", 2, "git_commit", Some(soft))];
        actions[1].undone_ms = Some(9);
        assert_eq!(pick_target(&actions, "/r", None).unwrap().id, "one");
        assert!(pick_target(&actions, "/r", Some("two")).unwrap_err().contains("already undone"));
    }

    #[test]
    fn a_later_push_is_reported_as_out_of_reach() {
        let soft = UndoOp::ResetSoft { to: "a".into(), expect_head: "b".into() };
        let actions = vec![action("commit", 1, "git_commit", Some(soft)), action("push", 2, "git_push", None)];
        let later = later_irreversible(&actions, &actions[0]);
        assert_eq!(later.len(), 1);
        assert_eq!(later[0].id, "push");
        assert!(later_irreversible(&actions, &actions[1]).is_empty());
    }

    #[test]
    fn the_log_reads_back_and_the_summaries_are_short() {
        let a = vec![action("x", 1, "git_commit", Some(UndoOp::Checkout { branch: "main".into() }))];
        assert_eq!(parse(&serde_json::to_string(&a).unwrap()), a);
        assert!(parse("{ nope").is_empty());
        assert_eq!(summarize("git_commit", &json!({"message": "fix: totals\n\nbody"})), "Committed “fix: totals”");
        assert_eq!(summarize("git_branch_create", &json!({"name": "feature/x"})), "Created branch feature/x");
        assert!(describe(&UndoOp::ResetHard { to: "0123456789".into(), expect_head: "x".into() }).contains("01234567"));
    }
}
