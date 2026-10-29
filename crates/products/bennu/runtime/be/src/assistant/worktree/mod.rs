//! An isolated copy of the project for one conversation: the assistant works there, and nothing reaches the
//! user's files until they choose to apply it.
//!
//! ## Two kinds, chosen when the conversation starts
//!
//! * **Copy — text only.** File, search, build and live tools; no code intelligence, nothing indexed. For a
//!   git repository (with git access on) the copy is a detached linked worktree, which costs no full copy;
//!   otherwise it is a plain copy, capped in size, that leaves out version control, dependency folders and
//!   build output.
//! * **Git worktree — indexed.** The same worktree, opened as a *hidden* project (see `hidden_projects`) so
//!   every tool works on it. It never appears in the user's projects, and its indexing progress is shown in
//!   the conversation, not in the editor.
//!
//! A worktree starts from what the user sees, not from the last commit: files they had changed or not yet
//! added are copied over it. Where it lives is the app data directory, never beside the repository.
//!
//! ## Getting the work back
//!
//! The copy's files are compared with a manifest taken when it was made. **Apply** shows the combined diff
//! first, refuses — naming each file — if the project changed a file the copy also changed, and otherwise
//! writes every change or none (`apply`). **Discard** drops it. Either one tears everything down: the
//! processes the conversation started there, the index (in memory and on disk), the worktree and its git
//! record, the folder. Deleting the conversation does the same; a sweep at startup removes what a crash left.
//!
//! The rules that need no disk — path mapping, what an apply means, how clashes are found — are the pure
//! `bennu_assistant::worktree`, with the tests.

mod apply;
mod files;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use bennu_assistant::prelude::{rel_of, Base, Manifest, WorktreeMode};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::session::TOPIC;
use super::store;

/// A copy not used for this long is swept at startup, whether or not it holds changes.
const TTL_MS: u64 = 30 * 24 * 3600 * 1000;
/// Copies kept at most; the least recently used go first.
const MAX_KEPT: usize = 12;

/// Where one conversation's copy is and how it was made. Saved beside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Meta {
    pub conversation: String,
    pub mode: WorktreeMode,
    /// The project as the user opened it.
    pub real_root: String,
    /// The folder created for the copy (the worktree's root, or the plain copy's).
    pub tree_root: String,
    /// The project inside it: the assistant's root. Differs from `tree_root` when the project is a
    /// subfolder of its repository.
    pub copy_root: String,
    pub via_git: bool,
    pub repo_root: Option<String>,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub created_ms: u64,
    pub touched_ms: u64,
}

/// What the copy and the project looked like when it was made (see `bennu_assistant::worktree`).
#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct Stored {
    pub base: Manifest,
    /// Only where the project's bytes differed from the copy's at the start (a checkout that converted line
    /// endings); `None` means the project had no such file.
    pub real: std::collections::BTreeMap<String, Option<Base>>,
}

// ── Where things are ─────────────────────────────────────────────────────────────────────────────

fn base_dir() -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("assistant-worktrees")
}

fn dir_of(conversation: &str) -> Result<PathBuf, String> {
    if !store::valid_id(conversation) {
        return Err(format!("`{conversation}` is not a conversation id"));
    }
    Ok(base_dir().join(conversation))
}

fn now_ms() -> u64 {
    super::now_ms()
}

pub(super) fn load(conversation: &str) -> Option<Meta> {
    let text = std::fs::read_to_string(dir_of(conversation).ok()?.join("meta.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn save(meta: &Meta) -> Result<(), String> {
    let path = dir_of(&meta.conversation)?.join("meta.json");
    std::fs::write(path, serde_json::to_vec(meta).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn load_stored(conversation: &str) -> Result<Stored, String> {
    let text = std::fs::read_to_string(dir_of(conversation)?.join("manifest.json")).map_err(|e| format!("the copy's manifest is missing: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("the copy's manifest is damaged: {e}"))
}

fn save_stored(conversation: &str, stored: &Stored) -> Result<(), String> {
    std::fs::write(dir_of(conversation)?.join("manifest.json"), serde_json::to_vec(stored).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

// ── Conversations being run ──────────────────────────────────────────────────────────────────────

static RUNNING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// A run began in this conversation: its copy must not be applied or discarded under it.
pub(super) fn mark_running(conversation: &str) {
    RUNNING.lock().unwrap_or_else(|p| p.into_inner()).insert(conversation.to_string());
}

pub(super) fn unmark_running(conversation: &str) {
    RUNNING.lock().unwrap_or_else(|p| p.into_inner()).remove(conversation);
}

pub(super) fn is_running(conversation: &str) -> bool {
    RUNNING.lock().unwrap_or_else(|p| p.into_inner()).contains(conversation)
}

// ── Progress ─────────────────────────────────────────────────────────────────────────────────────

fn emit(state: &BennuState, conversation: &str, phase: &str, done: usize, total: usize) {
    state.event_sink().emit(TOPIC, json!({ "kind": "worktree", "conversation_id": conversation, "phase": phase, "done": done, "total": total }));
}

// ── Making a copy ────────────────────────────────────────────────────────────────────────────────

/// What a conversation asks for when it starts.
pub(super) struct Request<'a> {
    pub conversation: &'a str,
    pub mode: WorktreeMode,
    pub real_root: &'a str,
    /// The user turned git access on in Settings: the only case a worktree may be made.
    pub git_allowed: bool,
}

/// Make the copy. Blocking and long: call it off the request worker.
pub(super) fn create(state: &BennuState, req: &Request) -> Result<Meta, String> {
    let dir = dir_of(req.conversation)?;
    files::remove_tree(&dir)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let built = build(state, req, &dir);
    if built.is_err() {
        let _ = files::remove_tree(&dir);
    }
    built
}

fn build(state: &BennuState, req: &Request, dir: &Path) -> Result<Meta, String> {
    let real = Path::new(req.real_root);
    let tree = dir.join("tree");
    let repo = if req.git_allowed { files::git_root_of(real) } else { None };
    if req.mode == WorktreeMode::Git && repo.is_none() {
        return Err(git_unavailable(req.git_allowed));
    }
    let tree_s = tree.to_string_lossy().to_string();
    let made = match repo {
        Some(_) => {
            emit(state, req.conversation, "worktree", 0, 0);
            match make_worktree(state, req, &tree_s) {
                Ok(made) => Ok(made),
                // The text-only copy can always fall back to a plain one; the indexed kind cannot.
                Err(e) if req.mode == WorktreeMode::Copy => plain_copy(state, req, &tree).map_err(|p| format!("{e}; and {p}")),
                Err(e) => Err(e),
            }
        }
        None => plain_copy(state, req, &tree),
    }?;
    let (copy_root, via_git, workdir, head, branch) = made;

    emit(state, req.conversation, "snapshot", 0, 0);
    let walked = files::walk(Path::new(&copy_root), None).unwrap_or_else(|(_, w)| w);
    let base = files::manifest_of(&walked, &|d, t| emit(state, req.conversation, "snapshot", d, t));
    let real_overrides = if via_git { real_differences(real, &walked, &base) } else { Default::default() };
    let meta = Meta {
        conversation: req.conversation.to_string(),
        mode: req.mode,
        real_root: req.real_root.to_string(),
        tree_root: tree_s,
        copy_root,
        via_git,
        repo_root: workdir,
        head,
        branch,
        created_ms: now_ms(),
        touched_ms: now_ms(),
    };
    save_stored(req.conversation, &Stored { base, real: real_overrides })?;
    save(&meta)?;
    Ok(meta)
}

fn git_unavailable(git_allowed: bool) -> String {
    if git_allowed {
        "This project is not a git repository, so there is no worktree to make. Use the text-only copy instead.".into()
    } else {
        "A git worktree needs git access, which is off. Turn it on in Settings › Imber, or use the text-only copy.".into()
    }
}

type Made = (String, bool, Option<String>, Option<String>, Option<String>);

/// A detached git worktree, started from what the user sees: their uncommitted work is copied over it.
fn make_worktree(state: &BennuState, req: &Request, tree: &str) -> Result<Made, String> {
    let added = state.host_call("__assistant_worktree", json!({ "op": "assistant_worktree_add", "args": { "root": req.real_root, "dest": tree } }))?;
    let text = |k: &str| added.get(k).and_then(Value::as_str).map(str::to_string);
    let workdir = text("workdir").ok_or("git did not say where the repository is")?;
    let changed: Vec<String> = added.get("changed").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    overlay_changes(Path::new(&workdir), Path::new(tree), &changed);
    let sub = rel_of(&workdir, req.real_root).ok_or("the project is not inside its repository")?;
    let copy_root = if sub.is_empty() { tree.replace('\\', "/") } else { format!("{}/{sub}", tree.replace('\\', "/")) };
    Ok((copy_root, true, Some(workdir), text("head"), text("branch")))
}

/// Make the worktree look like the working directory for every path the user had touched.
fn overlay_changes(workdir: &Path, tree: &Path, changed: &[String]) {
    for rel in changed {
        let (from, to) = (workdir.join(rel), tree.join(rel));
        if from.is_file() {
            if let Some(parent) = to.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&from, &to);
        } else if !from.exists() {
            let _ = std::fs::remove_file(&to);
        }
    }
}

/// A size-capped copy of the project's sources.
fn plain_copy(state: &BennuState, req: &Request, tree: &Path) -> Result<Made, String> {
    emit(state, req.conversation, "copying", 0, 0);
    let walked = files::walk(Path::new(req.real_root), Some((files::MAX_COPY_BYTES, files::MAX_COPY_FILES))).map_err(|(why, w)| too_big(&why, &w))?;
    files::copy_all(&walked, tree, &|d, t| emit(state, req.conversation, "copying", d, t))?;
    Ok((tree.to_string_lossy().replace('\\', "/"), false, None, None, None))
}

fn too_big(why: &files::Overflow, seen: &files::Walked) -> String {
    let limit = match why {
        files::Overflow::Bytes => format!("{} MB", files::MAX_COPY_BYTES / (1024 * 1024)),
        files::Overflow::Files => format!("{} files", files::MAX_COPY_FILES),
    };
    format!(
        "The project is too large for a plain copy (over {limit}; {} files seen so far). Nothing was copied. A git repository gets a worktree instead, which needs no full copy — turn git access on in Settings › Imber.",
        seen.files.len()
    )
}

/// For a worktree: the files whose bytes in the project differ from the worktree's at the start.
fn real_differences(real: &Path, walked: &files::Walked, base: &Manifest) -> std::collections::BTreeMap<String, Option<Base>> {
    let mut out = std::collections::BTreeMap::new();
    let prefix_of = |rel: &String| real.join(rel);
    for rel in walked.files.keys() {
        let mine = files::identity(&prefix_of(rel)).ok();
        if mine != base.get(rel).copied() {
            out.insert(rel.clone(), mine);
        }
    }
    out
}

// ── Opening the index (the indexed kind) ─────────────────────────────────────────────────────────

/// Open the copy as a hidden project and index it, waiting for a Java index to be ready (the tools are
/// useless before). Idempotent: a copy already open is left alone. Blocking: call it off the request worker.
pub(super) fn attach(state: &BennuState, meta: &Meta) {
    if !meta.mode.indexed() {
        return;
    }
    crate::hidden_projects::hide(&meta.copy_root, &meta.conversation);
    emit(state, &meta.conversation, "indexing", 0, 0);
    let java = match crate::project::open_and_start(state, &meta.copy_root, crate::lsp_registry::SessionOrigin::Background, true) {
        Ok(info) => info.kind.is_java(),
        Err(e) => {
            state.event_sink().emit(TOPIC, json!({ "kind": "worktree", "conversation_id": meta.conversation, "phase": "failed", "message": e }));
            return;
        }
    };
    if java {
        wait_for_index(&meta.copy_root);
    }
    emit(state, &meta.conversation, "ready", 0, 0);
}

fn wait_for_index(root: &str) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(180);
    while std::time::Instant::now() < until {
        if crate::index_service::IndexService::global().index_stats(root).ready {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}

// ── Starting a run in a copy ─────────────────────────────────────────────────────────────────────

fn fresh_state(ctx: &BennuState) -> BennuState {
    let mut state = BennuState::new(ctx.event_sink());
    if let Some(host) = ctx.host_caller() {
        state = state.with_host_caller(host);
    }
    state
}

/// The copy this message works in. A conversation that already has one keeps it (and has it re-opened if the
/// index was released); a new conversation that asked for one gets it made; any other works on the project.
/// Slow work runs on the blocking pool, never on a runtime worker (it waits on the shell and on git).
pub(super) async fn prepare(
    ctx: &BennuState,
    conv: &mut store::Conversation,
    wanted: Option<&str>,
    real_root: &str,
    git_allowed: bool,
) -> Result<Option<Meta>, String> {
    let state = fresh_state(ctx);
    let meta = if let Some(mode) = conv.worktree.clone() {
        let id = conv.id.clone();
        load(&id).ok_or_else(|| format!("This conversation's isolated {mode} copy no longer exists (it was swept or removed). Start a new conversation to work on the project."))?
    } else {
        let Some(word) = wanted.filter(|_| conv.messages.is_empty()) else { return Ok(None) };
        let mode = WorktreeMode::parse(word).ok_or_else(|| format!("`{word}` is not a kind of isolated copy"))?;
        let (id, root) = (conv.id.clone(), real_root.to_string());
        let st = fresh_state(ctx);
        let made = tokio::task::spawn_blocking(move || create(&st, &Request { conversation: &id, mode, real_root: &root, git_allowed }))
            .await
            .map_err(|e| format!("could not make the copy: {e}"))??;
        conv.worktree = Some(mode.id().to_string());
        made
    };
    let attached = meta.clone();
    tokio::task::spawn_blocking(move || {
        attach(&state, &attached);
        let mut touched = attached;
        touched.touched_ms = now_ms();
        let _ = save(&touched);
    })
    .await
    .map_err(|e| format!("could not open the copy: {e}"))?;
    Ok(Some(meta))
}

// ── Tearing it down ──────────────────────────────────────────────────────────────────────────────

/// Drop everything a copy holds. Best effort, in the order that lets each step succeed; returns what could
/// not be removed, so the caller can say so instead of claiming it is gone.
pub(super) fn teardown(state: Option<&BennuState>, meta: &Meta) -> Option<String> {
    crate::live::stop_owned(&meta.copy_root, &meta.conversation);
    crate::live::stop_root(&meta.copy_root);
    if meta.mode.indexed() {
        crate::project_close::release_now(&meta.copy_root);
        crate::index_service::IndexService::global().discard_index_files(&meta.copy_root);
    }
    crate::hidden_projects::unhide(&meta.copy_root);
    if meta.via_git {
        if let (Some(state), Some(_)) = (state, &meta.repo_root) {
            let _ = state.host_call("__assistant_worktree", json!({ "op": "assistant_worktree_remove", "args": { "root": meta.real_root, "dest": meta.tree_root } }));
        }
    }
    dir_of(&meta.conversation).and_then(|d| files::remove_tree(&d)).err()
}

/// A conversation is gone: so is its copy.
pub(super) fn discard_for_deleted(state: &BennuState, conversation: &str) {
    if let Some(meta) = load(conversation) {
        let _ = teardown(Some(state), &meta);
    } else if let Ok(dir) = dir_of(conversation) {
        let _ = files::remove_tree(&dir);
    }
}

// ── The sweep ────────────────────────────────────────────────────────────────────────────────────

/// What a directory under the copies folder is, for deciding whether to keep it.
#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub id: String,
    pub touched_ms: u64,
    pub conversation_exists: bool,
}

/// Which copies to remove: those whose conversation is gone, those unused past the time limit, and the
/// least recently used beyond the cap.
pub(super) fn sweep_victims(entries: &[Entry], now_ms: u64, ttl_ms: u64, max_kept: usize) -> Vec<String> {
    let mut victims: Vec<String> = Vec::new();
    let mut keep: Vec<&Entry> = Vec::new();
    for e in entries {
        if !e.conversation_exists || now_ms.saturating_sub(e.touched_ms) > ttl_ms {
            victims.push(e.id.clone());
        } else {
            keep.push(e);
        }
    }
    keep.sort_by(|a, b| b.touched_ms.cmp(&a.touched_ms));
    victims.extend(keep.into_iter().skip(max_kept).map(|e| e.id.clone()));
    victims
}

/// Remove copies a crash or an old conversation left behind. Runs once, off the main path, at startup.
pub(super) fn sweep() {
    let Ok(read) = std::fs::read_dir(base_dir()) else { return };
    let convs = store::dir();
    let entries: Vec<Entry> = read
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().to_string();
            let touched_ms = load(&id).map(|m| m.touched_ms).unwrap_or(0);
            Some(Entry { conversation_exists: store::load_in(&convs, &id).is_ok(), id, touched_ms })
        })
        .collect();
    for id in sweep_victims(&entries, now_ms(), TTL_MS, MAX_KEPT) {
        if let Some(meta) = load(&id) {
            let _ = teardown(None, &meta);
        } else if let Ok(dir) = dir_of(&id) {
            let _ = files::remove_tree(&dir);
        }
    }
}

// ── What the panel asks ──────────────────────────────────────────────────────────────────────────

fn info_json(meta: &Meta) -> Value {
    json!({
        "conversationId": meta.conversation,
        "mode": meta.mode.id(),
        "realRoot": meta.real_root,
        "copyRoot": meta.copy_root,
        "viaGit": meta.via_git,
        "head": meta.head.as_deref().map(|h| h.chars().take(8).collect::<String>()),
        "branch": meta.branch,
        "createdMs": meta.created_ms,
    })
}

/// Which copies the composer may offer for `root`, and why one is not available.
#[arbor_rpc::handler]
fn assistant_worktree_options(_ctx: &BennuState, root: String, git: bool) -> Result<Value, String> {
    let repo = files::git_root_of(Path::new(&root));
    let git_ok = git && repo.is_some();
    let git_reason = if git_ok { String::new() } else { git_unavailable(git && repo.is_none()) };
    let (copy_ok, copy_reason) = if git_ok {
        (true, String::new())
    } else {
        match files::walk(Path::new(&root), Some((files::MAX_COPY_BYTES, files::MAX_COPY_FILES))) {
            Ok(_) => (true, String::new()),
            Err((why, w)) => (false, too_big(&why, &w)),
        }
    };
    Ok(json!({
        "git": { "available": git_ok, "reason": git_reason },
        "copy": { "available": copy_ok, "reason": copy_reason, "via": if git_ok { "git" } else { "copy" } },
    }))
}

/// The copy a conversation works in, if it has one.
#[arbor_rpc::handler]
fn assistant_worktree_info(_ctx: &BennuState, id: String) -> Result<Value, String> {
    Ok(load(&id).map(|m| info_json(&m)).unwrap_or(Value::Null))
}

/// The combined diff of the copy against the project, and the files that could not be applied.
#[arbor_rpc::handler]
fn assistant_worktree_diff(_ctx: &BennuState, id: String) -> Result<Value, String> {
    let meta = load(&id).ok_or("this conversation has no isolated copy")?;
    let gathered = apply::gather(&meta, &load_stored(&id)?)?;
    Ok(apply::diff_json(&meta, &gathered))
}

/// Apply the copy to the project, all or nothing, then drop it. `applied: false` carries the files in the way.
#[arbor_rpc::handler]
fn assistant_worktree_apply(ctx: &BennuState, id: String) -> Result<Value, String> {
    let meta = load(&id).ok_or("this conversation has no isolated copy")?;
    if is_running(&id) {
        return Err("Imber is still working in this copy — wait for it to finish or stop it first".into());
    }
    let gathered = apply::gather(&meta, &load_stored(&id)?)?;
    if !gathered.plan.can_apply() {
        return Ok(json!({ "applied": false, "clashes": gathered.plan.clashes }));
    }
    let targets: Vec<PathBuf> = gathered.plan.changes.iter().map(|c| PathBuf::from(&meta.real_root).join(&c.rel)).collect();
    let run_key = format!("apply-{id}");
    if let Err(held) = super::tools::reserve::reserve_for(&run_key, &id, "applying an isolated copy", &targets) {
        return Ok(json!({ "applied": false, "held": held }));
    }
    let result = apply::execute(&meta, &gathered);
    super::tools::reserve::release_run(&run_key);
    let count = result?;
    let leftover = teardown(Some(ctx), &meta);
    finish_conversation(&id, &format!("The isolated copy was applied to the project ({count} file(s)) and removed. Later messages work on the project itself."));
    Ok(json!({ "applied": true, "count": count, "leftover": leftover }))
}

/// Drop the copy without applying it.
#[arbor_rpc::handler]
fn assistant_worktree_discard(ctx: &BennuState, id: String) -> Result<Value, String> {
    let meta = load(&id).ok_or("this conversation has no isolated copy")?;
    if is_running(&id) {
        return Err("Imber is still working in this copy — stop it first".into());
    }
    let leftover = teardown(Some(ctx), &meta);
    finish_conversation(&id, "The isolated copy was discarded; nothing was applied to the project. Later messages work on the project itself.");
    Ok(json!({ "leftover": leftover }))
}

/// The conversation no longer has a copy: record it, and say so in the thread.
fn finish_conversation(id: &str, notice: &str) {
    let dir = store::dir();
    if let Ok(mut conv) = store::load_in(&dir, id) {
        conv.worktree = None;
        conv.messages.push(json!({
            "id": format!("n{}", now_ms()), "role": "assistant", "at": now_ms(),
            "blocks": [{ "type": "notice", "level": "info", "text": notice }],
        }));
        let _ = store::save_in(&dir, &conv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, touched: u64, exists: bool) -> Entry {
        Entry { id: id.into(), touched_ms: touched, conversation_exists: exists }
    }

    #[test]
    fn a_copy_whose_conversation_is_gone_is_swept() {
        let v = sweep_victims(&[entry("a", 1000, false), entry("b", 1000, true)], 2000, 10_000, 10);
        assert_eq!(v, ["a"]);
    }

    #[test]
    fn a_copy_unused_past_the_limit_is_swept_and_a_recent_one_is_not() {
        let v = sweep_victims(&[entry("old", 0, true), entry("new", 95_000, true)], 100_000, 50_000, 10);
        assert_eq!(v, ["old"]);
    }

    #[test]
    fn the_cap_removes_the_least_recently_used() {
        let entries: Vec<Entry> = (0..5).map(|i| entry(&format!("c{i}"), 1000 + i, true)).collect();
        let v = sweep_victims(&entries, 2000, 100_000, 3);
        assert_eq!(v, ["c1", "c0"]);
    }

    #[test]
    fn nothing_is_swept_when_all_are_fresh_and_within_the_cap() {
        assert!(sweep_victims(&[entry("a", 1000, true)], 1500, 100_000, 3).is_empty());
    }

    #[test]
    fn a_git_worktree_without_git_access_is_refused_with_the_way_out() {
        assert!(git_unavailable(false).contains("Settings"));
        assert!(git_unavailable(true).contains("not a git repository"));
    }

    #[test]
    fn a_run_marks_its_conversation_busy_until_it_ends() {
        mark_running("c-busy");
        assert!(is_running("c-busy") && !is_running("c-other"));
        unmark_running("c-busy");
        assert!(!is_running("c-busy"));
    }

    #[test]
    fn a_plain_copy_of_a_small_project_has_its_files_and_a_matching_manifest() {
        let src = std::env::temp_dir().join(format!("bennu-wt-mod-src-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("src")).unwrap();
        std::fs::write(src.join("src/A.java"), "class A {}").unwrap();
        std::fs::write(src.join("pom.xml"), "<project/>").unwrap();
        let dst = std::env::temp_dir().join(format!("bennu-wt-mod-dst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dst);
        let walked = files::walk(&src, Some((files::MAX_COPY_BYTES, files::MAX_COPY_FILES))).unwrap();
        files::copy_all(&walked, &dst, &|_, _| {}).unwrap();
        let base = files::manifest_of(&walked, &|_, _| {});
        let copy = files::manifest_of(&files::walk(&dst, None).unwrap(), &|_, _| {});
        assert_eq!(base, copy);
        // No bytes differ, so no override is needed.
        assert!(real_differences(&src, &files::walk(&dst, None).unwrap(), &base).is_empty());
        let _ = std::fs::remove_dir_all(src);
        let _ = std::fs::remove_dir_all(dst);
    }
}
