//! Shared download/install plumbing for every sample [`Pack`]: stream the
//! archive (hashing as we go), extract it, generate the pack's `registry.toml`,
//! and write an install marker — all **non-real-time**, tracked in the shared
//! [`JobRegistry`](crate::jobs::JobRegistry) (hard rule: the job system handles
//! downloads, never the audio RT path).
//!
//! Layout per pack under its install dir:
//! - `archive.zip`   — the GitHub archive, deleted after extraction
//! - `<repo>-<ref>/` — extracted tree, with a generated `registry.toml`
//! - `install.json`  — install marker (sha256, size, instrument count)

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use merula::prelude::{list_manifest_instruments, InstrumentInfo, Registry};

use super::{pack_dir, Pack, PackStatus};
use crate::merula::config::MerulaConfig;
use crate::merula::events::{emit, PackProgress, EVT_PACK_PROGRESS};
use crate::jobs::{JobInfo, JobRegistry, JobStatus};

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

/// The install status of one pack (installed marker, or a not-installed stub).
pub fn status(cfg: &MerulaConfig, pack: &Pack) -> PackStatus {
    let dir = pack_dir(cfg, pack.id);
    let path = dir.display().to_string();
    match read_manifest(&dir) {
        Some(m) => PackStatus {
            id: pack.id.to_string(),
            name: pack.name.to_string(),
            description: pack.description.to_string(),
            approx_bytes: pack.approx_bytes,
            installed: true,
            path,
            size_bytes: m.size_bytes,
            sha256: Some(m.sha256),
            instrument_count: m.instrument_count,
        },
        None => PackStatus {
            id: pack.id.to_string(),
            name: pack.name.to_string(),
            description: pack.description.to_string(),
            approx_bytes: pack.approx_bytes,
            installed: false,
            path,
            size_bytes: 0,
            sha256: None,
            instrument_count: 0,
        },
    }
}

/// The instrument names declared by an installed pack's registry (cheap header
/// scan — no sample decode). Empty when the pack isn't installed. Used by the
/// eval validator to know which names resolve without a full load.
pub fn installed_names(cfg: &MerulaConfig, pack: &Pack) -> Vec<String> {
    let dir = pack_dir(cfg, pack.id);
    let Some(manifest) = read_manifest(&dir) else {
        return Vec::new();
    };
    let registry_path = dir.join(&manifest.registry_rel);
    let Ok(text) = std::fs::read_to_string(&registry_path) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let Some(inner) = line.strip_prefix('[') else { continue };
        let Some(header) = inner.strip_suffix(']') else { continue };
        let name = header.trim().trim_matches('"').to_string();
        if !name.is_empty() {
            names.push(name);
        }
    }
    names
}

/// Append an installed pack's instruments — name / kind / articulations — to
/// `out` **without decoding samples** (no-op if not installed). The cheap
/// listing path for the sound bank; pairs with [`load_subset_into`], which
/// decodes (for playback) only the referenced instruments.
pub fn list_into(cfg: &MerulaConfig, pack: &Pack, out: &mut Vec<InstrumentInfo>) {
    let dir = pack_dir(cfg, pack.id);
    let Some(manifest) = read_manifest(&dir) else {
        return;
    };
    let registry_path = dir.join(&manifest.registry_rel);
    out.extend(list_manifest_instruments(&registry_path));
}

/// Merge **only** the entries named in `needed` from an installed pack's registry
/// into `reg` (no-op if not installed). The lazy-loading playback path: decode
/// just the instruments the arrangement references, not the whole pack. A load
/// failure is logged and skipped so the other packs still resolve.
pub fn load_subset_into(
    cfg: &MerulaConfig,
    pack: &Pack,
    reg: &mut Registry,
    needed: &HashSet<String>,
) {
    let dir = pack_dir(cfg, pack.id);
    let Some(manifest) = read_manifest(&dir) else {
        return;
    };
    let registry_path = dir.join(&manifest.registry_rel);
    if let Err(e) = reg.load_manifest_subset_into(&registry_path, needed) {
        tracing::warn!("merula: pack `{}` subset load failed ({e}); skipping", pack.id);
    }
}

/// Kick off a download+install for `pack` in the background, tracked as a job.
/// Returns the job id immediately; progress flows via the Jobs overlay +
/// `merula:pack_progress`.
pub fn start(app: &AppHandle, cfg: &MerulaConfig, pack: &'static Pack) -> String {
    let dir = pack_dir(cfg, pack.id);
    let state = app.state::<crate::AppState>();
    let name = format!("Download {} sample bank", pack.name);
    let command = pack.archive_url.to_string();
    let job_id = {
        let mut jobs = match state.jobs.lock() {
            Ok(j) => j,
            Err(_) => return String::new(),
        };
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id: id.clone(),
            name: name.clone(),
            plugin_name: "merula".to_string(),
            command: command.clone(),
            started_at: JobRegistry::now_secs(),
            status: JobStatus::Running,
            category: Some("Downloads".to_string()),
            non_cancellable: false,
            // Hidden from the Jobs panel / overlay / badge: the user-facing
            // surface is the merula **Downloads & Exports** overlay (live %, phase,
            // cancel, reveal) plus the inline Sound-bank progress, so a Jobs entry
            // would duplicate it. Still registered so cancel + the terminal event
            // have a registry entry (revealable via "Show hidden").
            hidden: true,
            is_system: false,
            finished_at: None,
            // Route to the merula window's feedback host so downloads surface
            // there (it also shows inline pack progress), not in main.
            target: Some("merula".to_string()),
        });
        id
    };
    // Register the job (hidden) so cancel + the terminal event have a registry
    // entry; the visible surface is the Transfers overlay + inline Sound-bank.
    let _ = app.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        &name,
        "plugin_name": "merula",
        "command":     &command,
        "category":    "Downloads",
        "hidden":      true,
        "target":      "merula",
    }));

    let app = app.clone();
    let job_id_task = job_id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = download_and_install(&app, &dir, pack, &job_id_task).await {
            finish_job(&app, &job_id_task, Err(e));
        }
    });

    job_id
}

/// Re-index an installed pack from its extracted tree (no network). Regenerates
/// `registry.toml` via the pack's [`Layout`](super::Layout) and rewrites the
/// install marker's instrument count, leaving the downloaded samples untouched.
///
/// `Err` when the pack isn't installed, its extracted tree is gone, or its layout
/// can't rebuild from the tree (the GM `.sf2` is deleted post-install, so GM can
/// only be rebuilt by re-downloading).
pub fn reindex(cfg: &MerulaConfig, pack: &Pack) -> Result<PackStatus, String> {
    let dir = pack_dir(cfg, pack.id);
    let manifest = read_manifest(&dir).ok_or_else(|| format!("pack `{}` is not installed", pack.id))?;
    if matches!(pack.layout, super::Layout::Sf2) {
        return Err("re-indexing isn't supported for General MIDI — re-download it to rebuild".to_string());
    }
    // The extracted tree's root is the parent of the registry path (`<repo>-<ref>/`).
    let registry_path = dir.join(&manifest.registry_rel);
    let root = registry_path
        .parent()
        .ok_or_else(|| "malformed install marker (no registry parent)".to_string())?
        .to_path_buf();
    if !root.exists() {
        return Err("the pack's extracted files are missing — re-download it".to_string());
    }

    let (toml, instrument_count) = super::layout::generate(&root, pack.layout);
    std::fs::write(&registry_path, toml).map_err(|e| format!("write registry: {e}"))?;
    write_manifest(&dir, &InstallManifest { instrument_count, ..manifest })?;
    Ok(status(cfg, pack))
}

// ── Internals ────────────────────────────────────────────────────────────────

/// Stream the archive to disk (hashing as we go), then extract + index it.
async fn download_and_install(
    app: &AppHandle,
    dir: &Path,
    pack: &'static Pack,
    job_id: &str,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use std::io::Write;

    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let archive_path = dir.join("archive.zip");

    let resp = reqwest::get(pack.archive_url)
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
        emit_progress(app, pack, job_id, "downloading", received, total, &mut last_pct);
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
            extract_and_index(&app_blocking, &dir_owned, &archive_owned, pack, &job_owned)
        })
        .await
        .map_err(|e| format!("extract task failed: {e}"))??;

    // Remove the archive; write the install marker.
    let _ = std::fs::remove_file(&archive_path);
    let manifest = InstallManifest {
        url: pack.archive_url.to_string(),
        sha256,
        size_bytes,
        instrument_count,
        registry_rel,
    };
    write_manifest(dir, &manifest)?;

    finish_job(app, job_id, Ok(JobOutcome::Completed));
    Ok(())
}

/// Unzip the archive into `dir`, generate the pack's `registry.toml`, and return
/// `(extracted_bytes, instrument_count, registry_rel)`.
fn extract_and_index(
    app: &AppHandle,
    dir: &Path,
    archive_path: &Path,
    pack: &'static Pack,
    job_id: &str,
) -> Result<(u64, usize, String), String> {
    // The GM pack downloads a single `.sf2` — convert it directly (wav + SFZ)
    // instead of unzipping a tree.
    if matches!(pack.layout, super::Layout::Sf2) {
        return super::gm::convert(app, dir, archive_path, pack, job_id);
    }

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
        // Track the archive's top-level dir (`<repo>-<ref>/`).
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
        emit_progress(app, pack, job_id, "extracting", i as u64 + 1, count as u64, &mut last_pct);
    }

    let root = root.ok_or_else(|| "empty archive".to_string())?;
    let (toml, instrument_count) = super::layout::generate(&root, pack.layout);
    let registry_path = root.join("registry.toml");
    std::fs::write(&registry_path, toml).map_err(|e| format!("write registry: {e}"))?;
    let registry_rel = registry_path
        .strip_prefix(dir)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| "registry.toml".to_string());

    Ok((extracted_bytes, instrument_count, registry_rel))
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
pub(super) fn is_cancelled(app: &AppHandle, job_id: &str) -> bool {
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
    pack: &Pack,
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
    emit(
        app,
        EVT_PACK_PROGRESS,
        PackProgress {
            job_id: job_id.to_string(),
            pack_id: pack.id.to_string(),
            phase: phase.to_string(),
            done,
            total,
            pct,
        },
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

/// Emit a single (un-throttled) progress event — used by the GM converter, whose
/// per-preset steps are already coarse enough not to need throttling.
pub(super) fn emit_phase(
    app: &AppHandle,
    pack: &Pack,
    job_id: &str,
    phase: &str,
    done: u64,
    total: u64,
) {
    let pct = if total > 0 {
        ((done as f64 / total as f64) * 100.0) as i64
    } else {
        -1
    };
    emit(
        app,
        EVT_PACK_PROGRESS,
        PackProgress {
            job_id: job_id.to_string(),
            pack_id: pack.id.to_string(),
            phase: phase.to_string(),
            done,
            total,
            pct,
        },
    );
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
        Err(e) => {
            tracing::warn!("merula: pack download/install failed ({job_id}): {e}");
            (JobStatus::Failed { error: e.clone() }, false, Some(e))
        }
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
