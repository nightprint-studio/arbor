//! Downloadable ONNX **transcription models** (basic-pitch, Demucs), fetched
//! on-demand so the base bundle stays light (only onnxruntime is linked in).
//!
//! Mirrors the sample-pack download plumbing but for a single file each: stream
//! the `.onnx` to `<nemus-data>/models/<file>` with progress, tracked as a hidden
//! job routed to the nemus window. The transcriber backend selection
//! (`nemus::import`) checks [`is_installed`] / [`model_path`] to pick the ONNX
//! path over the built-in DSP one.
//!
//! ⚠️ The default URLs are best-effort and may move — they're overridable in the
//! nemus config (`basic_pitch_url` / `demucs_url`) so the artifact source can be
//! pointed anywhere without a rebuild.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::jobs::{JobInfo, JobRegistry, JobStatus};
use crate::nemus::config::{self, NemusConfig};

/// Stable model ids (used in commands + filenames).
pub const BASIC_PITCH_ID: &str = "basic-pitch";
pub const DEMUCS_ID: &str = "demucs";

/// Default download URLs, **overridable** via the nemus config (`basic_pitch_url`
/// / `demucs_url`).
///
/// basic-pitch's ONNX (`nmp.onnx`) ships in Spotify's own GitHub repo — the raw
/// path below is verified. Demucs has **no official ONNX** export (the project
/// ships PyTorch weights); we use the community HT-Demucs FT *drums specialist*
/// (StemSplitio), whose I/O matches `demucs.rs`. We need only drums (pitch runs on
/// `mix − drums`), so the single drums model is enough. If the URL moves, set
/// `demucs_url` to a compatible `htdemucs` ONNX — or skip Demucs entirely (the DSP
/// onset detector still handles drums on the mix).
const BASIC_PITCH_DEFAULT_URL: &str =
    "https://raw.githubusercontent.com/spotify/basic-pitch/main/basic_pitch/saved_models/icassp_2022/nmp.onnx";
const DEMUCS_DEFAULT_URL: &str =
    "https://huggingface.co/StemSplitio/htdemucs-ft-onnx/resolve/main/htdemucs_ft_drums.onnx";

/// A declarative downloadable model.
struct ModelDesc {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    filename: &'static str,
    approx_bytes: u64,
}

const MODELS: &[ModelDesc] = &[
    ModelDesc {
        id: BASIC_PITCH_ID,
        name: "basic-pitch (polyphonic pitch)",
        description: "Spotify's lightweight polyphonic note transcription model. \
            Far better than the built-in DSP pitch on real, polyphonic audio. Small.",
        filename: "basic-pitch.onnx",
        approx_bytes: 17_000_000,
    },
    ModelDesc {
        id: DEMUCS_ID,
        name: "Demucs (stem separation)",
        description: "HT-Demucs drums separator — removes percussion before pitch \
            detection (and isolates the kit) for cleaner notes. Large; optional.",
        filename: "htdemucs.onnx",
        approx_bytes: 316_000_000,
    },
];

fn desc(id: &str) -> Option<&'static ModelDesc> {
    MODELS.iter().find(|m| m.id == id)
}

/// Directory holding the downloaded models (`<nemus-data>/models` by default).
pub fn models_dir(cfg: &NemusConfig) -> PathBuf {
    match &cfg.models_dir {
        Some(d) => PathBuf::from(d),
        None => arbor_core::prelude::nemus_data_dir().join("models"),
    }
}

/// On-disk path of a model's file (whether or not it's downloaded yet).
pub fn model_path(cfg: &NemusConfig, id: &str) -> Option<PathBuf> {
    desc(id).map(|m| models_dir(cfg).join(m.filename))
}

/// Whether a model has been downloaded.
pub fn is_installed(cfg: &NemusConfig, id: &str) -> bool {
    model_path(cfg, id).map(|p| p.exists()).unwrap_or(false)
}

/// The download URL for a model — the config override, else the built-in default.
fn url_for(cfg: &NemusConfig, id: &str) -> Option<String> {
    match id {
        BASIC_PITCH_ID => Some(cfg.basic_pitch_url.clone().unwrap_or_else(|| BASIC_PITCH_DEFAULT_URL.to_string())),
        DEMUCS_ID => Some(cfg.demucs_url.clone().unwrap_or_else(|| DEMUCS_DEFAULT_URL.to_string())),
        _ => None,
    }
}

/// The reported state of one model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub approx_bytes: u64,
    pub installed: bool,
    pub path: String,
    pub size_bytes: u64,
}

fn status(cfg: &NemusConfig, m: &ModelDesc) -> ModelStatus {
    let path = models_dir(cfg).join(m.filename);
    let size_bytes = std::fs::metadata(&path).map(|md| md.len()).unwrap_or(0);
    ModelStatus {
        id: m.id.to_string(),
        name: m.name.to_string(),
        description: m.description.to_string(),
        approx_bytes: m.approx_bytes,
        installed: path.exists(),
        path: path.display().to_string(),
        size_bytes,
    }
}

// ── Commands ────────────────────────────────────────────────────────────────

/// List every transcription model with its install state.
#[tauri::command]
pub async fn nemus_models() -> Result<Vec<ModelStatus>, crate::error::AppError> {
    let cfg = config::load();
    Ok(MODELS.iter().map(|m| status(&cfg, m)).collect())
}

/// Start a background download of model `id`. Returns the job id; progress flows
/// via `arbor://job-progress` / `job-done` (routed to the nemus window).
#[tauri::command]
pub async fn nemus_download_model(app: AppHandle, id: String) -> Result<String, crate::error::AppError> {
    let cfg = config::load();
    let m = desc(&id).ok_or_else(|| crate::error::AppError::Other(format!("unknown model `{id}`")))?;
    let url = url_for(&cfg, &id)
        .ok_or_else(|| crate::error::AppError::Other(format!("no URL for model `{id}`")))?;
    let dest = models_dir(&cfg).join(m.filename);
    Ok(start_download(&app, &id, m.name, &url, dest))
}

/// Delete a downloaded model.
#[tauri::command]
pub async fn nemus_delete_model(id: String) -> Result<(), crate::error::AppError> {
    let cfg = config::load();
    let Some(path) = model_path(&cfg, &id) else {
        return Err(crate::error::AppError::Other(format!("unknown model `{id}`")));
    };
    if path.exists() {
        std::fs::remove_file(&path).map_err(crate::error::AppError::Io)?;
    }
    Ok(())
}

// ── Download plumbing (single file, mirrors packs/download.rs) ─────────────────

fn start_download(app: &AppHandle, id: &str, name: &str, url: &str, dest: PathBuf) -> String {
    let state = app.state::<crate::AppState>();
    let job_name = format!("Download {name}");
    let job_id = {
        let mut jobs = match state.jobs.lock() {
            Ok(j) => j,
            Err(_) => return String::new(),
        };
        let jid = jobs.new_id();
        jobs.register(JobInfo {
            id: jid.clone(),
            name: job_name.clone(),
            plugin_name: "nemus".to_string(),
            command: format!("download model {id}"),
            started_at: JobRegistry::now_secs(),
            status: JobStatus::Running,
            category: Some("Downloads".to_string()),
            non_cancellable: false,
            hidden: true,
            is_system: false,
            finished_at: None,
            target: Some("nemus".to_string()),
        });
        jid
    };
    let _ = app.emit(
        "arbor://job-started",
        serde_json::json!({
            "job_id": &job_id, "name": &job_name, "plugin_name": "nemus",
            "command": format!("download model {id}"), "category": "Downloads",
            "hidden": true, "target": "nemus",
        }),
    );

    let app = app.clone();
    let url = url.to_string();
    let jid = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = download_file(&app, &url, &dest, &jid).await;
        finish_job(&app, &jid, outcome);
    });
    job_id
}

/// Stream `url` to `dest` (via a `.part` temp + rename), emitting throttled
/// progress on the job.
async fn download_file(app: &AppHandle, url: &str, dest: &Path, job_id: &str) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::io::Write;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create models dir: {e}"))?;
    }
    let tmp = dest.with_extension("part");

    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("server error: {e}"))?;
    let total = resp.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&tmp).map_err(|e| format!("create file: {e}"))?;
    let mut received: u64 = 0;
    let mut last_pct: i32 = -1;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download interrupted: {e}"))?;
        file.write_all(&chunk).map_err(|e| format!("write: {e}"))?;
        received += chunk.len() as u64;
        let pct = if total > 0 {
            ((received as f64 / total as f64) * 100.0) as i32
        } else {
            -1
        };
        if pct != last_pct {
            last_pct = pct;
            let _ = app.emit(
                "arbor://job-progress",
                serde_json::json!({ "job_id": job_id, "pct": pct }),
            );
        }
    }
    file.flush().map_err(|e| format!("flush: {e}"))?;
    drop(file);
    std::fs::rename(&tmp, dest).map_err(|e| format!("finalize: {e}"))?;
    Ok(())
}

fn finish_job(app: &AppHandle, job_id: &str, outcome: Result<(), String>) {
    let state = app.state::<crate::AppState>();
    let (status, success, error) = match outcome {
        Ok(()) => (JobStatus::Completed { exit_code: 0 }, true, None),
        Err(e) => (JobStatus::Failed { error: e.clone() }, false, Some(e)),
    };
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.set_status(job_id, status);
    }
    let _ = app.emit(
        "arbor://job-done",
        serde_json::json!({ "job_id": job_id, "success": success, "error": error }),
    );
}
