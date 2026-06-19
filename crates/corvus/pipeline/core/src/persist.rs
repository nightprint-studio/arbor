//! Run persistence — one JSON file per run under
//! `~/.config/arbor/pipeline_runs/<run_id>.json`.
//!
//! Pure FS + serde (no Tauri): the orchestrator host calls [`persist_run`]
//! after every state transition so a run survives an app restart, and
//! [`registry_from_disk`] rebuilds the registry at boot.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use corvus_pipeline_api::prelude::{LogEvent, LogLevel, PipelineRun, RunStatus};

use crate::registry::PipelineRegistry;

/// Cap on a single run's log buffer. The orchestrator trims to this after
/// each push; persistence enforces nothing beyond what it's handed.
pub const RUN_LOG_CAP: usize = 5_000;

/// Wall-clock milliseconds since the Unix epoch. `0` on a clock before the
/// epoch (never in practice). Used for run/step/log timestamps.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn run_store_dir() -> Option<PathBuf> {
    arbor_core::prelude::try_product_path(arbor_core::prelude::PRODUCT_CORVUS, "pipeline_runs")
}

pub fn persist_run(run: &PipelineRun) {
    let Some(dir) = run_store_dir() else { return; };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("pipeline: cannot create run store dir: {e}");
        return;
    }
    let path = dir.join(format!("{}.json", run.id));
    match serde_json::to_string_pretty(run) {
        Ok(s) => if let Err(e) = std::fs::write(&path, s) {
            tracing::warn!("pipeline: cannot persist run {}: {e}", run.id);
        },
        Err(e) => tracing::warn!("pipeline: cannot serialize run {}: {e}", run.id),
    }
}

pub fn remove_persisted_run(run_id: &str) {
    if let Some(dir) = run_store_dir() {
        let _ = std::fs::remove_file(dir.join(format!("{run_id}.json")));
    }
}

/// Build a `PipelineRegistry` pre-populated with runs restored from disk.
/// The internal `counter` is advanced past the highest recovered run id so
/// new runs don't collide with persisted files.
pub fn registry_from_disk() -> PipelineRegistry {
    let runs = load_persisted_runs();
    let max_id = runs.iter()
        .filter_map(|r| r.id.strip_prefix("pipe-run-"))
        .filter_map(|n| n.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    PipelineRegistry::from_recovered(runs, max_id)
}

/// Load previously persisted runs from disk. Runs that were still `Running`
/// or `Pending` at shutdown are coerced to `Failed` — they cannot be safely
/// resumed because their orchestrator thread died with the process.
pub fn load_persisted_runs() -> Vec<PipelineRun> {
    let Some(dir) = run_store_dir() else { return Vec::new(); };
    let Ok(iter) = std::fs::read_dir(&dir) else { return Vec::new(); };
    let mut out = Vec::new();
    for entry in iter.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
        let Ok(content) = std::fs::read_to_string(&path) else { continue; };
        let Ok(mut run) = serde_json::from_str::<PipelineRun>(&content) else { continue; };
        // A run marked Running/Pending at load-time lost its orchestrator —
        // mark it Failed with a sentinel log entry so the user sees why.
        // Also drops `queued` so a parked-at-shutdown run doesn't show a
        // "Queued" badge after recovery.
        if matches!(run.status, RunStatus::Running | RunStatus::Pending) {
            run.status = RunStatus::Failed;
            run.queued = false;
            if run.finished_at.is_none() { run.finished_at = Some(now_ms()); }
            run.log.push(LogEvent {
                ts:      now_ms(),
                level:   LogLevel::Warn,
                scope:   "pipeline".into(),
                message: "run state was recovered after app restart; marked as Failed"
                    .into(),
            });
            persist_run(&run);
        }
        out.push(run);
    }
    out
}
