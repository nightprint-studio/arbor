//! `graph` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name**, so the command is reached generically through the router. The pure
//! git work is libgit2 (`git2`)-coupled and already lives in the reusable shell
//! module [`crate::git::graph`]; rather than extract a CLI-style `corvus-git`
//! module, these handlers delegate straight to it (no subprocess involved),
//! mirroring the `stash` domain's delegate-to-shell path.
//!
//! The generic `rpc` command already wraps dispatch in `spawn_blocking`, so the
//! per-handler `tokio::task::spawn_blocking` that the old async commands used is
//! dropped here; the brief-lock-then-reopen shape (copy the repo path under the
//! lock, release it, open a fresh `Repository` handle for the walk) is kept so
//! large repos don't starve other commands waiting for the lock. Behavior
//! (locks held, fresh-handle reopen, errors) is byte-identical.
//!
//! No hooks fire in this domain.
//!
//! `start_file_meta_scan` and `export_graph_svg` spawn background work that
//! outlives the call and streams frontend events from inside it. Reached
//! through the generic `rpc` command they hold only `&AppState` (no
//! `AppHandle`), so the background thread captures the **event sink**
//! (`Arc<dyn EventSink>` — [`AppState::event_sink`]) plus the `Arc`s it needs
//! (the job registry), never an `AppHandle`. Topics and payloads are
//! byte-identical to the old inline commands — only the egress handle changed.

use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::EventSink;

use crate::error::AppError;
use crate::git::graph::{CommitDetail, GraphData, RepoFileEntry};
use crate::ipc::corvus;
use crate::jobs::JobRegistry;
use crate::AppState;

#[derive(serde::Serialize, Clone)]
struct FileMetaBatch {
    tab_id: String,
    entries: Vec<RepoFileEntry>,
}

// One cancellation flag per tab. When a new scan starts for a tab, the old
// flag is set to `true` so the previous background thread stops early.
static SCAN_TOKENS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
> = std::sync::LazyLock::new(Default::default);

#[corvus::handler]
fn get_repo_files(state: &AppState, tab_id: String) -> Result<Vec<String>, AppError> {
    // Grab the repo path under a brief lock then do the walk on a fresh handle
    // so large repos don't freeze the IPC queue.
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::get_repo_files(&repo)
}

#[corvus::handler]
fn get_files_last_commit(
    state: &AppState,
    tab_id: String,
    paths: Vec<String>,
) -> Result<Vec<RepoFileEntry>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::get_files_last_commit(&repo, paths)
}

/// Return a fast fingerprint of the repository's current ref state.
/// Used by the frontend cache to detect whether anything has changed
/// without loading the full graph.
///
/// Format: `<HEAD-SHA>|<ref1:sha1>,<ref2:sha2>,...` (refs sorted).
///
/// Only includes refs under `refs/heads/`, `refs/remotes/`, `refs/tags/` —
/// pseudo-refs like `FETCH_HEAD` and `ORIG_HEAD` are touched on every git
/// operation (even no-op fetches) and would make the fingerprint flap,
/// triggering pointless graph reloads from `refreshIfChanged`.
#[corvus::handler]
fn get_repo_fingerprint(state: &AppState, tab_id: String) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let inner = repo.inner();

    let head = inner
        .head()
        .ok()
        .and_then(|h| h.target())
        .map(|oid| oid.to_string())
        .unwrap_or_default();

    let mut parts: Vec<String> = inner
        .references()
        .map_err(|e| AppError::Other(e.to_string()))?
        .flatten()
        .filter_map(|r| {
            let name = r.name()?.to_owned();
            // Skip pseudo-refs (FETCH_HEAD, ORIG_HEAD, MERGE_HEAD, …) — they
            // mutate on every operation regardless of actual state changes.
            if !(name.starts_with("refs/heads/")
                || name.starts_with("refs/remotes/")
                || name.starts_with("refs/tags/"))
            {
                return None;
            }
            let target = r.target()?.to_string();
            Some(format!("{}:{}", name, target))
        })
        .collect();
    parts.sort_unstable();

    Ok(format!("{}|{}", head, parts.join(",")))
}

#[corvus::handler]
fn get_graph(
    state: &AppState,
    tab_id: String,
    offset: usize,
    limit: usize,
) -> Result<GraphData, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    // Mutable handle so we can feed `stash_foreach` on the same repo
    // after the (immutable-only) graph walk finishes.
    let mut repo = git2::Repository::open(&repo_path)?;
    let mut data = crate::git::graph::load_graph(&repo, offset, limit)?;
    // Stash collection is cheap (few entries, no deep diff). Failures
    // are swallowed: a broken stash reflog shouldn't hide the graph.
    data.stashes = crate::git::stash::collect_stash_refs(&mut repo).unwrap_or_default();
    Ok(data)
}

#[corvus::handler]
fn get_graph_for_file(
    state: &AppState,
    tab_id: String,
    file_path: String,
    offset: usize,
    limit: usize,
) -> Result<GraphData, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::load_graph_for_file(&repo, &file_path, offset, limit)
}

/// Grabs the repo path and releases the mutex immediately, then opens a *fresh*
/// Repository handle so the scan does not starve other commands waiting for the
/// lock.
#[corvus::handler]
fn get_repo_file_tree(state: &AppState, tab_id: String) -> Result<Vec<RepoFileEntry>, AppError> {
    // Hold the mutex only long enough to copy the path string.
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::get_repo_file_tree(&repo)
}

#[corvus::handler]
fn get_commit_detail(
    state: &AppState,
    tab_id: String,
    oid: String,
) -> Result<CommitDetail, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::graph::get_commit_detail(&repo, &oid)
}

/// Starts the file metadata scan in a background thread.
/// Cancels any previous scan for the same tab before starting a new one.
/// Emits `arbor://file-meta-batch` (batches of RepoFileEntry) progressively
/// and `arbor://file-meta-done` when complete or cancelled.
#[corvus::handler]
fn start_file_meta_scan(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;

    let repo_path = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.path.clone()
    };

    // Cancel any existing scan for this tab and register a fresh token.
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut tokens = SCAN_TOKENS.lock().unwrap();
        if let Some(old) = tokens.get(&tab_id) {
            old.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        tokens.insert(tab_id.clone(), std::sync::Arc::clone(&cancel));
    }

    let sink_bg = Arc::clone(&sink);

    std::thread::spawn(move || {
        use git2::{Repository, Sort};
        use std::collections::HashMap;
        use std::sync::atomic::Ordering;

        const BATCH_SIZE: usize  = 50;
        const MAX_COMMITS: usize = 20_000;

        let repo = match Repository::open(&repo_path) {
            Ok(r) => r,
            Err(_) => {
                sink_bg.emit("arbor://file-meta-done", serde_json::json!(&tab_id));
                return;
            }
        };

        let mut index = match repo.index() { Ok(i) => i, Err(_) => return };
        let _ = index.read(false);

        let mut entry_map: HashMap<String, RepoFileEntry> = index
            .iter()
            .filter_map(|e| std::str::from_utf8(&e.path).ok().map(|p| p.to_owned()))
            .map(|path| {
                let e = RepoFileEntry {
                    path: path.clone(),
                    last_commit_oid: None,
                    last_commit_short_oid: None,
                    last_commit_date: None,
                    last_commit_summary: None,
                };
                (path, e)
            })
            .collect();

        let total = entry_map.len();
        let mut found = 0usize;
        let mut pending: Vec<RepoFileEntry> = Vec::with_capacity(BATCH_SIZE);

        let mut revwalk = match repo.revwalk() { Ok(r) => r, Err(_) => return };
        if revwalk.push_head().is_ok() {
            let _ = revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME);
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.include_untracked(false).ignore_whitespace(false);

            let mut commit_count = 0usize;
            'walk: for oid_result in revwalk {
                if found >= total || commit_count >= MAX_COMMITS { break; }

                // Check cancellation every 100 commits (cheap atomic read).
                if commit_count % 100 == 0 && cancel.load(Ordering::Relaxed) {
                    sink_bg.emit("arbor://file-meta-done", serde_json::json!(&tab_id));
                    return;
                }

                let oid     = match oid_result             { Ok(o) => o, Err(_) => continue };
                let commit  = match repo.find_commit(oid)  { Ok(c) => c, Err(_) => continue };
                let tree    = match commit.tree()           { Ok(t) => t, Err(_) => continue };
                let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
                let diff = match parent_tree {
                    Some(ref pt) => repo.diff_tree_to_tree(Some(pt), Some(&tree), Some(&mut diff_opts)),
                    None         => repo.diff_tree_to_tree(None,     Some(&tree), Some(&mut diff_opts)),
                };
                let diff = match diff { Ok(d) => d, Err(_) => continue };

                let oid_full  = oid.to_string();
                let short_oid = oid_full[..7].to_string();
                let date      = commit.time().seconds();
                let summary   = commit.summary().unwrap_or("").to_string();

                for delta in diff.deltas() {
                    let candidates = [
                        delta.new_file().path().and_then(|p| p.to_str()),
                        delta.old_file().path().and_then(|p| p.to_str()),
                    ];
                    for path in candidates.into_iter().flatten() {
                        if let Some(entry) = entry_map.get_mut(path) {
                            if entry.last_commit_oid.is_none() {
                                entry.last_commit_oid       = Some(oid_full.clone());
                                entry.last_commit_short_oid = Some(short_oid.clone());
                                entry.last_commit_date      = Some(date);
                                entry.last_commit_summary   = Some(summary.clone());
                                found += 1;
                                pending.push(entry.clone());

                                if pending.len() >= BATCH_SIZE {
                                    let batch = std::mem::take(&mut pending);
                                    sink_bg.emit("arbor://file-meta-batch", serde_json::json!(FileMetaBatch {
                                        tab_id: tab_id.clone(),
                                        entries: batch,
                                    }));
                                }
                                if found >= total { break 'walk; }
                            }
                        }
                    }
                }
                commit_count += 1;
            }
        }

        if !pending.is_empty() {
            sink_bg.emit("arbor://file-meta-batch", serde_json::json!(FileMetaBatch {
                tab_id: tab_id.clone(),
                entries: pending,
            }));
        }
        sink_bg.emit("arbor://file-meta-done", serde_json::json!(&tab_id));
    });

    Ok(())
}

// ── SVG export helpers ───────────────────────────────────────────────────────

fn svg_finish_job(
    sink: &Arc<dyn EventSink>,
    jobs: &Arc<Mutex<JobRegistry>>,
    job_id: &str,
    success: bool,
    message: &str,
) {
    if let Ok(mut jobs) = jobs.lock() {
        let status = if success {
            crate::jobs::JobStatus::Completed { exit_code: 0 }
        } else {
            crate::jobs::JobStatus::Failed { error: message.to_string() }
        };
        jobs.set_status(job_id, status);
    }

    sink.emit("arbor://job-done", serde_json::json!({
        "job_id":    job_id,
        "success":   success,
        "exit_code": if success { 0i32 } else { -1i32 },
        "cancelled": false,
    }));

    let (title, level) = if success {
        ("SVG export complete", "success")
    } else {
        ("SVG export failed", "error")
    };

    sink.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   title,
        "message": message,
        "level":   level,
    }));
}

/// Export the full commit graph to an SVG file.
///
/// Returns the job-id immediately; the heavy work runs in a background thread.
/// Progress is streamed via `arbor://job-output`; a `plugin:notification` is
/// emitted when the export finishes (success or failure).
#[corvus::handler]
fn export_graph_svg(
    state: &AppState,
    tab_id: String,
    output_path: String,
    theme_vars: Option<std::collections::HashMap<String, String>>,
) -> Result<String, AppError> {
    use crate::jobs::{JobInfo, JobStatus};

    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    let jobs = Arc::clone(&state.jobs);

    // Grab repo path without holding the mutex into the background thread.
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        let repo    = mgr.get(&tab_id)?;
        repo.path.clone()
    };

    // Register a job entry so it appears in the Jobs overlay immediately.
    let job_id = {
        let mut jobs = jobs.lock()
            .map_err(|_| AppError::Other("mutex poisoned".into()))?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            "Export Graph as SVG".into(),
            plugin_name:     "arbor".into(),
            command:         format!("→ {output_path}"),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("System".into()),
            non_cancellable: true,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
            target:          None,
        });
        id
    };

    // Tell the frontend the job exists so it can render it in the overlay.
    sink.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        "Export Graph as SVG",
        "plugin_name": "arbor",
        "command":     format!("→ {output_path}"),
        "category":    "System",
    }));

    let jid     = job_id.clone();
    let sink_bg = Arc::clone(&sink);
    let jobs_bg = Arc::clone(&jobs);

    std::thread::spawn(move || {
        // Emit one output line to the ring-buffer + frontend.
        let emit_line = |line: &str| {
            if let Ok(mut jobs) = jobs_bg.lock() {
                jobs.append_output(&jid, line.to_string());
            }
            sink_bg.emit("arbor://job-output", serde_json::json!({
                "job_id": &jid,
                "text":   line,
            }));
        };

        emit_line("Opening repository…");
        let repo = match git2::Repository::open(&repo_path) {
            Ok(r)  => r,
            Err(e) => {
                let err = format!("Cannot open repo: {e}");
                emit_line(&format!("[error] {err}"));
                svg_finish_job(&sink_bg, &jobs_bg, &jid, false, &err);
                return;
            }
        };

        emit_line("Loading full commit graph (this may take a moment for large repos)…");
        let graph = match crate::git::graph::load_graph(&repo, 0, 999_999) {
            Ok(g)  => g,
            Err(e) => {
                let err = format!("Failed to load graph: {e}");
                emit_line(&format!("[error] {err}"));
                svg_finish_job(&sink_bg, &jobs_bg, &jid, false, &err);
                return;
            }
        };

        emit_line(&format!(
            "Graph loaded: {} commits, {} lanes.",
            graph.nodes.len(),
            graph.lane_count,
        ));

        let theme = crate::git::svg_export::ThemeColors::from_vars(
            &theme_vars.unwrap_or_default(),
        );

        match crate::git::svg_export::generate_svg_to_file(
            &graph,
            std::path::Path::new(&output_path),
            &theme,
            &emit_line,
        ) {
            Ok(()) => {
                let ok_msg = format!("Graph exported to '{output_path}'.");
                emit_line(&ok_msg);
                svg_finish_job(&sink_bg, &jobs_bg, &jid, true, &ok_msg);
            }
            Err(e) => {
                emit_line(&format!("[error] {e}"));
                svg_finish_job(&sink_bg, &jobs_bg, &jid, false, &e);
            }
        }
    });

    Ok(job_id)
}
