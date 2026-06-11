//! VSCO 2 Community Edition sample bank: storage, lazy download, and registry
//! wiring. All **non-real-time** — the download streams over HTTP and is tracked
//! in the shared [`JobRegistry`](crate::jobs::JobRegistry) (hard rule: the job
//! system handles download/render, never the audio RT path).
//!
//! Layout under the install dir (`<data>/arbor/grove/vsco` by default, or the
//! `[grove].vsco_dir` override):
//! - `archive.zip`            — the GitHub archive, deleted after extraction
//! - `VSCO-2-CE-<branch>/`    — extracted repo, with a generated `registry.toml`
//! - `install.json`           — install marker (sha256, size, instrument count)
//!
//! The registry maps each extracted `.sfz` to a dotted sound name
//! (`bank.instrument`); names that don't resolve still fall back to the synth, so
//! grove always makes a sound whether or not VSCO is installed.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use arbor_grove::prelude::Registry;

use super::config::GroveConfig;
use crate::jobs::{JobInfo, JobRegistry, JobStatus};

/// The GitHub archive of the full VSCO 2 CE repo (architecture decision: full
/// from `sgossner/VSCO-2-CE`, on-demand). A moving branch tarball — we record
/// the downloaded sha256 for integrity rather than pinning a known one.
const VSCO_ARCHIVE_URL: &str =
    "https://github.com/sgossner/VSCO-2-CE/archive/refs/heads/master.zip";

/// Progress event for the grove window during a VSCO download.
const EVT_VSCO_PROGRESS: &str = "grove:vsco_progress";

/// Install marker, written after a successful extract+index.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallManifest {
    url: String,
    sha256: String,
    size_bytes: u64,
    instrument_count: usize,
    /// Registry TOML path, relative to the install dir.
    registry_rel: String,
}

/// Reported install state of the VSCO bank.
#[derive(Debug, Clone, Serialize)]
pub struct VscoStatus {
    pub installed: bool,
    pub path: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub instrument_count: usize,
}

/// The VSCO install directory for the given config.
pub fn vsco_dir(cfg: &GroveConfig) -> PathBuf {
    match &cfg.vsco_dir {
        Some(dir) => PathBuf::from(dir),
        None => arbor_core::prelude::arbor_data_dir().join("grove").join("vsco"),
    }
}

/// Read the current install status.
pub fn status(cfg: &GroveConfig) -> VscoStatus {
    let dir = vsco_dir(cfg);
    let path = dir.display().to_string();
    match read_manifest(&dir) {
        Some(m) => VscoStatus {
            installed: true,
            path,
            size_bytes: m.size_bytes,
            sha256: Some(m.sha256),
            instrument_count: m.instrument_count,
        },
        None => VscoStatus {
            installed: false,
            path,
            size_bytes: 0,
            sha256: None,
            instrument_count: 0,
        },
    }
}

/// Load the VSCO sound registry, if installed. Built on the audio thread and
/// handed to `open_output_stream`; `None` → the default synth bank.
pub fn load_registry(cfg: &GroveConfig) -> Option<Registry> {
    let dir = vsco_dir(cfg);
    let manifest = read_manifest(&dir)?;
    let registry_path = dir.join(&manifest.registry_rel);
    match Registry::load_manifest(&registry_path) {
        Ok(reg) => Some(reg),
        Err(e) => {
            tracing::warn!("grove: VSCO registry load failed ({e}); using default synth bank");
            None
        }
    }
}

/// Kick off a download+install in the background, tracked as a job. Returns the
/// job id immediately; progress flows via the Jobs overlay + `grove:vsco_progress`.
pub fn start_download(app: &AppHandle, cfg: &GroveConfig) -> String {
    let dir = vsco_dir(cfg);
    let state = app.state::<crate::AppState>();
    let job_id = {
        let mut jobs = match state.jobs.lock() {
            Ok(j) => j,
            Err(_) => return String::new(),
        };
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id: id.clone(),
            name: "Download VSCO 2 sample bank".to_string(),
            plugin_name: "grove".to_string(),
            command: VSCO_ARCHIVE_URL.to_string(),
            started_at: JobRegistry::now_secs(),
            status: JobStatus::Running,
            category: Some("Downloads".to_string()),
            non_cancellable: false,
            hidden: false,
            is_system: false,
            finished_at: None,
        });
        id
    };

    let app = app.clone();
    let job_id_task = job_id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = download_and_install(&app, &dir, &job_id_task).await {
            finish_job(&app, &job_id_task, Err(e));
        }
    });

    job_id
}

// ── Internals ────────────────────────────────────────────────────────────────

/// Stream the archive to disk (hashing as we go), then extract + index it.
async fn download_and_install(
    app: &AppHandle,
    dir: &Path,
    job_id: &str,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let archive_path = dir.join("archive.zip");

    let resp = reqwest::get(VSCO_ARCHIVE_URL)
        .await
        .map_err(|e| format!("request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("server error: {e}"))?;
    let total = resp.content_length().unwrap_or(0);

    let mut file =
        std::fs::File::create(&archive_path).map_err(|e| format!("create archive: {e}"))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut last_pct: i64 = -1;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        if is_cancelled(app, job_id) {
            let _ = std::fs::remove_file(&archive_path);
            finish_job(app, job_id, Ok(JobOutcome::Cancelled));
            return Ok(());
        }
        let chunk = chunk.map_err(|e| format!("download interrupted: {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|e| format!("write archive: {e}"))?;
        received += chunk.len() as u64;
        emit_progress(app, job_id, "downloading", received, total, &mut last_pct);
    }
    file.flush().map_err(|e| format!("flush archive: {e}"))?;
    let sha256 = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    // Extract + index off the async worker (sync zip + fs walk).
    let dir_owned = dir.to_path_buf();
    let archive_owned = archive_path.clone();
    let app_blocking = app.clone();
    let job_owned = job_id.to_string();
    let (size_bytes, instrument_count, registry_rel) =
        tauri::async_runtime::spawn_blocking(move || {
            extract_and_index(&app_blocking, &dir_owned, &archive_owned, &job_owned, received)
        })
        .await
        .map_err(|e| format!("extract task failed: {e}"))??;

    // Remove the archive; write the install marker.
    let _ = std::fs::remove_file(&archive_path);
    let manifest = InstallManifest {
        url: VSCO_ARCHIVE_URL.to_string(),
        sha256,
        size_bytes,
        instrument_count,
        registry_rel,
    };
    write_manifest(dir, &manifest)?;

    finish_job(app, job_id, Ok(JobOutcome::Completed));
    Ok(())
}

/// Unzip the archive into `dir`, generate `registry.toml`, and return
/// `(extracted_bytes, instrument_count, registry_rel)`.
fn extract_and_index(
    app: &AppHandle,
    dir: &Path,
    archive_path: &Path,
    job_id: &str,
    _download_bytes: u64,
) -> Result<(u64, usize, String), String> {
    let file = std::fs::File::open(archive_path).map_err(|e| format!("open archive: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("read archive: {e}"))?;

    let count = zip.len();
    let mut extracted_bytes: u64 = 0;
    let mut root: Option<PathBuf> = None;
    let mut last_pct: i64 = -1;

    for i in 0..count {
        if is_cancelled(app, job_id) {
            return Err("cancelled".to_string());
        }
        let mut entry = zip.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let out_path = dir.join(&rel);
        // Track the archive's top-level dir (`VSCO-2-CE-<branch>/`).
        if root.is_none() {
            if let Some(first) = rel.components().next() {
                root = Some(dir.join(first.as_os_str()));
            }
        }
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| format!("mkdir: {e}"))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
            }
            let mut out =
                std::fs::File::create(&out_path).map_err(|e| format!("create file: {e}"))?;
            extracted_bytes +=
                std::io::copy(&mut entry, &mut out).map_err(|e| format!("extract: {e}"))?;
        }
        emit_progress(app, job_id, "extracting", i as u64 + 1, count as u64, &mut last_pct);
    }

    let root = root.ok_or_else(|| "empty archive".to_string())?;
    let (toml, instrument_count) = generate_registry_toml(&root);
    let registry_path = root.join("registry.toml");
    std::fs::write(&registry_path, toml).map_err(|e| format!("write registry: {e}"))?;
    let registry_rel = registry_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| "registry.toml".to_string());

    Ok((extracted_bytes, instrument_count, registry_rel))
}

/// Scan `root` for `.sfz` instruments and build a TOML sound registry. Each
/// instrument is named `<parent-folder>.<file-stem>` (a dotted bank.instrument),
/// with its `.sfz` path relative to `root` (how `load_manifest` resolves it).
fn generate_registry_toml(root: &Path) -> (String, usize) {
    let mut sfz: Vec<PathBuf> = Vec::new();
    collect_sfz(root, &mut sfz);
    sfz.sort();

    let mut out = String::from("# Auto-generated VSCO 2 sound registry (grove).\n\n");
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut count = 0;
    for path in &sfz {
        let Ok(rel) = path.strip_prefix(root) else { continue };
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string());
        let bank = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string());
        let (Some(stem), Some(bank)) = (stem, bank) else { continue };
        let name = format!("{}.{}", sanitize(&bank), sanitize(&stem));
        if !seen.insert(name.clone()) {
            continue;
        }
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        out.push_str(&format!("[\"{name}\"]\nkind = \"sfz\"\nfile = \"{rel_str}\"\n\n"));
        count += 1;
    }
    (out, count)
}

/// Recursively collect `.sfz` files under `dir`.
fn collect_sfz(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sfz(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("sfz")) {
            out.push(path);
        }
    }
}

/// Normalise a path segment into a registry-name-safe token (lowercase, spaces
/// and odd chars → `_`).
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

fn manifest_path(dir: &Path) -> PathBuf {
    dir.join("install.json")
}

fn read_manifest(dir: &Path) -> Option<InstallManifest> {
    let text = std::fs::read_to_string(manifest_path(dir)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_manifest(dir: &Path, manifest: &InstallManifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(manifest).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(manifest_path(dir), text).map_err(|e| format!("write marker: {e}"))
}

/// True when the user cancelled the job.
fn is_cancelled(app: &AppHandle, job_id: &str) -> bool {
    let state = app.state::<crate::AppState>();
    state
        .jobs
        .lock()
        .map(|j| j.is_cancelled(job_id))
        .unwrap_or(false)
}

/// Emit a throttled progress event (only when the integer percentage changes)
/// and log a coarse line into the job output.
fn emit_progress(
    app: &AppHandle,
    job_id: &str,
    phase: &str,
    done: u64,
    total: u64,
    last_pct: &mut i64,
) {
    let pct = if total > 0 {
        ((done as f64 / total as f64) * 100.0) as i64
    } else {
        -1
    };
    if pct == *last_pct {
        return;
    }
    *last_pct = pct;
    let _ = app.emit_to(
        crate::grove_window::GROVE_WINDOW_LABEL,
        EVT_VSCO_PROGRESS,
        serde_json::json!({ "job_id": job_id, "phase": phase, "done": done, "total": total, "pct": pct }),
    );
    let state = app.state::<crate::AppState>();
    if let Ok(mut jobs) = state.jobs.lock() {
        let line = if pct >= 0 {
            format!("[{phase}] {pct}% ({done}/{total})")
        } else {
            format!("[{phase}] {done} bytes")
        };
        jobs.append_output(job_id, line);
    };
}

/// Terminal job outcome.
enum JobOutcome {
    Completed,
    Cancelled,
}

/// Set the job's terminal status and emit `arbor://job-done`.
fn finish_job(app: &AppHandle, job_id: &str, result: Result<JobOutcome, String>) {
    let (status, success, error) = match result {
        Ok(JobOutcome::Completed) => (JobStatus::Completed { exit_code: 0 }, true, None),
        Ok(JobOutcome::Cancelled) => (JobStatus::Cancelled, false, None),
        Err(e) => (JobStatus::Failed { error: e.clone() }, false, Some(e)),
    };
    let state = app.state::<crate::AppState>();
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.set_status(job_id, status);
    }
    let _ = app.emit(
        "arbor://job-done",
        serde_json::json!({ "job_id": job_id, "success": success, "error": error }),
    );
}
