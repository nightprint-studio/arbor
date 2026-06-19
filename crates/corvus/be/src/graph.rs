//! `graph` domain — commit-graph + repo-file queries, served **out-of-process**.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::graph`), but the context is [`CorvusState`] and the
//! pure git work is the shared `corvus-git` `graph` / `graph_svg` modules. The
//! single-shot reads (`get_graph`, `get_graph_for_file`, `get_commit_detail`,
//! the file-tree / fingerprint queries) cross the boundary as one `Response`
//! frame exactly as in-process — `get_graph` is already paginated by
//! `offset`/`limit`, so there is no unbounded payload. **No hooks fire**.
//!
//! Two handlers stream from a background thread via the transport-agnostic
//! `EventSink` (each emit becomes an `Event` frame the shell re-emits to the FE):
//! - `start_file_meta_scan` — emits `arbor://file-meta-batch` / `-done`; keeps
//!   the per-tab cancellation map ([`SCAN_TOKENS`]) module-local, since the scan
//!   runs entirely in this process.
//! - `export_graph_svg` — returns a `job_id` immediately, drives the Jobs-overlay
//!   entry over the reverse channel via [`JobHandle`] (ADR-3), and emits the same
//!   `arbor://job-started` / `-output` / `-done` + `plugin:notification` events
//!   as the in-process copy.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::EventSink;
use corvus_core::prelude::CorvusState;
use corvus_git::graph::{CommitDetail, GraphData, RepoFileEntry};
use git2::Repository;

use crate::jobs::JobHandle;
use crate::repo::{open, repo_path};

#[derive(serde::Serialize, Clone)]
struct FileMetaBatch {
    tab_id: String,
    entries: Vec<RepoFileEntry>,
}

/// One cancellation flag per tab. Starting a new scan for a tab sets the old
/// flag so the previous background thread stops early. Module-local because the
/// scan lives entirely in this process — there is no shell state to consult.
static SCAN_TOKENS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(Default::default);

// ── Single-shot reads ────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn get_repo_files(state: &CorvusState, tab_id: String) -> Result<Vec<String>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::graph::get_repo_files(&repo).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_files_last_commit(
    state: &CorvusState,
    tab_id: String,
    paths: Vec<String>,
) -> Result<Vec<RepoFileEntry>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::graph::get_files_last_commit(&repo, paths).map_err(|e| e.to_string())
}

/// Fast fingerprint of the repo's current ref state — `<HEAD>|<ref:sha>,…` over
/// `refs/heads`, `refs/remotes`, `refs/tags` only (pseudo-refs like `FETCH_HEAD`
/// flap on every op and would force pointless graph reloads). The FE cache uses
/// it to skip reloading the graph when nothing changed.
#[arbor_rpc::handler]
fn get_repo_fingerprint(state: &CorvusState, tab_id: String) -> Result<String, String> {
    let repo = open(state, &tab_id)?;

    let head = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .map(|oid| oid.to_string())
        .unwrap_or_default();

    let mut parts: Vec<String> = repo
        .references()
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|r| {
            let name = r.name()?.to_owned();
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

#[arbor_rpc::handler]
fn get_graph(
    state: &CorvusState,
    tab_id: String,
    offset: usize,
    limit: usize,
) -> Result<GraphData, String> {
    // Mutable handle so `collect_stash_refs` can run on the same repo after the
    // (immutable-only) graph walk.
    let mut repo = open(state, &tab_id)?;
    let mut data = corvus_git::graph::load_graph(&repo, offset, limit).map_err(|e| e.to_string())?;
    // Stash collection is cheap; a broken stash reflog shouldn't hide the graph.
    data.stashes = corvus_git::stash::collect_stash_refs(&mut repo).unwrap_or_default();
    Ok(data)
}

#[arbor_rpc::handler]
fn get_graph_for_file(
    state: &CorvusState,
    tab_id: String,
    file_path: String,
    offset: usize,
    limit: usize,
) -> Result<GraphData, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::graph::load_graph_for_file(&repo, &file_path, offset, limit)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_repo_file_tree(state: &CorvusState, tab_id: String) -> Result<Vec<RepoFileEntry>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::graph::get_repo_file_tree(&repo).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_commit_detail(
    state: &CorvusState,
    tab_id: String,
    oid: String,
) -> Result<CommitDetail, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::graph::get_commit_detail(&repo, &oid).map_err(|e| e.to_string())
}

// ── Background file-metadata scan ────────────────────────────────────────────

/// Walk history newest-first, attributing each tracked file to its last commit,
/// emitting `arbor://file-meta-batch` in batches and `arbor://file-meta-done`
/// when complete or cancelled. Cancels any previous scan for the same tab first.
#[arbor_rpc::handler]
fn start_file_meta_scan(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let sink = state.event_sink();
    let repo_path = repo_path(state, &tab_id)?;

    // Cancel any existing scan for this tab and register a fresh token.
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut tokens = SCAN_TOKENS.lock().unwrap();
        if let Some(old) = tokens.get(&tab_id) {
            old.store(true, Ordering::Relaxed);
        }
        tokens.insert(tab_id.clone(), Arc::clone(&cancel));
    }

    let sink_bg = Arc::clone(&sink);

    std::thread::spawn(move || {
        use git2::Sort;

        const BATCH_SIZE: usize = 50;
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

                // Cheap atomic cancellation check every 100 commits.
                if commit_count % 100 == 0 && cancel.load(Ordering::Relaxed) {
                    sink_bg.emit("arbor://file-meta-done", serde_json::json!(&tab_id));
                    return;
                }

                let oid = match oid_result { Ok(o) => o, Err(_) => continue };
                let commit = match repo.find_commit(oid) { Ok(c) => c, Err(_) => continue };
                let tree = match commit.tree() { Ok(t) => t, Err(_) => continue };
                let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
                let diff = match parent_tree {
                    Some(ref pt) => repo.diff_tree_to_tree(Some(pt), Some(&tree), Some(&mut diff_opts)),
                    None => repo.diff_tree_to_tree(None, Some(&tree), Some(&mut diff_opts)),
                };
                let diff = match diff { Ok(d) => d, Err(_) => continue };

                let oid_full = oid.to_string();
                let short_oid = oid_full[..7].to_string();
                let date = commit.time().seconds();
                let summary = commit.summary().unwrap_or("").to_string();

                for delta in diff.deltas() {
                    let candidates = [
                        delta.new_file().path().and_then(|p| p.to_str()),
                        delta.old_file().path().and_then(|p| p.to_str()),
                    ];
                    for path in candidates.into_iter().flatten() {
                        if let Some(entry) = entry_map.get_mut(path) {
                            if entry.last_commit_oid.is_none() {
                                entry.last_commit_oid = Some(oid_full.clone());
                                entry.last_commit_short_oid = Some(short_oid.clone());
                                entry.last_commit_date = Some(date);
                                entry.last_commit_summary = Some(summary.clone());
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

// ── SVG export ───────────────────────────────────────────────────────────────

/// Close out an SVG-export job: set its registry status, then emit the terminal
/// `arbor://job-done` + a `plugin:notification` — byte-identical to the
/// in-process `svg_finish_job`.
fn svg_finish_job(
    sink: &Arc<dyn EventSink>,
    job: &JobHandle,
    success: bool,
    message: &str,
) {
    let status = if success {
        JobStatus::Completed { exit_code: 0 }
    } else {
        JobStatus::Failed { error: message.to_string() }
    };
    job.set_status(status);

    sink.emit("arbor://job-done", serde_json::json!({
        "job_id":    job.id,
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

/// Export the full commit graph to an SVG file. Returns the job-id immediately;
/// the heavy work runs in a background thread, streaming progress via
/// `arbor://job-output` and finishing with a `plugin:notification`.
#[arbor_rpc::handler]
fn export_graph_svg(
    state: &CorvusState,
    tab_id: String,
    output_path: String,
    theme_vars: Option<HashMap<String, String>>,
) -> Result<String, String> {
    let host = state
        .host_caller()
        .ok_or_else(|| "export_graph_svg: no reverse channel for jobs".to_string())?;
    let sink = state.event_sink();
    let repo_path = repo_path(state, &tab_id)?;

    // Register the job in the shell registry (the shell mints the id).
    let job = JobHandle::register(
        host,
        JobSpec {
            name: "Export Graph as SVG".into(),
            plugin_name: "arbor".into(),
            command: format!("→ {output_path}"),
            category: Some("System".into()),
            non_cancellable: true,
            hidden: false,
            is_system: true,
            target: None,
        },
    )?;
    let job_id = job.id.clone();

    // Tell the FE the job exists so it renders in the overlay.
    sink.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        "Export Graph as SVG",
        "plugin_name": "arbor",
        "command":     format!("→ {output_path}"),
        "category":    "System",
    }));

    let sink_bg = Arc::clone(&sink);

    std::thread::spawn(move || {
        // One output line → the registry ring-buffer + the FE live stream.
        let emit_line = |line: &str| {
            job.append(line);
            sink_bg.emit("arbor://job-output", serde_json::json!({
                "job_id": &job.id,
                "text":   line,
            }));
        };

        emit_line("Opening repository…");
        let repo = match Repository::open(&repo_path) {
            Ok(r) => r,
            Err(e) => {
                let err = format!("Cannot open repo: {e}");
                emit_line(&format!("[error] {err}"));
                svg_finish_job(&sink_bg, &job, false, &err);
                return;
            }
        };

        emit_line("Loading full commit graph (this may take a moment for large repos)…");
        let graph = match corvus_git::graph::load_graph(&repo, 0, 999_999) {
            Ok(g) => g,
            Err(e) => {
                let err = format!("Failed to load graph: {e}");
                emit_line(&format!("[error] {err}"));
                svg_finish_job(&sink_bg, &job, false, &err);
                return;
            }
        };

        emit_line(&format!(
            "Graph loaded: {} commits, {} lanes.",
            graph.nodes.len(),
            graph.lane_count,
        ));

        let theme = corvus_git::graph_svg::ThemeColors::from_vars(&theme_vars.unwrap_or_default());

        match corvus_git::graph_svg::generate_svg_to_file(
            &graph,
            std::path::Path::new(&output_path),
            &theme,
            &emit_line,
        ) {
            Ok(()) => {
                let ok_msg = format!("Graph exported to '{output_path}'.");
                emit_line(&ok_msg);
                svg_finish_job(&sink_bg, &job, true, &ok_msg);
            }
            Err(e) => {
                emit_line(&format!("[error] {e}"));
                svg_finish_job(&sink_bg, &job, false, &e);
            }
        }
    });

    Ok(job_id)
}
