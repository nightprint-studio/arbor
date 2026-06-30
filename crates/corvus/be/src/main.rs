//! `corvus-be` — the headless git backend process for Model D.
//!
//! Stage 1 proved the process boundary end to end (spawn, framed-stdio
//! handshake, request/response, event push, error wire-format) with a self-test
//! method set. Stage 2 moves the git domains onto it: each domain's handler
//! functions live in their own module here, auto-advertised via `Hello` and
//! auto-routed out-of-process by the shell's `SplitBroker`, once their git
//! dependencies are extracted into the shared `corvus-git` crate. **bisect** and
//! **stash** are served so far (reset next). See `docs/corvus-be-bringup.md`.
//!
//! It owns a [`CorvusState`] (the shell pushes the open tabs' repo paths + the
//! resolved git program into it); handlers resolve a `tab_id` to a path and run
//! the shared `corvus-git` logic. The shell re-emits this process's events to the
//! FE and fires any owed plugin hooks shell-side after the call returns.
//!
//! **stdout is the protocol channel** — all logs go to stderr.

use std::io::{self, Write};
use std::sync::Arc;

use arbor_feedback::prelude::{JobSpec, JobStatus};
use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{http_auth_args_for_credentials, CloneOptions};
use corvus_git_provider_api::prelude::{
    CiFilter, FindingState, MrFilter, ProviderError, SecurityFilters, Severity,
};
use corvus_plugin::prelude::{build_hook_dispatcher, corvus_be_api_installer};
use corvus_plugin_ns::prelude::NsHost;
use serde_json::json;

// Domain handler modules — their `#[arbor_rpc::handler]`s self-register via
// inventory, so `arbor_rpc::registry()` collects them and `Hello` advertises
// them. The shell pushes repo paths to `repo_registry`; `bisect` and `stash`
// are the git domains served out-of-process so far.
mod avatar;
mod bisect;
mod branch;
mod ci;
mod corvus_config;
mod diff;
mod git_cli;
mod gitflow;
mod graph;
mod issues;
mod jobs;
mod linked_worktree;
mod merge;
mod missing;
mod mr;
mod host_handle;
mod notes;
mod plugin_profile;
mod plugin_rpc;
mod provider;
mod rebase;
mod recovery;
mod reflog;
mod remote;
mod repo;
mod repo_browser;
mod repo_config;
mod repo_lifecycle;
mod repo_ops;
mod repo_registry;
mod reset;
mod search;
mod security;
mod stage;
mod stash;
mod stats;
mod status;
mod submodule;
mod tickets;
mod workspace;
mod workspace_mutation;
mod workspace_query;
mod workspace_runs;
mod worktree;
mod worktree_links;

// ── Self-test handlers (Stage 1) ────────────────────────────────────────────
// Plain `#[arbor_rpc::handler]`s, exactly like the shell-side ones — the context
// is `&CorvusState` (downcast from `&dyn Any` by the generated thunk). They
// register via `inventory`, so `arbor_rpc::registry()` collects them and `Hello`
// advertises them by name.

/// Liveness round-trip: `rpc("corvus", "be_ping", {})` → `"pong"`.
#[arbor_rpc::handler]
fn be_ping(_ctx: &CorvusState) -> Result<String, String> {
    Ok("pong".to_string())
}

/// Echo — proves argument decode across the boundary.
#[arbor_rpc::handler]
fn be_echo(_ctx: &CorvusState, message: String) -> Result<String, String> {
    Ok(message)
}

/// Push-event proof: emits `arbor://corvus-be-pong` back through the sink, which
/// the shell re-emits to the FE. Returns immediately.
#[arbor_rpc::handler]
fn be_emit(ctx: &CorvusState, note: Option<String>) -> Result<(), String> {
    ctx.emit(
        "arbor://corvus-be-pong",
        serde_json::json!({ "from": "corvus-be", "note": note }),
    );
    Ok(())
}

/// Reverse-channel proof (`docs/reverse-channel.md`): resolve a credential
/// session for `account` by calling **back** to the shell — which holds the
/// keyring + `VaultSessionProvider` — and return only the resolved `base_url`,
/// never the token. Exercises the whole backend→shell→keyring chain end to end.
/// e.g. `rpc("corvus", "be_session_probe", { "account": "linear" })` with a
/// connected Linear account → `"https://api.linear.app/graphql"`.
///
/// Synchronous on purpose: `host_call` blocks on the shell's reply, delivered by
/// the serve loop's reader thread while this handler is parked on its worker —
/// the reentrancy the reverse channel is built for.
#[arbor_rpc::handler]
fn be_session_probe(ctx: &CorvusState, account: String) -> Result<String, String> {
    let session = ctx.host_call("__session", serde_json::json!(account))?;
    let base = session.get("base_url").and_then(|b| b.as_str()).unwrap_or_default();
    Ok(base.to_string())
}

/// Drive an async provider call from a sync `NsHost` method, reusing the current
/// tokio handle when one is available (corvus-be always runs on a multi-thread
/// runtime, so the `try_current` branch is the normal path) and falling back to a
/// fresh runtime otherwise. Byte-for-byte the shell's `block_on_provider!` macro,
/// including the `ProviderError::Internal("runtime: …")` fallback — the Lua
/// sandbox thread is not guaranteed to be inside a runtime, so the guard is
/// required (a bare `Handle::current().block_on` would panic off-runtime).
macro_rules! block_on_provider {
    ($fut:expr) => {{
        let rt = tokio::runtime::Handle::try_current().ok();
        if let Some(h) = rt {
            h.block_on($fut)
        } else {
            match tokio::runtime::Runtime::new() {
                Ok(r) => r.block_on($fut),
                Err(e) => Err(ProviderError::Internal(format!("runtime: {e}"))),
            }
        }
    }};
}

/// The issue-tracker twin of [`block_on_provider!`]: the tracker futures return
/// `Result<_, IssueTrackerError>` (not `ProviderError`), so the runtime-build
/// fallback arm must produce an `IssueTrackerError` to keep the `if`/`else` arms
/// the same type. `Network` is the closest "request never completed" variant for
/// a missing runtime; `err()` later maps it to the same wire string.
macro_rules! block_on_tracker {
    ($fut:expr) => {{
        let rt = tokio::runtime::Handle::try_current().ok();
        if let Some(h) = rt {
            h.block_on($fut)
        } else {
            match tokio::runtime::Runtime::new() {
                Ok(r) => r.block_on($fut),
                Err(e) => Err(corvus_issues::prelude::IssueTrackerError::Network(format!(
                    "runtime: {e}"
                ))),
            }
        }
    }};
}

/// Host-side impl of `corvus-plugin-ns`'s [`NsHost`]: the bridge that lets the
/// ported git `ns_shell` namespaces (`arbor.notes`, `arbor.repo`, …) run inside
/// this process.
///
/// Holds an `Arc<CorvusState>` so its methods can fire plugin hooks through the
/// state's hook broker — the same broker the OOP RPC handlers fire onto, so a
/// note saved from a Lua plugin and one saved from the FE hit the same listeners.
/// The git work goes through the shared `corvus-git` crate (plain `git2`), opening
/// the active repo by the path the namespace installer reads from
/// `__arbor_current_repo__`; the provider work goes through `be::provider` over
/// the reverse channel — identical results + error strings to the shell.
struct CorvusNsHost {
    state: Arc<CorvusState>,
}

impl CorvusNsHost {
    fn new(state: Arc<CorvusState>) -> Self {
        Self { state }
    }

    /// Resolve a repo to its path the way the shell's `resolve_repo_path` did:
    /// an explicit `repo_id` → the workspace registry entry's path
    /// (`repo '{id}' not registered` on miss); else the active repo
    /// (`active_repo_path`, the `__arbor_current_repo__` global) — `no active tab`
    /// when both are absent. corvus-be has no active-tab concept, so the active
    /// path IS the global (the faithful + cleaner bridge).
    fn resolve_repo_path(
        &self,
        active_repo_path: Option<&str>,
        repo_id: Option<&str>,
    ) -> Result<String, String> {
        if let Some(id) = repo_id {
            let reg = workspace::registry::registry(&self.state);
            let entry = reg.get(id).ok_or_else(|| format!("repo '{id}' not registered"))?;
            return Ok(entry.path.clone());
        }
        active_repo_path
            .map(|p| p.to_string())
            .ok_or_else(|| "no active tab".to_string())
    }
}

impl NsHost for CorvusNsHost {
    fn notes_list(
        &self,
        repo_path: &str,
        commit_oid: &str,
    ) -> Result<serde_json::Value, String> {
        let repo = git2::Repository::open(repo_path).map_err(|e| format!("notes.list: {e}"))?;
        let notes = corvus_git::notes::list_notes(&repo, commit_oid)
            .map_err(|e| format!("notes.list: {e}"))?;
        serde_json::to_value(&notes).map_err(|e| format!("notes.list encode: {e}"))
    }

    fn notes_get(
        &self,
        repo_path: &str,
        commit_oid: &str,
        namespace: &str,
    ) -> Result<Option<String>, String> {
        let repo = git2::Repository::open(repo_path).map_err(|e| format!("notes.get: {e}"))?;
        let oid = git2::Oid::from_str(commit_oid).map_err(|e| format!("notes.get: {e}"))?;
        let notes_ref = format!("refs/notes/{namespace}");
        // Absent note → `None` (not an error), mirroring the shell's `find_note`
        // `Err(_) => None` arm.
        let content = match repo.find_note(Some(&notes_ref), oid) {
            Ok(note) => Some(note.message().unwrap_or("").to_string()),
            Err(_) => None,
        };
        Ok(content)
    }

    fn notes_set(
        &self,
        repo_path: &str,
        commit_oid: &str,
        namespace: &str,
        content: &str,
        plugin_name: &str,
    ) -> Result<(), String> {
        {
            let repo = git2::Repository::open(repo_path).map_err(|e| format!("{e}"))?;
            corvus_git::notes::set_note(&repo, commit_oid, namespace, content)
                .map_err(|e| format!("{e}"))?;
        }
        // Fire the hook through the same broker the RPC `save_commit_note` uses;
        // `tab_id` is absent on the Lua path (the active repo is path-resolved), so
        // the payload carries `plugin` instead — same shape the shell emitted.
        self.state.fire_hook(
            "on_note_saved",
            serde_json::json!({
                "commit_oid": commit_oid,
                "namespace": namespace,
                "plugin": plugin_name,
            }),
        );
        Ok(())
    }

    fn notes_delete(
        &self,
        repo_path: &str,
        commit_oid: &str,
        namespace: &str,
        plugin_name: &str,
    ) -> Result<(), String> {
        {
            let repo = git2::Repository::open(repo_path).map_err(|e| format!("{e}"))?;
            corvus_git::notes::delete_note(&repo, commit_oid, namespace)
                .map_err(|e| format!("{e}"))?;
        }
        self.state.fire_hook(
            "on_note_deleted",
            serde_json::json!({
                "commit_oid": commit_oid,
                "namespace": namespace,
                "plugin": plugin_name,
            }),
        );
        Ok(())
    }

    // ── repo ─────────────────────────────────────────────────────────────────

    fn repo_branch(&self, repo_path: &str) -> Result<String, String> {
        let repo =
            git2::Repository::open(repo_path).map_err(|e| format!("repo.branch open: {e}"))?;
        let head = repo.head().map_err(|e| format!("repo.branch head: {e}"))?;
        Ok(head.shorthand().unwrap_or("HEAD").to_string())
    }

    fn repo_is_dirty(&self, repo_path: &str) -> Result<bool, String> {
        let repo =
            git2::Repository::open(repo_path).map_err(|e| format!("repo.is_dirty open: {e}"))?;
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        let statuses = repo
            .statuses(Some(&mut opts))
            .map_err(|e| format!("repo.is_dirty statuses: {e}"))?;
        Ok(!statuses.is_empty())
    }

    fn repo_remote(&self, repo_path: &str, name: &str) -> Result<Option<String>, String> {
        // The borrowed `Remote<'_>` lives only inside the `and_then` chain, so its
        // URL is copied out before `repo` drops. `Ok(None)` when the remote (or its
        // URL) is absent; `Err` only on repo-open failure (shell's `repo.remote`).
        let repo =
            git2::Repository::open(repo_path).map_err(|e| format!("repo.remote open: {e}"))?;
        Ok(repo
            .find_remote(name)
            .ok()
            .and_then(|r| r.url().map(|s| s.to_string())))
    }

    fn repo_fetch_active_tab(&self, repo_path: &str) -> Result<(), String> {
        // Route through the SAME reverse-channel credential path `fetch_remote`
        // uses (the keyring is shell-side). The proactive refresh is best-effort.
        let url = git2::Repository::open(repo_path)
            .ok()
            .and_then(|r| {
                r.find_remote("origin")
                    .ok()
                    .and_then(|rm| rm.url().map(|s| s.to_string()))
            })
            .unwrap_or_default();
        let _ = self.state.host_call("__maybe_refresh_url", json!(url));
        let host = self
            .state
            .host_caller()
            .ok_or_else(|| "no reverse channel".to_string())?;
        let repo = git2::Repository::open(repo_path).map_err(|e| format!("fetch failed: {e}"))?;
        match corvus_git::remote::fetch(&repo, "origin", &crate::remote::credential_resolver(host)) {
            Ok(_) => {
                // Path-resolved → no tab_id; emit a path-keyed payload.
                self.state
                    .emit("arbor://graph-refresh", json!({ "path": repo_path }));
                Ok(())
            }
            Err(e) => {
                eprintln!("corvus-be: auto-fetch failed: {e}");
                Err(format!("fetch failed: {e}"))
            }
        }
    }

    fn repo_release_handles(&self, repo_path: &str) {
        // corvus-be opens repos by path per call and caches no handles, so there
        // is nothing to evict. Kept for Lua-surface fidelity.
        let _ = repo_path;
    }

    fn repo_branches(&self, repo_path: &str) -> Result<serde_json::Value, String> {
        let repo =
            git2::Repository::open(repo_path).map_err(|e| format!("repo.branches open: {e}"))?;
        let head_name = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()));
        let branches = repo
            .branches(None)
            .map_err(|e| format!("repo.branches list: {e}"))?;
        let mut out: Vec<serde_json::Value> = Vec::new();
        for b in branches.flatten() {
            let (branch, btype) = b;
            if let Ok(Some(name)) = branch.name() {
                out.push(json!({
                    "name": name,
                    "is_remote": matches!(btype, git2::BranchType::Remote),
                    "is_head": head_name.as_deref() == Some(name),
                }));
            }
        }
        Ok(serde_json::Value::Array(out))
    }

    fn repo_tags(&self, repo_path: &str) -> Result<serde_json::Value, String> {
        let repo =
            git2::Repository::open(repo_path).map_err(|e| format!("repo.tags open: {e}"))?;
        let names = repo
            .tag_names(None)
            .map_err(|e| format!("repo.tags list: {e}"))?;
        let mut out: Vec<serde_json::Value> = Vec::new();
        for maybe in names.iter() {
            let Some(name) = maybe else { continue };
            let mut entry = serde_json::Map::new();
            entry.insert("name".into(), json!(name));
            if let Ok(obj) = repo.revparse_single(&format!("refs/tags/{name}")) {
                entry.insert("target".into(), json!(obj.id().to_string()));
            }
            out.push(serde_json::Value::Object(entry));
        }
        Ok(serde_json::Value::Array(out))
    }

    fn repo_commits(
        &self,
        repo_path: &str,
        from: Option<&str>,
        to: &str,
        limit: u64,
        include_merges: bool,
    ) -> Result<serde_json::Value, String> {
        let repo =
            git2::Repository::open(repo_path).map_err(|e| format!("repo.commits open: {e}"))?;
        let to_oid = repo
            .revparse_single(to)
            .map_err(|e| format!("repo.commits revparse '{to}': {e}"))?
            .id();
        let from_oid: Option<git2::Oid> = match from {
            Some(f) => Some(
                repo.revparse_single(f)
                    .map_err(|e| format!("repo.commits revparse '{f}': {e}"))?
                    .id(),
            ),
            None => None,
        };
        let mut walk = repo
            .revwalk()
            .map_err(|e| format!("repo.commits revwalk: {e}"))?;
        walk.set_sorting(git2::Sort::TIME)
            .map_err(|e| format!("repo.commits sort: {e}"))?;
        walk.push(to_oid)
            .map_err(|e| format!("repo.commits push to: {e}"))?;
        if let Some(fo) = from_oid {
            walk.hide(fo)
                .map_err(|e| format!("repo.commits hide from: {e}"))?;
        }

        let mut out: Vec<serde_json::Value> = Vec::new();
        let mut idx: u64 = 1;
        for oid_res in walk {
            if idx > limit {
                break;
            }
            let oid = match oid_res {
                Ok(o) => o,
                Err(_) => continue,
            };
            let commit = match repo.find_commit(oid) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if !include_merges && commit.parent_count() > 1 {
                continue;
            }
            let oid_s = oid.to_string();
            let short = oid_s.chars().take(7).collect::<String>();
            let author = commit.author();
            let parents: Vec<String> = commit.parent_ids().map(|p| p.to_string()).collect();
            out.push(json!({
                "oid": oid_s,
                "short_oid": short,
                "summary": commit.summary().unwrap_or(""),
                "message": commit.message().unwrap_or(""),
                "author_name": author.name().unwrap_or(""),
                "author_email": author.email().unwrap_or(""),
                "author_time": author.when().seconds(),
                "parents": parents,
            }));
            idx += 1;
        }
        Ok(serde_json::Value::Array(out))
    }

    fn repo_untracked(&self, repo_path: &str) -> Result<serde_json::Value, String> {
        let repo =
            git2::Repository::open(repo_path).map_err(|e| format!("repo.untracked open: {e}"))?;
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .include_ignored(false)
            .recurse_untracked_dirs(true);
        let statuses = repo
            .statuses(Some(&mut opts))
            .map_err(|e| format!("repo.untracked statuses: {e}"))?;
        let mut out: Vec<serde_json::Value> = Vec::new();
        for entry in statuses.iter() {
            if !entry.status().is_wt_new() {
                continue;
            }
            if let Some(p) = entry.path() {
                out.push(json!(p));
            }
        }
        Ok(serde_json::Value::Array(out))
    }

    fn repo_staged_files(&self, repo_path: &str) -> Result<serde_json::Value, String> {
        let repo = git2::Repository::open(repo_path)
            .map_err(|e| format!("repo.staged_files open: {e}"))?;
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(false)
            .include_ignored(false)
            .renames_head_to_index(true);
        let statuses = repo
            .statuses(Some(&mut opts))
            .map_err(|e| format!("repo.staged_files statuses: {e}"))?;
        let mut out: Vec<serde_json::Value> = Vec::new();
        for entry in statuses.iter() {
            let status = entry.status();
            let label = if status.is_index_new() {
                "added"
            } else if status.is_index_modified() {
                "modified"
            } else if status.is_index_deleted() {
                "deleted"
            } else if status.is_index_renamed() {
                "renamed"
            } else if status.is_index_typechange() {
                "typechange"
            } else {
                continue;
            };
            let rel_path = entry
                .head_to_index()
                .as_ref()
                .and_then(|d| d.new_file().path().map(|p| p.to_string_lossy().to_string()))
                .or_else(|| entry.path().map(|p| p.to_string()));
            let Some(rel) = rel_path else { continue };
            out.push(json!({ "path": rel, "status": label }));
        }
        Ok(serde_json::Value::Array(out))
    }

    fn repo_clone(&self, cfg: serde_json::Value) -> Result<String, String> {
        // Validation already happened installer-side; pull the resolved fields.
        let url = cfg.get("url").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let dest = cfg.get("dest").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let branch = cfg
            .get("branch")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let shallow = cfg.get("shallow").and_then(|v| v.as_bool()).unwrap_or(false);
        let recurse = cfg
            .get("recurse_submodules")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let name_override = cfg.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
        let category_override =
            cfg.get("category").and_then(|v| v.as_str()).map(|s| s.to_string());
        let plugin_name = cfg
            .get("plugin_name")
            .and_then(|v| v.as_str())
            .unwrap_or("arbor")
            .to_string();

        let display_name = name_override.unwrap_or_else(|| format!("Clone: {url}"));
        let category = category_override.or_else(|| Some("Clone".to_string()));
        let display_cmd = {
            let mut parts: Vec<String> = vec!["git".into(), "clone".into(), "--progress".into()];
            if let Some(ref b) = branch {
                parts.push("--branch".into());
                parts.push(b.clone());
            }
            if shallow {
                parts.push("--depth".into());
                parts.push("1".into());
            }
            if recurse {
                parts.push("--recurse-submodules".into());
            }
            parts.push("--".into());
            parts.push(url.clone());
            parts.push(dest.clone());
            parts.join(" ")
        };

        // Mint the job in the shell registry over the reverse channel + emit
        // `arbor://job-started` — the same job contract every OOP background op
        // uses. The Lua `on_done` callback is dropped (the shell's callback
        // registry is shell-process-local) — the one Lua-surface delta.
        let host = self
            .state
            .host_caller()
            .ok_or_else(|| "repo.clone: no reverse channel".to_string())?;
        let job = crate::jobs::JobHandle::register(
            Arc::clone(&host),
            JobSpec {
                name: display_name.clone(),
                plugin_name: plugin_name.clone(),
                command: display_cmd.clone(),
                category: category.clone(),
                non_cancellable: false,
                hidden: false,
                is_system: false,
                target: None,
            },
        )?;
        let job_id = job.id.clone();
        let sink = self.state.event_sink();
        sink.emit(
            "arbor://job-started",
            json!({
                "job_id": &job_id,
                "name": &display_name,
                "plugin_name": &plugin_name,
                "command": &display_cmd,
                "category": &category,
            }),
        );

        // Run the credentialed clone on a detached worker so the Lua call returns
        // the job id immediately (the shell's `spawn_clone_job` was likewise
        // fire-and-forget). Auth is resolved over the reverse channel — the
        // keyring stays shell-side.
        let git = crate::repo::git(&self.state);
        let opts = CloneOptions {
            url: url.clone(),
            dest_path: dest,
            branch,
            shallow,
            recurse_submodules: recurse,
        };
        let sink_bg = Arc::clone(&sink);
        let host_bg = Arc::clone(&host);
        let spawn_result = std::thread::Builder::new()
            .name(format!("corvus-clone-{job_id}"))
            .spawn(move || {
                let auth: Vec<String> = crate::remote::credential_resolver(host_bg)(&url)
                    .ok()
                    .flatten()
                    .map(|(u, p)| http_auth_args_for_credentials(&url, &u, &p))
                    .unwrap_or_default();
                let result = corvus_git::repo::clone_repo(&git, &opts, &auth).map_err(|e| e.to_string());
                let (status, success, msg) = match &result {
                    Ok(_) => (JobStatus::Completed { exit_code: 0 }, true, String::new()),
                    Err(e) => (JobStatus::Failed { error: e.clone() }, false, e.clone()),
                };
                if !msg.is_empty() {
                    job.append(&msg);
                    sink_bg.emit("arbor://job-output", json!({ "job_id": &job.id, "text": msg }));
                }
                job.set_status(status);
                sink_bg.emit(
                    "arbor://job-done",
                    json!({ "job_id": &job.id, "success": success, "exit_code": if success { 0 } else { -1 } }),
                );
            });
        if let Err(e) = spawn_result {
            let err = format!("failed to spawn clone thread: {e}");
            let _ = host.call(
                "__job_set_status",
                json!({ "job_id": &job_id, "status": JobStatus::Failed { error: err.clone() } }),
            );
            sink.emit(
                "arbor://job-done",
                json!({ "job_id": &job_id, "success": false, "exit_code": -1 }),
            );
            return Err(err);
        }
        Ok(job_id)
    }

    // ── workspace ────────────────────────────────────────────────────────────

    fn workspace_list(&self) -> Result<serde_json::Value, String> {
        let store = workspace::store::store(&self.state);
        let out: Vec<serde_json::Value> = store.ordered().iter().map(ws_to_json).collect();
        Ok(serde_json::Value::Array(out))
    }

    fn workspace_active(&self) -> Result<Option<serde_json::Value>, String> {
        let store = workspace::store::store(&self.state);
        Ok(store.active().map(ws_to_json))
    }

    fn workspace_get(&self, ws_id: &str) -> Result<Option<serde_json::Value>, String> {
        let store = workspace::store::store(&self.state);
        Ok(store.get(ws_id).map(ws_to_json))
    }

    fn workspace_list_repos(&self, ws_id: Option<&str>) -> Result<serde_json::Value, String> {
        let reg = workspace::registry::registry(&self.state);
        let mut out: Vec<serde_json::Value> = Vec::new();
        match ws_id {
            Some(id) => {
                let store = workspace::store::store(&self.state);
                let Some(ws) = store.get(id) else {
                    return Ok(serde_json::Value::Array(out));
                };
                for repo_id in &ws.repo_ids {
                    if let Some(e) = reg.get(repo_id) {
                        out.push(entry_to_json(e));
                    }
                }
            }
            None => {
                for e in reg.list() {
                    out.push(entry_to_json(&e));
                }
            }
        }
        Ok(serde_json::Value::Array(out))
    }

    fn workspace_repo(&self, repo_id: &str) -> Result<Option<serde_json::Value>, String> {
        let reg = workspace::registry::registry(&self.state);
        Ok(reg.get(repo_id).map(entry_to_json))
    }

    fn workspace_switch(&self, ws_id: &str, _plugin_name: &str) -> Result<(), String> {
        // Reload → validate → set active → persist, under the registry lock
        // (`mutate` saves for us). Returns `(from_id, payload)` for the emit/hook.
        let (from, payload) = workspace::store::mutate(&self.state, |store| {
            if store.get(ws_id).is_none() {
                return Err(format!("workspace '{ws_id}' not found"));
            }
            let from = store.active_workspace_id.clone();
            store.active_workspace_id = Some(ws_id.to_string());
            let ws = store
                .get(ws_id)
                .ok_or_else(|| format!("workspace '{ws_id}' vanished mid-switch"))?;
            Ok((from, ws_to_json(ws)))
        })?;

        let mut payload = payload;
        if let (Some(f), Some(obj)) = (from, payload.as_object_mut()) {
            obj.insert("from_id".into(), serde_json::Value::String(f));
        }
        // Same egress + broker the shell used (no `plugin` key in the payload).
        self.state.emit("arbor://workspace-switched", payload.clone());
        self.state.fire_hook("on_workspace_switched", payload);
        Ok(())
    }

    // ── linked_worktrees ─────────────────────────────────────────────────────

    fn linked_worktrees_list(&self) -> Result<serde_json::Value, String> {
        let reg = worktree_links::registry(&self.state);
        let out: Vec<serde_json::Value> = reg
            .list()
            .iter()
            .map(|l| {
                json!({
                    "id": l.id,
                    "name": l.name,
                    "sync_enabled": l.sync_enabled,
                    "member_count": l.members.len(),
                })
            })
            .collect();
        Ok(serde_json::Value::Array(out))
    }

    fn linked_worktrees_get(&self, id: &str) -> Result<Option<serde_json::Value>, String> {
        let reg = worktree_links::registry(&self.state);
        match reg.get(id) {
            Some(l) => serde_json::to_value(l)
                .map(Some)
                .map_err(|e| format!("linked_worktrees.get encode: {e}")),
            None => Ok(None),
        }
    }

    fn linked_worktrees_set_sync_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        worktree_links::mutate(&self.state, |reg| reg.set_sync_enabled(id, enabled))?;
        self.state
            .emit("arbor://worktree-links-changed", json!({}));
        Ok(())
    }

    // ── mr ───────────────────────────────────────────────────────────────────

    fn mr_list(
        &self,
        active_repo_path: Option<&str>,
        repo_id: Option<&str>,
        state_filter: &str,
        author: Option<&str>,
        resolve_current_user: bool,
        labels: Option<&[String]>,
        query: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let path = self
            .resolve_repo_path(active_repo_path, repo_id)
            .map_err(|e| format!("arbor.mr.list: {e}"))?;
        let resolved = crate::provider::provider_for_path(&path)
            .map_err(|e| format!("arbor.mr.list resolve provider: {e}"))?;
        crate::provider::maybe_refresh(&resolved.info.provider);

        // Resolve the `current_user` sentinel; auth failure → empty result no-op.
        let mut effective_author = author.map(|s| s.to_string());
        if resolve_current_user {
            match block_on_provider!(resolved.provider.current_user()) {
                Ok(u) => effective_author = Some(u.login),
                Err(_) => return Ok(json!([])),
            }
        }

        let filter = MrFilter {
            state: Some(state_filter.to_string()),
            author: effective_author.clone(),
            assignee: None,
            labels: labels.map(|s| s.to_vec()),
            query: query.map(|s| s.to_string()),
            page: None,
            per_page: Some(100),
        };
        let mrs = block_on_provider!(resolved.provider.list_mrs(&resolved.repo, filter))
            .map_err(|e| format!("arbor.mr.list: {}", crate::provider::pe(e)))?;

        // Defensive client-side author filter (some providers don't honour it).
        let mrs: Vec<_> = match effective_author {
            Some(a) => mrs
                .into_iter()
                .filter(|m| m.author.login.eq_ignore_ascii_case(&a))
                .collect(),
            None => mrs,
        };
        serde_json::to_value(&mrs).map_err(|e| format!("arbor.mr.list encode: {e}"))
    }

    fn mr_current_user(
        &self,
        active_repo_path: Option<&str>,
        repo_id: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let path = self
            .resolve_repo_path(active_repo_path, repo_id)
            .map_err(|e| format!("arbor.mr.current_user: {e}"))?;
        let resolved = crate::provider::provider_for_path(&path)
            .map_err(|e| format!("arbor.mr.current_user resolve: {e}"))?;
        crate::provider::maybe_refresh(&resolved.info.provider);
        let user = block_on_provider!(resolved.provider.current_user())
            .map_err(|e| format!("arbor.mr.current_user: {}", crate::provider::pe(e)))?;
        serde_json::to_value(&user).map_err(|e| format!("encode: {e}"))
    }

    // ── ci ───────────────────────────────────────────────────────────────────

    fn ci_runs(
        &self,
        repo_path: Option<&str>,
        repo_id: Option<&str>,
        branch: Option<&str>,
        status: Option<&str>,
        mr_number: Option<u64>,
        per_page: Option<u32>,
    ) -> Result<serde_json::Value, String> {
        let path = self
            .resolve_repo_path(repo_path, repo_id)
            .map_err(|e| format!("arbor.ci.runs: {e}"))?;
        let resolved = crate::provider::provider_for_path(&path)
            .map_err(|e| format!("arbor.ci.runs resolve: {e}"))?;
        // No maybe_refresh here — byte-faithful to the shell's `ns_shell/ci.rs`.
        let filter = CiFilter {
            branch: branch.map(|s| s.to_string()),
            status: status.map(|s| s.to_string()),
            mr_number,
            head_sha: None,
            page: None,
            per_page: per_page.or(Some(20)),
        };
        let runs = block_on_provider!(resolved.provider.list_ci_runs(&resolved.repo, filter))
            .map_err(|e| format!("arbor.ci.runs: {}", crate::provider::pe(e)))?;
        serde_json::to_value(&runs).map_err(|e| format!("arbor.ci.runs encode: {e}"))
    }

    // ── security ─────────────────────────────────────────────────────────────

    fn security_supports(
        &self,
        repo_id: Option<&str>,
        current_repo: Option<&str>,
    ) -> Result<bool, String> {
        let path = self
            .resolve_repo_path(current_repo, repo_id)
            .map_err(|e| format!("arbor.security.supports: {e}"))?;
        // No provider for this remote → not supported, NOT an error (shell parity).
        let resolved = match crate::provider::provider_for_path(&path) {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };
        block_on_provider!(resolved.provider.supports_security(&resolved.repo))
            .map_err(|e| format!("arbor.security.supports: {}", crate::provider::pe(e)))
    }

    fn security_summary(
        &self,
        repo_id: Option<&str>,
        current_repo: Option<&str>,
        range_days: u32,
    ) -> Result<serde_json::Value, String> {
        let path = self
            .resolve_repo_path(current_repo, repo_id)
            .map_err(|e| format!("arbor.security.summary: {e}"))?;
        let resolved = crate::provider::provider_for_path(&path)
            .map_err(|e| format!("arbor.security.summary resolve: {e}"))?;
        let summary =
            block_on_provider!(resolved.provider.fetch_security_summary(&resolved.repo, range_days))
                .map_err(|e| format!("arbor.security.summary: {}", crate::provider::pe(e)))?;
        serde_json::to_value(&summary).map_err(|e| format!("arbor.security.summary encode: {e}"))
    }

    fn security_findings(
        &self,
        repo_id: Option<&str>,
        current_repo: Option<&str>,
        severities: &[String],
        states: &[String],
        report_types: &[String],
        search: Option<&str>,
        limit: Option<u32>,
    ) -> Result<serde_json::Value, String> {
        let path = self
            .resolve_repo_path(current_repo, repo_id)
            .map_err(|e| format!("arbor.security.findings: {e}"))?;
        let resolved = crate::provider::provider_for_path(&path)
            .map_err(|e| format!("arbor.security.findings resolve: {e}"))?;

        // Parse the raw tokens exactly as the shell did (unknown dropped; empty
        // `states` → active default `[Detected, Confirmed]`).
        let severities: Vec<Severity> =
            severities.iter().filter_map(|s| parse_severity(s)).collect();
        let states: Vec<FindingState> = if states.is_empty() {
            vec![FindingState::Detected, FindingState::Confirmed]
        } else {
            states.iter().filter_map(|s| parse_state(s)).collect()
        };
        let filters = SecurityFilters {
            severities,
            states,
            report_types: report_types.to_vec(),
            search: search.map(|s| s.to_string()),
            limit,
        };
        let findings = block_on_provider!(resolved
            .provider
            .fetch_security_findings(&resolved.repo, filters))
        .map_err(|e| format!("arbor.security.findings: {}", crate::provider::pe(e)))?;
        serde_json::to_value(&findings).map_err(|e| format!("arbor.security.findings encode: {e}"))
    }

    fn security_refresh_active_tab(
        &self,
        current_repo: Option<&str>,
        range_days: u32,
    ) -> Result<serde_json::Value, String> {
        let Some(path) = current_repo else {
            return Err("arbor.security.refresh_active_tab: no active tab".to_string());
        };
        // Resolve the active path to an open tab id for the emit payload (the
        // shell read `active_tab_id`; here we map path → tab_id). No matching open
        // tab → the shell's "no active tab" branch.
        let tab_id = self
            .state
            .open_tabs()
            .into_iter()
            .find(|(_, p)| p == path)
            .map(|(t, _)| t)
            .ok_or_else(|| "arbor.security.refresh_active_tab: no active tab".to_string())?;
        let resolved = crate::provider::provider_for_path(path)
            .map_err(|e| format!("arbor.security.refresh_active_tab resolve: {e}"))?;
        let summary =
            block_on_provider!(resolved.provider.fetch_security_summary(&resolved.repo, range_days))
                .map_err(|e| {
                    format!("arbor.security.refresh_active_tab: {}", crate::provider::pe(e))
                })?;
        self.state.emit(
            "arbor://security-refresh",
            json!({ "tab_id": tab_id, "summary": &summary }),
        );
        serde_json::to_value(&summary)
            .map_err(|e| format!("arbor.security.refresh_active_tab encode: {e}"))
    }

    // ── toolchain ─────────────────────────────────────────────────────────────
    //
    // PROXY: the toolchain registry lives in the shell's `AppState`, so each op
    // is a reverse-channel round-trip to the matching `__toolchain_<op>` handler
    // in `src-tauri/src/ipc/mod.rs`, which reads/mutates the real registry exactly
    // as `ns_shell/toolchain.rs` did and returns the same shapes / error strings.
    // The shell handler already carries the `toolchain.<op>[ lock| encode]:`
    // prefixes, so these surface the `host_call` error `String` verbatim.

    fn toolchain_list(&self, kind: &str) -> Result<serde_json::Value, String> {
        self.state
            .host_call("__toolchain_list", json!({ "kind": kind }))
    }

    fn toolchain_active(&self, kind: &str) -> Result<Option<serde_json::Value>, String> {
        // The shell handler returns the entry JSON, or JSON `null` when none is
        // active — map `null` to `None` (→ Lua nil) here.
        let v = self
            .state
            .host_call("__toolchain_active", json!({ "kind": kind }))?;
        Ok(if v.is_null() { None } else { Some(v) })
    }

    fn toolchain_env(&self, kind: &str, id: Option<&str>) -> Result<serde_json::Value, String> {
        self.state
            .host_call("__toolchain_env", json!({ "kind": kind, "id": id }))
    }

    fn toolchain_detect(&self, kind: &str) -> Result<serde_json::Value, String> {
        self.state
            .host_call("__toolchain_detect", json!({ "kind": kind }))
    }

    fn toolchain_add(&self, kind: &str, entry: serde_json::Value) -> Result<(), String> {
        self.state
            .host_call("__toolchain_add", json!({ "kind": kind, "entry": entry }))
            .map(|_| ())
    }

    fn toolchain_remove(&self, kind: &str, id: &str) -> Result<(), String> {
        self.state
            .host_call("__toolchain_remove", json!({ "kind": kind, "id": id }))
            .map(|_| ())
    }

    fn toolchain_set_active(&self, kind: &str, id: &str) -> Result<(), String> {
        self.state
            .host_call("__toolchain_set_active", json!({ "kind": kind, "id": id }))
            .map(|_| ())
    }

    // ── tabs ──────────────────────────────────────────────────────────────────
    //
    // DIRECT: resolve `repo_id` against the workspace registry (reload-on-access,
    // infallible — the shell's `registry lock: …` branch can't fire here) and emit
    // `arbor://open-repo-tab` with the same `{ repo_id, path, display_name,
    // remote_url? }` payload the shell produced.

    fn tabs_open_repo(&self, repo_id: &str) -> Result<(), String> {
        let reg = workspace::registry::registry(&self.state);
        let entry = match reg.get(repo_id) {
            Some(e) => e.clone(),
            None => return Err(format!("repo '{repo_id}' not in registry")),
        };
        let payload = json!({
            "repo_id":      entry.id,
            "path":         entry.path,
            "display_name": entry.display_name,
            "remote_url":   entry.remote_url,
        });
        self.state.emit("arbor://open-repo-tab", payload);
        Ok(())
    }

    // ── issues ────────────────────────────────────────────────────────────────
    //
    // DIRECT: corvus-be owns the reverse-channel-backed issue-tracker registry
    // (`crate::issues`), so these run in-process, blocking on the backend tokio
    // runtime via `block_on_tracker!`. Error text is mapped through
    // `crate::issues::err` so it is byte-identical to the shell's `to_app_error`
    // string, then carries the `issues.<op>:` prefix the shell's
    // `ns_shell/issues.rs` applied.

    fn issues_search(&self, filters: serde_json::Value) -> Result<serde_json::Value, String> {
        // `null`/malformed filters → IssueFilters::default (mirrors the shell's
        // `unwrap_or_default()`).
        let f: corvus_issues::prelude::IssueFilters =
            serde_json::from_value(filters).unwrap_or_default();
        let issues = block_on_tracker!(crate::issues::linear().search_issues(f))
            .map_err(|e| format!("issues.search: {}", crate::issues::err(e)))?;
        serde_json::to_value(&issues).map_err(|e| format!("issues.search encode: {e}"))
    }

    fn issues_get(&self, id: &str) -> Result<serde_json::Value, String> {
        let issue = block_on_tracker!(crate::issues::linear().get_issue(id))
            .map_err(|e| format!("issues.get: {}", crate::issues::err(e)))?;
        serde_json::to_value(&issue).map_err(|e| format!("issues.get encode: {e}"))
    }

    fn issues_lookup(
        &self,
        repo_path: &str,
        identifier: &str,
    ) -> Result<Option<serde_json::Value>, String> {
        // Route per-repo: resolve the configured tracker off corvus-be's typed
        // RepoConfig (legacy `ticket_links.tracker` override wins over
        // `issue_tracker`), exactly as the shell's `integrations::tracker_for_repo`.
        // Empty identifier / no tracker / no match → Ok(None) (Lua nil).
        let id = identifier.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let cfg = crate::repo_config::load(repo_path)?;
        let Some(tracker) = cfg.ticket_links.and_then(|t| t.tracker).or(cfg.issue_tracker) else {
            return Ok(None);
        };
        let Some(t) = crate::issues::registry().get(&tracker) else {
            return Ok(None);
        };
        let issue_opt = block_on_tracker!(t.lookup_by_identifier(id))
            .map_err(|e| format!("issues.lookup: {}", crate::issues::err(e)))?;
        match issue_opt {
            None => Ok(None),
            Some(issue) => serde_json::to_value(&issue)
                .map(Some)
                .map_err(|e| format!("issues.lookup encode: {e}")),
        }
    }

    fn issues_transition(&self, id: &str, status_id: &str) -> Result<serde_json::Value, String> {
        let issue = block_on_tracker!(crate::issues::linear().transition_issue(id, status_id))
            .map_err(|e| format!("issues.transition: {}", crate::issues::err(e)))?;
        serde_json::to_value(&issue).map_err(|e| format!("issues.transition encode: {e}"))
    }

    fn issues_comment(&self, issue_id: &str, body: &str) -> Result<serde_json::Value, String> {
        let comment = block_on_tracker!(crate::issues::linear().add_comment(issue_id, body))
            .map_err(|e| format!("issues.comment: {}", crate::issues::err(e)))?;
        serde_json::to_value(&comment).map_err(|e| format!("issues.comment encode: {e}"))
    }

    fn issues_branch_name(&self, issue: serde_json::Value) -> Result<String, String> {
        // Pure compute; a malformed issue table is a programming error → the
        // installer raises the returned String as a Lua RuntimeError (matching the
        // shell's raise-on-bad-shape `branch_name`).
        let i: corvus_issues::prelude::Issue =
            serde_json::from_value(issue).map_err(|e| e.to_string())?;
        Ok(corvus_issues::prelude::branch_name_for_issue(&i))
    }

    // ── terminal ──────────────────────────────────────────────────────────────
    //
    // DIRECT: corvus-be runs the command in-process. Mirrors the shell's
    // `TerminalManager::exec_command` byte-for-byte (split on whitespace, first
    // token = program; `cwd` when given; suppress the console window; spawn failure
    // → `exec failed: …`). Permission gating runs installer-side.

    fn terminal_exec(
        &self,
        command: &str,
        cwd: Option<&str>,
    ) -> Result<(i32, String, String), String> {
        use arbor_process_ext::prelude::NoWindowExt;

        let mut parts = command.split_whitespace();
        let prog = parts.next().ok_or_else(|| "empty command".to_string())?;

        let mut cmd = std::process::Command::new(prog);
        cmd.args(parts);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let output = cmd
            .no_window()
            .output()
            .map_err(|e| format!("exec failed: {e}"))?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok((exit_code, stdout, stderr))
    }

    // ── job ───────────────────────────────────────────────────────────────────
    //
    // PROXY: the `JobRegistry` (and the OS process) live in the shell's
    // `AppState`, so every op round-trips. `job_new_id` reuses the pre-existing
    // `__job_register` handler to reserve an id + register a Running `JobInfo`; the
    // rest route to the new `__job_*` handlers.

    fn job_new_id(
        &self,
        name: &str,
        plugin_name: &str,
        command: &str,
        category: Option<&str>,
        hidden: bool,
        target: Option<&str>,
    ) -> Result<String, String> {
        let spec = json!({
            "name": name,
            "plugin_name": plugin_name,
            "command": command,
            "category": category,
            "non_cancellable": false,
            "hidden": hidden,
            "is_system": false,
            "target": target,
        });
        let v = self.state.host_call("__job_register", spec)?;
        v.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "job.spawn: __job_register returned non-string id".to_string())
    }

    fn job_spawn(&self, spec: serde_json::Value) -> Result<(), String> {
        self.state.host_call("__job_spawn", spec).map(|_| ())
    }

    fn job_list(&self) -> Result<serde_json::Value, String> {
        self.state.host_call("__job_list", json!({}))
    }

    fn job_cancel(&self, job_id: &str) -> Result<(), String> {
        self.state
            .host_call("__job_cancel", json!({ "job_id": job_id }))
            .map(|_| ())
    }

    fn job_dismiss(&self, job_id: &str) -> Result<bool, String> {
        let v = self
            .state
            .host_call("__job_dismiss", json!({ "job_id": job_id }))?;
        Ok(v.as_bool().unwrap_or(false))
    }

    fn job_clear_finished(&self) -> Result<Vec<String>, String> {
        let v = self.state.host_call("__job_clear_finished", json!({}))?;
        serde_json::from_value(v).map_err(|e| format!("job.clear_finished decode: {e}"))
    }

    // ── ui branding ───────────────────────────────────────────────────────────
    //
    // PROXY: the Tauri window-icon API + `AppState.branding` + `arbor://*`
    // rebroadcast live in the shell. Pure validation ran installer-side; these
    // forward the resolved values to the matching `__set_branding` /
    // `__clear_branding` / `__set_theme_overlay` / `__clear_theme_overlay`
    // handlers. The only host-originated error (`window_icon_path failed: …`) is
    // surfaced verbatim.

    fn ui_set_branding(
        &self,
        svg: Option<&str>,
        window_icon_path: Option<&str>,
        plugin_name: &str,
    ) -> Result<(), String> {
        self.state
            .host_call(
                "__set_branding",
                json!({ "svg": svg, "window_icon_path": window_icon_path, "plugin": plugin_name }),
            )
            .map(|_| ())
    }

    fn ui_clear_branding(&self, plugin_name: &str) -> Result<(), String> {
        self.state
            .host_call("__clear_branding", json!({ "plugin": plugin_name }))
            .map(|_| ())
    }

    fn ui_set_theme_overlay(
        &self,
        plugin_name: &str,
        vars: serde_json::Value,
    ) -> Result<(), String> {
        self.state
            .host_call("__set_theme_overlay", json!({ "plugin": plugin_name, "vars": vars }))
            .map(|_| ())
    }

    fn ui_clear_theme_overlay(&self, plugin_name: &str) -> Result<(), String> {
        self.state
            .host_call("__clear_theme_overlay", json!({ "plugin": plugin_name }))
            .map(|_| ())
    }

    // ── pipeline ──────────────────────────────────────────────────────────────
    //
    // PROXY: the pipeline engine + runtime live in the shell's `AppState`, so each
    // op is a reverse-channel round-trip to the matching `__pipeline_<op>` handler
    // in `src-tauri/src/ipc/mod.rs`, which reads/mutates the real engine and
    // starts/resumes/discards runs exactly as `ns_shell/pipeline.rs` did and
    // returns the same shapes / error strings. The shell handler carries the
    // `pipeline.<op>: …` prefixes, so these surface the `host_call` error `String`
    // verbatim. `register_op` / `unregister_op` are purely Lua-local (no method).

    fn pipeline_define(
        &self,
        config: serde_json::Value,
        plugin_name: &str,
    ) -> Result<(), String> {
        self.state
            .host_call(
                "__pipeline_define",
                json!({ "config": config, "plugin_name": plugin_name }),
            )
            .map(|_| ())
    }

    fn pipeline_run(
        &self,
        plugin_name: &str,
        pipeline_id: &str,
        cwd: Option<&str>,
        silent: Option<bool>,
    ) -> Result<String, String> {
        let v = self.state.host_call(
            "__pipeline_run",
            json!({
                "plugin_name": plugin_name,
                "pipeline_id": pipeline_id,
                "cwd":         cwd,
                "silent":      silent,
            }),
        )?;
        v.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "pipeline.run: malformed run_id reply".to_string())
    }

    fn pipeline_resume(&self, run_id: &str) -> Result<(), String> {
        self.state
            .host_call("__pipeline_resume", json!({ "run_id": run_id }))
            .map(|_| ())
    }

    fn pipeline_discard(&self, run_id: &str) -> Result<(), String> {
        self.state
            .host_call("__pipeline_discard", json!({ "run_id": run_id }))
            .map(|_| ())
    }

    fn pipeline_is_locked(&self, lock_key: &str) -> Result<Option<String>, String> {
        // The shell handler returns the holding run id, or JSON `null` when free —
        // map `null` to `None` (→ Lua nil).
        let v = self
            .state
            .host_call("__pipeline_is_locked", json!({ "lock_key": lock_key }))?;
        Ok(v.as_str().map(|s| s.to_string()))
    }

    fn pipeline_list(&self, plugin_name: &str) -> Result<serde_json::Value, String> {
        self.state
            .host_call("__pipeline_list", json!({ "plugin_name": plugin_name }))
    }

    fn pipeline_get(
        &self,
        plugin_name: &str,
        id: &str,
    ) -> Result<Option<serde_json::Value>, String> {
        let v = self.state.host_call(
            "__pipeline_get",
            json!({ "plugin_name": plugin_name, "id": id }),
        )?;
        Ok(if v.is_null() { None } else { Some(v) })
    }

    fn pipeline_cancel(&self, run_id: &str) -> Result<(), String> {
        self.state
            .host_call("__pipeline_cancel", json!({ "run_id": run_id }))
            .map(|_| ())
    }

    fn pipeline_list_runs(
        &self,
        plugin_name: &str,
        filter_plugin: Option<&str>,
        filter_pipeline_id: Option<&str>,
        all: bool,
    ) -> Result<serde_json::Value, String> {
        self.state.host_call(
            "__pipeline_list_runs",
            json!({
                "plugin_name":  plugin_name,
                "plugin":       filter_plugin,
                "pipeline_id":  filter_pipeline_id,
                "all":          all,
            }),
        )
    }

    fn pipeline_get_run(&self, run_id: &str) -> Result<Option<serde_json::Value>, String> {
        let v = self
            .state
            .host_call("__pipeline_get_run", json!({ "run_id": run_id }))?;
        Ok(if v.is_null() { None } else { Some(v) })
    }

    fn pipeline_list_ops(&self) -> Result<serde_json::Value, String> {
        self.state.host_call("__pipeline_list_ops", json!({}))
    }

    // ── cloud ─────────────────────────────────────────────────────────────────
    // PROXY: every op round-trips to the `__cloud_<op>` handler in the shell
    // (src-tauri/src/ipc/mod.rs). The host_call error String is surfaced verbatim.

    fn cloud_secret_set(&self, secret_ref: &str, value: &str) -> Result<(), String> {
        self.state.host_call("__cloud_secret_set", json!({ "secret_ref": secret_ref, "value": value })).map(|_| ())
    }

    fn cloud_secret_exists(&self, secret_ref: &str) -> Result<bool, String> {
        let v = self.state.host_call("__cloud_secret_exists", json!({ "secret_ref": secret_ref }))?;
        Ok(v.as_bool().unwrap_or(false))
    }

    fn cloud_secret_delete(&self, secret_ref: &str) -> Result<(), String> {
        self.state.host_call("__cloud_secret_delete", json!({ "secret_ref": secret_ref })).map(|_| ())
    }

    fn cloud_test_connection(&self, opts: serde_json::Value) -> Result<serde_json::Value, String> {
        self.state.host_call("__cloud_test_connection", opts)
    }

    fn cloud_test_connection_async(&self, opts: serde_json::Value) -> Result<(), String> {
        self.state.host_call("__cloud_test_connection_async", opts).map(|_| ())
    }

    fn cloud_list(&self, opts: serde_json::Value) -> Result<serde_json::Value, String> {
        self.state.host_call("__cloud_list", opts)
    }

    fn cloud_list_stream(&self, opts: serde_json::Value) -> Result<String, String> {
        let v = self.state.host_call("__cloud_list_stream", opts)?;
        Ok(v.as_str().unwrap_or_default().to_string())
    }

    fn cloud_search_stream(&self, opts: serde_json::Value) -> Result<String, String> {
        let v = self.state.host_call("__cloud_search_stream", opts)?;
        Ok(v.as_str().unwrap_or_default().to_string())
    }

    fn cloud_cancel(&self, stream_id: &str) -> Result<(), String> {
        self.state.host_call("__cloud_cancel", json!({ "stream_id": stream_id })).map(|_| ())
    }

    fn cloud_is_cancelled(&self, stream_id: &str) -> Result<bool, String> {
        let v = self.state.host_call("__cloud_is_cancelled", json!({ "stream_id": stream_id }))?;
        Ok(v.as_bool().unwrap_or(false))
    }

    fn cloud_stat(&self, opts: serde_json::Value) -> Result<serde_json::Value, String> {
        self.state.host_call("__cloud_stat", opts)
    }

    fn cloud_delete(&self, opts: serde_json::Value) -> Result<(), String> {
        self.state.host_call("__cloud_delete", opts).map(|_| ())
    }

    fn cloud_copy(&self, opts: serde_json::Value) -> Result<(), String> {
        self.state.host_call("__cloud_copy", opts).map(|_| ())
    }

    fn cloud_download(&self, opts: serde_json::Value) -> Result<String, String> {
        let v = self.state.host_call("__cloud_download", opts)?;
        Ok(v.as_str().unwrap_or_default().to_string())
    }

    fn cloud_upload(&self, opts: serde_json::Value) -> Result<String, String> {
        let v = self.state.host_call("__cloud_upload", opts)?;
        Ok(v.as_str().unwrap_or_default().to_string())
    }

    fn cloud_sync(&self, opts: serde_json::Value) -> Result<String, String> {
        let v = self.state.host_call("__cloud_sync", opts)?;
        Ok(v.as_str().unwrap_or_default().to_string())
    }

    fn cloud_download_many(&self, opts: serde_json::Value) -> Result<String, String> {
        let v = self.state.host_call("__cloud_download_many", opts)?;
        Ok(v.as_str().unwrap_or_default().to_string())
    }

    fn cloud_concat_files(&self, opts: serde_json::Value) -> Result<(), String> {
        self.state.host_call("__cloud_concat_files", opts).map(|_| ())
    }

    fn cloud_report_progress(&self, opts: serde_json::Value) -> Result<(), String> {
        self.state.host_call("__cloud_report_progress", opts).map(|_| ())
    }

    fn cloud_report_done(&self, opts: serde_json::Value) -> Result<(), String> {
        self.state.host_call("__cloud_report_done", opts).map(|_| ())
    }

    fn cloud_pick_chunk_order(&self, opts: serde_json::Value) -> Result<(), String> {
        self.state.host_call("__cloud_pick_chunk_order", opts).map(|_| ())
    }

    fn cloud_oauth_start(&self, opts: serde_json::Value) -> Result<String, String> {
        let v = self.state.host_call("__cloud_oauth_start", opts)?;
        Ok(v.as_str().unwrap_or_default().to_string())
    }

    // ── brp ────────────────────────────────────────────────────────────────────
    //
    // PROXY: the `BrpRegistry` lives in the shell's `AppState.brp`, so each op is
    // a reverse-channel round-trip to the matching `__brp_<op>` handler in
    // `src-tauri/src/ipc/mod.rs`, which mirrors `ns_shell/brp.rs`. The shell
    // returns the Lua-shaped envelope (`{ ok, result|error }`) for connect/call,
    // a `BrpStatus` JSON for disconnect/status, the `sub_id` for watch, and a bool
    // for unwatch. Error `String`s surface verbatim.

    fn brp_connect(&self, endpoint: &str, timeout_ms: u64) -> Result<serde_json::Value, String> {
        self.state.host_call(
            "__brp_connect",
            json!({ "endpoint": endpoint, "timeout_ms": timeout_ms }),
        )
    }

    fn brp_disconnect(&self) -> Result<serde_json::Value, String> {
        self.state.host_call("__brp_disconnect", json!({}))
    }

    fn brp_status(&self) -> Result<serde_json::Value, String> {
        self.state.host_call("__brp_status", json!({}))
    }

    fn brp_call(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        self.state
            .host_call("__brp_call", json!({ "method": method, "params": params }))
    }

    fn brp_watch(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<u64, String> {
        // Best-effort registration; the shell returns the new sub id. The SSE
        // events fire shell-side and are dropped (no channel into this VM).
        let v = self
            .state
            .host_call("__brp_watch", json!({ "method": method, "params": params }))?;
        v.as_u64()
            .ok_or_else(|| "arbor.brp.watch: __brp_watch returned non-integer sub id".to_string())
    }

    fn brp_unwatch(&self, sub_id: u64) -> Result<bool, String> {
        let v = self
            .state
            .host_call("__brp_unwatch", json!({ "sub_id": sub_id }))?;
        Ok(v.as_bool().unwrap_or(false))
    }
}

// ── NsHost JSON marshalling helpers ──────────────────────────────────────────

/// Hand-build the 6-key workspace table (`{ id, name, color_idx, group_id,
/// repo_ids, repo_count }`) the shell's `ws_to_lua` produced — NOT a full serde of
/// `WorkspaceDef` (which carries order/metadata/settings_override the shell
/// omitted), so the Lua shape stays byte-identical.
fn ws_to_json(ws: &workspace::store::WorkspaceDef) -> serde_json::Value {
    json!({
        "id": ws.id,
        "name": ws.name,
        "color_idx": ws.color_idx,
        "group_id": ws.group_id,
        "repo_ids": ws.repo_ids,
        "repo_count": ws.repo_ids.len(),
    })
}

/// Hand-build the 4-key repo-entry table (`{ id, path, display_name,
/// remote_url }`) the shell's `entry_to_lua` produced.
fn entry_to_json(e: &workspace::registry::RepoRegistryEntry) -> serde_json::Value {
    json!({
        "id": e.id,
        "path": e.path,
        "display_name": e.display_name,
        "remote_url": e.remote_url,
    })
}

/// Parse a lowercase severity token (unknown → `None`), verbatim from the shell's
/// `ns_shell/security.rs`.
fn parse_severity(s: &str) -> Option<Severity> {
    match s.to_ascii_lowercase().as_str() {
        "critical" => Some(Severity::Critical),
        "high" => Some(Severity::High),
        "medium" => Some(Severity::Medium),
        "low" => Some(Severity::Low),
        "info" => Some(Severity::Info),
        "unknown" => Some(Severity::Unknown),
        _ => None,
    }
}

/// Parse a lowercase finding-state token (unknown → `None`), verbatim from the
/// shell's `ns_shell/security.rs`.
fn parse_state(s: &str) -> Option<FindingState> {
    match s.to_ascii_lowercase().as_str() {
        "detected" => Some(FindingState::Detected),
        "confirmed" => Some(FindingState::Confirmed),
        "resolved" => Some(FindingState::Resolved),
        "dismissed" => Some(FindingState::Dismissed),
        _ => None,
    }
}

fn main() {
    // The framed-stdio plumbing + the whole plugin runtime, wired in two calls:
    // `BackendIo` builds the writer/sink/reverse-channel/runtime; `App::plugin_host`
    // builds the `PluginHost` (filtered to `corvus`), its headless `AppCtx`, the
    // hook dispatcher (`corvus_plugin::build_hook_dispatcher`, shared with the
    // shell's in-process host so a hook fans out identically), and the scheduler.
    // The plugin reload is DEFERRED to after the `Hello` frame — `App::run`'s
    // default post-`Hello` hook does it (on-load hooks emit events, which must not
    // precede the handshake frame on the pipe).
    // Seed the active profile from the same build-specific pointer the shell
    // reads (`active-profile` / `active-profile-dev`), so this process's
    // profile-scoped plugin paths (`plugin_dir()` = `…/plugins/installed`, and the
    // marketplace root below) resolve to the SAME profile the launcher is on.
    // Without this, corvus-be stays on the `default` profile and a dev launcher
    // would serve plugins from the wrong (or empty) profile.
    arbor_core::prelude::init_active_profile();

    let mut app = arbor_be::App::new(arbor_be::BackendIo::new());
    app.plugin_host("corvus", build_hook_dispatcher);
    // After the Flip (plugin-relocation Phase 2) this backend is the sole loader of
    // the Corvus product's plugins, so it must scan the marketplace install dir
    // just like the launcher host does (`setup/scheduler.rs`'s
    // `set_extra_plugin_roots`). The host's built-in `plugin_dir()` only covers the
    // `installed/` pool; without this, marketplace-installed plugins (the bulk)
    // never load and no contributions reach the Corvus window.
    app.plugin_host_handle()
        .lock()
        .expect("corvus-be: plugin host poisoned at extra-roots set")
        .set_extra_plugin_roots(vec![arbor_core::prelude::marketplace_plugins_dir()]);

    // The state every handler gets: event egress + the hook broker + the reverse
    // channel. `Arc`-shared so `CorvusNsHost` (which the git `arbor.*` namespaces
    // call through) fires hooks onto the same broker the RPC handlers fire onto.
    let state = Arc::new(
        CorvusState::new(app.sink())
            .with_hooks(app.hooks())
            .with_host_caller(app.host_caller()),
    );

    // Build the git/product `arbor.*` namespace installers over the shared state,
    // then hand them to the plugin host's API installer. Each installer captures
    // an `Arc<dyn NsHost>` (the shared `CorvusNsHost`) and marshals Lua <-> JSON
    // through it. After the flip this backend is the sole loader of the Corvus
    // product's plugins (the shell's `ns_shell` copies were deleted).
    let ns_host: Arc<dyn NsHost> = Arc::new(CorvusNsHost::new(Arc::clone(&state)));
    // The ordered git/product namespace set (and the UiBranding-after-core
    // invariant) is owned by `corvus-plugin-ns`, not spelled out here.
    app.api_installer(corvus_be_api_installer(corvus_plugin_ns::installers(ns_host.clone())));

    // Publish the plugin host for the Plugin-Manager RPC adapter
    // (`plugin_rpc::CorvusRpcCtx`): after the Phase-2 flip the shell stops loading
    // Corvus plugins, so the Plugin Manager reads/mutates THIS host. The generic
    // `PluginRpc` handlers reach it through the adapter, which reads this
    // module-static handle (kept off `CorvusState`, which stays host-free).
    host_handle::install(app.plugin_host_handle());

    // The method routing, declared as handler groups (the `Dispatcher` assembles
    // the maps, the advertised-name union, and the per-call context). The git +
    // self-test `#[handler]`s dispatch with the primary `&CorvusState`; the
    // Plugin-Manager `PluginRpc` bundle dispatches with a `CorvusRpcCtx` adapter
    // built fresh per call (the orphan rule blocks `impl PluginRpcContext for
    // CorvusState`, so the bundle can't share it). All of `inventory("")` covers
    // the corvus program (this binary links only its own handlers).
    let dispatcher = arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle())
        .inventory("")
        .group(plugin_rpc::methods(), {
            let state = Arc::clone(&state);
            move || plugin_rpc::CorvusRpcCtx::new(Arc::clone(&state))
        });

    // Pre-serve inits: the issue-tracker + git-provider registries resolve
    // credentials over the reverse channel; git self-detect resolves the system
    // binary before the shell pushes the `"git"` config section. `App::run` fires
    // them in order, then serves with its default post-`Hello` reload +
    // start-schedulers.
    let issues_host = app.host_caller();
    let provider_host = app.host_caller();
    app.init(move || issues::init(issues_host));
    app.init(move || provider::init(provider_host));
    app.init(|| {
        corvus_git_cli::detect(None);
    });

    if let Err(e) = app.run(dispatcher) {
        eprintln!("corvus-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited.
    let _ = io::stderr().flush();
}
