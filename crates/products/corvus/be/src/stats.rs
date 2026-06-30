//! `stats` domain — repository-statistics handlers served **out-of-process** by
//! corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::stats`). Both commands spawn background work that
//! outlives the call and emits frontend events from inside it; the background
//! worker captures [`CorvusState::event_sink`] (`Arc<dyn EventSink>`) — never an
//! `AppHandle` — exactly the shape the OOP boundary needs. The pure git work
//! (commit walk, line-stat aggregation, JSON/HTML rendering) is the shared
//! [`corvus_git::stats`] crate, so [`RepoStats`] and the export file format are
//! byte-identical to in-process.
//!
//! **`export_repo_stats`** mirrors the security export's job mechanics: it
//! registers the job in the shell's single-source registry over the reverse
//! channel ([`JobHandle`]), snapshots the branding logo via the `__branding_logo`
//! host call (the OOP twin of the shell's `state.branding.snapshot().logo_svg`),
//! reads the repo display name from the libgit2 handle corvus-be already opens,
//! and emits the byte-identical `arbor://job-started` / `arbor://job-output` /
//! `arbor://job-done` payloads + `plugin:notification` toast.
//!
//! The per-repo `stats_exclude` config is read straight from the repo's
//! `.arbor/config.toml` (the same direct-read precedent as the gitflow domain) —
//! the shell owns the file, but corvus-be opens the workdir anyway.
//!
//! Read/export domain — **no hooks fire here**.
//!
//! ## Memoisation
//! Full parity with the in-process copy: `CorvusState` owns the same two pieces
//! the shell's `AppState` does — a per-tab `stats_cache` (keyed by a HEAD +
//! stats-exclude fingerprint) and a `stats_computing` dedup guard — held as JSON
//! so `corvus-core` stays git2-free (this handler serializes `RepoStats` in/out).
//! `compute_repo_stats` short-circuits on a cache hit (synchronous
//! `arbor://repo-stats-ready` from the cached value) and no-ops a duplicate
//! concurrent run; `export_repo_stats` reuses a cached `RepoStats` when present.
//! The corvus-be cache is process-local (its own twin of the shell's), so OOP
//! and in-process each memoise their own computations.

use std::sync::Arc;

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::EventSink;
use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{compute_stats, export_to_file, RepoStats, StatsExclude};
use serde::Deserialize;

use crate::jobs::JobHandle;
use crate::repo::open;

/// Just the `stats_exclude` slice of a repo's `.arbor/config.toml` — serde
/// ignores the rest of the file. Mirrors `RepoConfig.stats_exclude` shell-side.
#[derive(Deserialize, Default)]
struct RepoStatsExclude {
    #[serde(default)]
    stats_exclude: StatsExcludeSlice,
}

/// The wire shape of the shell's `StatsExcludeConfig` (`extensions`/`folders`/
/// `files`), deserialized straight from TOML and adapted into the crate-owned
/// [`StatsExclude`] value-struct (which is not itself `Deserialize`).
#[derive(Deserialize, Default)]
struct StatsExcludeSlice {
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    folders: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
}

impl From<StatsExcludeSlice> for StatsExclude {
    fn from(s: StatsExcludeSlice) -> Self {
        StatsExclude {
            extensions: s.extensions,
            folders: s.folders,
            files: s.files,
        }
    }
}

/// Resolve the per-repo `stats_exclude` config from the repo's
/// `.arbor/config.toml`, falling back to the empty default when the file is
/// absent / unparseable. Direct-read off the workdir corvus-be already has the
/// path for — the OOP twin of `config::repo_config::load(path).stats_exclude`.
fn stats_exclude_for(repo_path: &str) -> StatsExclude {
    let per_repo_file = std::path::Path::new(repo_path).join(".arbor").join("config.toml");
    std::fs::read_to_string(&per_repo_file)
        .ok()
        .and_then(|s| toml::from_str::<RepoStatsExclude>(&s).ok())
        .map(|c| c.stats_exclude.into())
        .unwrap_or_default()
}

/// Mark an export job done and emit the completion event + notification toast.
/// Drives the shell's single-source registry through [`JobHandle::set_status`]
/// and emits via the captured event sink — byte-identical to the shell's
/// in-process `stats_finish_job`.
fn stats_finish_job(
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
        ("Stats export complete", "success")
    } else {
        ("Stats export failed", "error")
    };
    sink.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   title,
        "message": message,
        "level":   level,
    }));
}

/// Export repository statistics to a JSON or HTML file.
///
/// Returns a job-id immediately; the export runs in a background task.
/// Emits `arbor://job-started`, `arbor://job-output`, `arbor://job-done` and
/// `plugin:notification` so the export shows up in the Jobs overlay.
#[arbor_rpc::handler]
fn export_repo_stats(
    state: &CorvusState,
    tab_id: String,
    output_path: String,
    format: String,
) -> Result<String, String> {
    let sink = state.event_sink();

    // The reverse channel: the job registry lives in the shell, and the
    // branding logo is read with a host call.
    let host = state
        .host_caller()
        .ok_or_else(|| "host caller unavailable".to_string())?;

    // Grab repo path + display name from the libgit2 handle corvus-be already
    // opens for this tab (workdir folder; falls back to tab_id).
    let (repo_path, repo_name) = {
        let repo = open(state, &tab_id)?;
        let path = repo
            .workdir()
            .unwrap_or_else(|| repo.path())
            .to_path_buf();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| tab_id.clone());
        (path, name)
    };

    // Reuse the memoised stats if `compute_repo_stats` already produced them for
    // this tab — keyed by `tab_id` regardless of the cache fingerprint, matching
    // the in-process copy's `c.get(&tab_id).map(|(_, s)| s.clone())`.
    let cached_stats: Option<RepoStats> = state
        .stats_cache()
        .lock()
        .ok()
        .and_then(|c| c.get(&tab_id).and_then(|(_, v)| serde_json::from_value(v.clone()).ok()));

    // Register a job entry so it appears in the Jobs overlay immediately.
    let job = JobHandle::register(Arc::clone(&host), JobSpec {
        name:            format!("Export Stats as {}", format.to_uppercase()),
        plugin_name:     "arbor".into(),
        command:         format!("→ {output_path}"),
        category:        Some("Export".into()),
        non_cancellable: true,
        hidden:          false,
        is_system:       true,
        target:          None,
    })?;
    let job_id = job.id.clone();

    sink.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        format!("Export Stats as {}", format.to_uppercase()),
        "plugin_name": "arbor",
        "command":     format!("→ {output_path}"),
        "category":    "Export",
    }));

    // Snapshot the branding logo override over the reverse channel — the OOP
    // twin of `state.branding.snapshot().logo_svg`. Co-branded exports pick up
    // the same logo the user sees.
    let logo_override: Option<String> = host
        .call("__branding_logo", serde_json::Value::Null)
        .ok()
        .and_then(|v| serde_json::from_value(v).ok());

    let sink_bg = Arc::clone(&sink);

    std::thread::spawn(move || {
        let emit_line = |line: &str| {
            job.append(line);
            sink_bg.emit("arbor://job-output", serde_json::json!({
                "job_id": &job.id,
                "text":   line,
            }));
        };

        // Use cached stats if available, otherwise compute fresh.
        let stats = if let Some(s) = cached_stats {
            emit_line("Using cached statistics…");
            s
        } else {
            emit_line("Computing repository statistics…");
            let repo = match git2::Repository::open(&repo_path) {
                Ok(r)  => r,
                Err(e) => {
                    let err = format!("Cannot open repo: {e}");
                    emit_line(&format!("[error] {err}"));
                    stats_finish_job(&sink_bg, &job, false, &err);
                    return;
                }
            };
            let excl = stats_exclude_for(&repo_path.to_string_lossy());
            match compute_stats(&repo, &excl) {
                Ok(s)  => s,
                Err(e) => {
                    let err = format!("Failed to compute stats: {e}");
                    emit_line(&format!("[error] {err}"));
                    stats_finish_job(&sink_bg, &job, false, &err);
                    return;
                }
            }
        };

        emit_line(&format!("Writing {format} export…"));
        match export_to_file(
            &stats,
            std::path::Path::new(&output_path),
            &format,
            &repo_name,
            logo_override.as_deref(),
        ) {
            Ok(()) => {
                let ok_msg = format!("Stats exported to '{output_path}'.");
                emit_line(&ok_msg);
                stats_finish_job(&sink_bg, &job, true, &ok_msg);
            }
            Err(e) => {
                emit_line(&format!("[error] {e}"));
                stats_finish_job(&sink_bg, &job, false, &e);
            }
        }
    });

    Ok(job_id)
}

/// Kick off a background stats computation for the given tab.
///
/// Returns immediately (Ok). The result arrives as a frontend event:
///   - `arbor://repo-stats-ready`  { tab_id, stats }  — success
///   - `arbor://repo-stats-error`  { tab_id, error }  — failure
#[arbor_rpc::handler]
fn compute_repo_stats(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let sink = state.event_sink();

    // Repo path + HEAD sha + exclusion config (read off the workdir corvus-be
    // opens). HEAD feeds the cache key so a HEAD move invalidates the entry.
    let (repo_path, head_sha, exclude) = {
        let repo = open(state, &tab_id)?;
        let sha = repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .map(|oid| oid.to_string())
            .unwrap_or_default();
        let path = repo
            .workdir()
            .unwrap_or_else(|| repo.path())
            .to_string_lossy()
            .into_owned();
        let excl = stats_exclude_for(&path);
        (path, sha, excl)
    };

    // Cache key fingerprints HEAD + the exclusion config so changing either always
    // invalidates the cached result (identical shape to the in-process copy).
    let exclude_key = format!(
        "ext:{};folders:{};files:{}",
        exclude.extensions.join(","),
        exclude.folders.join(","),
        exclude.files.join(","),
    );
    let cache_key = format!("{head_sha}|{exclude_key}");

    // Return the cached result immediately if HEAD + exclusions haven't changed.
    {
        let cache = state.stats_cache();
        let guard = cache.lock().map_err(|_| "stats_cache mutex poisoned".to_string())?;
        if let Some((cached_key, cached_stats)) = guard.get(&tab_id) {
            if *cached_key == cache_key {
                let stats = cached_stats.clone();
                drop(guard);
                sink.emit("arbor://repo-stats-ready", serde_json::json!({
                    "tab_id": &tab_id,
                    "stats": stats,
                }));
                return Ok(());
            }
        }
    }

    // Guard against duplicate concurrent runs (last one wins when it finishes).
    {
        let computing = state.stats_computing();
        let mut guard = computing.lock().map_err(|_| "stats_computing mutex poisoned".to_string())?;
        if guard.contains(&tab_id) {
            return Ok(());
        }
        guard.insert(tab_id.clone());
    }

    let cache_arc = state.stats_cache();
    let computing_arc = state.stats_computing();
    let tab_id_bg = tab_id.clone();

    std::thread::spawn(move || {
        let result = (|| -> std::result::Result<RepoStats, Box<dyn std::error::Error + Send + Sync>> {
            let repo = git2::Repository::open(&repo_path)?;
            Ok(compute_stats(&repo, &exclude)?)
        })();

        // Always unmark as computing, even on error.
        if let Ok(mut computing) = computing_arc.lock() {
            computing.remove(&tab_id_bg);
        }

        match result {
            Ok(stats) => {
                // Store the RepoStats as JSON (CorvusState stays git2-free).
                if let Ok(stats_json) = serde_json::to_value(&stats) {
                    if let Ok(mut cache) = cache_arc.lock() {
                        cache.insert(tab_id_bg.clone(), (cache_key, stats_json));
                    }
                }
                sink.emit("arbor://repo-stats-ready", serde_json::json!({
                    "tab_id": &tab_id_bg,
                    "stats": stats,
                }));
            }
            Err(e) => {
                eprintln!("corvus-be: stats computation failed for tab {tab_id_bg}: {e}");
                sink.emit("arbor://repo-stats-error", serde_json::json!({
                    "tab_id": &tab_id_bg,
                    "error": e.to_string(),
                }));
            }
        }
    });

    Ok(())
}
