//! Audio / MIDI import commands: WAV → MIDI (transcription) and MIDI → `.nemus`
//! (deterministic), wired to the `arbor-nemus-transcribe` + `arbor-nemus-import`
//! crates.
//!
//! Three entry points, all off the audio RT path and the async runtime's worker
//! pool (the work is blocking CPU/IO):
//! - [`nemus_convert_wav_to_midi`] (D4) — transcribe a WAV and write a `.mid`.
//! - [`nemus_import_audio_as_nemus`] (D3) — transcribe a WAV and return idiomatic
//!   `.nemus` text; the transient MIDI never touches disk.
//! - [`nemus_import_midi_as_nemus`] (D5) — convert an existing `.mid` to `.nemus`
//!   (skips transcription).
//!
//! Long-running runs register a hidden job (routed to the nemus window) and emit
//! `arbor://job-progress`, so the Imports surface a live percentage without ever
//! blocking the UI. The chosen transcription backend is the built-in DSP one for
//! now (the ONNX backend plugs into the same `transcriber_for` seam later).

use std::path::PathBuf;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use arbor_nemus::prelude::{
    midi_to_nemus, smf_to_nemus, transcriber_for, Backend, DecodedAudio, ImportOptions,
    TranscribeOptions, TranscribePhase, TranscribeProgress,
};

use crate::error::AppError;
use crate::jobs::{JobInfo, JobRegistry, JobStatus};

/// Options accepted by every import command (all optional; sensible defaults).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImportOpts {
    // ── transcription (WAV → MIDI) ──
    /// Separate stems before pitch detection (ML backends only; DSP ignores it).
    pub split_stems: Option<bool>,
    /// Tempo (BPM) stamped into the MIDI; the converter derives `cps` from it.
    pub tempo_bpm: Option<f64>,
    /// Detect a pitched part.
    pub detect_pitch: Option<bool>,
    /// Detect a drum part.
    pub detect_drums: Option<bool>,
    // ── conversion (MIDI → .nemus) ──
    /// Quantisation grid (subdivisions per cycle); `0` keeps raw timing.
    pub grid: Option<u32>,
    /// Beats per cycle (bar length). `4` maps a 4/4 bar to one cycle.
    pub beats_per_cycle: Option<u32>,
}

impl ImportOpts {
    fn transcribe(&self) -> TranscribeOptions {
        let d = TranscribeOptions::default();
        TranscribeOptions {
            split_stems: self.split_stems.unwrap_or(d.split_stems),
            detect_pitch: self.detect_pitch.unwrap_or(d.detect_pitch),
            detect_drums: self.detect_drums.unwrap_or(d.detect_drums),
            tempo_bpm: self.tempo_bpm.unwrap_or(d.tempo_bpm),
            ppq: d.ppq,
        }
    }
    fn import(&self) -> ImportOptions {
        let d = ImportOptions::default();
        ImportOptions {
            grid: self.grid.unwrap_or(d.grid),
            beats_per_cycle: self.beats_per_cycle.unwrap_or(d.beats_per_cycle),
            ..d
        }
    }
}

/// Resolve `split_stems`: honour an explicit choice, else default it **on** when
/// a Demucs model is installed (downloading it is the opt-in) and off otherwise.
fn default_split_stems(o: &ImportOpts) -> bool {
    if let Some(v) = o.split_stems {
        return v;
    }
    #[cfg(feature = "onnx")]
    {
        let cfg = crate::nemus::config::load();
        return crate::nemus::models::is_installed(&cfg, crate::nemus::models::DEMUCS_ID);
    }
    #[allow(unreachable_code)]
    false
}

/// Pick the transcription backend: the ONNX one (basic-pitch, polyphonic; with
/// Demucs stem separation when that model is also installed) when its model is
/// installed, else the always-available built-in DSP backend.
fn select_transcriber() -> Box<dyn arbor_nemus::prelude::Transcriber> {
    #[cfg(feature = "onnx")]
    {
        use crate::nemus::models;
        let cfg = crate::nemus::config::load();
        if let Some(bp) = models::model_path(&cfg, models::BASIC_PITCH_ID) {
            if bp.exists() {
                let demucs = models::model_path(&cfg, models::DEMUCS_ID).filter(|p| p.exists());
                match arbor_nemus::prelude::OnnxTranscriber::new(&bp, demucs) {
                    Ok(t) => return Box::new(t),
                    Err(e) => tracing::warn!("nemus: ONNX backend init failed ({e}); using DSP fallback"),
                }
            }
        }
    }
    transcriber_for(Backend::BuiltinDsp)
}

// ── Commands ────────────────────────────────────────────────────────────────

/// D4 — transcribe a WAV and write a `.mid` to `output`. Returns the job id;
/// completion + progress are reported via `arbor://job-progress` / `job-done`.
#[tauri::command]
pub async fn nemus_convert_wav_to_midi(
    app: AppHandle,
    input: String,
    output: String,
    op_id: Option<String>,
    opts: Option<ImportOpts>,
) -> Result<String, AppError> {
    let o = opts.unwrap_or_default();
    let mut topts = o.transcribe();
    topts.split_stems = default_split_stems(&o);
    let in_path = PathBuf::from(input);
    let out_path = PathBuf::from(output);
    let job_id = register_job(&app, &format!("Convert {} → MIDI", in_path.display()), "wav-to-midi", op_id);

    let app_t = app.clone();
    let jid = job_id.clone();
    std::thread::Builder::new()
        .name(format!("nemus-wav2midi-{job_id}"))
        .spawn(move || {
            let mut last = -1i32;
            let outcome: std::result::Result<(), String> = (|| {
                let audio = DecodedAudio::load(&in_path).map_err(|e| e.to_string())?;
                let tr = select_transcriber();
                let smf = tr
                    .transcribe(&audio, &topts, &mut |p| report(&app_t, &jid, &mut last, p))
                    .map_err(|e| e.to_string())?;
                let mut buf = Vec::new();
                smf.write(&mut buf).map_err(|e| format!("midi write: {e}"))?;
                std::fs::write(&out_path, buf).map_err(|e| e.to_string())?;
                Ok(())
            })();
            finish_job(&app_t, &jid, &outcome);
        })
        .map_err(|e| AppError::Other(format!("failed to spawn transcription thread: {e}")))?;

    Ok(job_id)
}

/// D3 — transcribe a WAV and return idiomatic `.nemus` text (the MIDI stays in
/// memory). Progress rides the job system; the text is returned so the caller can
/// open it in a new tab.
#[tauri::command]
pub async fn nemus_import_audio_as_nemus(
    app: AppHandle,
    input: String,
    op_id: Option<String>,
    opts: Option<ImportOpts>,
) -> Result<String, AppError> {
    let o = opts.unwrap_or_default();
    let mut topts = o.transcribe();
    topts.split_stems = default_split_stems(&o);
    let iopts = o.import();
    let in_path = PathBuf::from(input);
    let job_id = register_job(&app, &format!("Import {} → .nemus", in_path.display()), "audio-to-nemus", op_id);

    let app_t = app.clone();
    let jid = job_id.clone();
    let work = tauri::async_runtime::spawn_blocking(move || -> std::result::Result<String, String> {
        let mut last = -1i32;
        let audio = DecodedAudio::load(&in_path).map_err(|e| e.to_string())?;
        let tr = select_transcriber();
        let smf = tr
            .transcribe(&audio, &topts, &mut |p| report(&app_t, &jid, &mut last, p))
            .map_err(|e| e.to_string())?;
        smf_to_nemus(&smf, &iopts).map_err(|e| e.to_string())
    })
    .await;

    let result: std::result::Result<String, String> = match work {
        Ok(r) => r,
        Err(e) => Err(format!("import task failed: {e}")),
    };
    finish_job(&app, &job_id, &result.as_ref().map(|_| ()).map_err(Clone::clone));
    result.map_err(AppError::Nemus)
}

/// D5 — convert an existing `.mid` file to idiomatic `.nemus` text. No
/// transcription; the deterministic path only. Fast enough not to need a job.
#[tauri::command]
pub async fn nemus_import_midi_as_nemus(
    input: String,
    opts: Option<ImportOpts>,
) -> Result<String, AppError> {
    let iopts = opts.unwrap_or_default().import();
    let in_path = PathBuf::from(input);
    let text = tauri::async_runtime::spawn_blocking(move || -> std::result::Result<String, String> {
        let bytes = std::fs::read(&in_path).map_err(|e| e.to_string())?;
        midi_to_nemus(&bytes, &iopts).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| AppError::Other(format!("import task failed: {e}")))?;
    text.map_err(AppError::Nemus)
}

// ── Progress + job plumbing (mirrors `render.rs`) ─────────────────────────────

/// Map a transcription progress tick to an overall percent and emit it (only on
/// whole-percent change, so a long run produces ~100 events).
fn report(app: &AppHandle, job_id: &str, last: &mut i32, p: TranscribeProgress) {
    let (base, span) = match p.phase {
        TranscribePhase::SeparatingStems => (0.0, 30.0),
        TranscribePhase::DetectingPitch => (30.0, 40.0),
        TranscribePhase::DetectingOnsets => (70.0, 20.0),
        TranscribePhase::Assembling => (90.0, 10.0),
    };
    let pct = (base + span * p.fraction as f64).round() as i32;
    if pct != *last {
        *last = pct;
        let _ = app.emit("arbor://job-progress", serde_json::json!({ "job_id": job_id, "pct": pct }));
    }
}

/// Register a hidden, nemus-routed job and announce it. When `preset_id` is given
/// (the FE generated it so it can track the transfer before the call returns), it
/// is used as the job id; otherwise one is allocated. Returns the job id (the
/// preset, or empty when the registry lock is poisoned — work still proceeds).
fn register_job(app: &AppHandle, name: &str, command: &str, preset_id: Option<String>) -> String {
    let state = app.state::<crate::AppState>();
    let id = {
        let mut jobs = match state.jobs.lock() {
            Ok(j) => j,
            Err(_) => return preset_id.unwrap_or_default(),
        };
        let id = preset_id.unwrap_or_else(|| jobs.new_id());
        jobs.register(JobInfo {
            id: id.clone(),
            name: name.to_string(),
            plugin_name: "nemus".to_string(),
            command: command.to_string(),
            started_at: JobRegistry::now_secs(),
            status: JobStatus::Running,
            category: Some("Imports".to_string()),
            non_cancellable: false,
            hidden: true,
            is_system: false,
            finished_at: None,
            target: Some("nemus".to_string()),
        });
        id
    };
    let _ = app.emit(
        "arbor://job-started",
        serde_json::json!({
            "job_id": &id, "name": name, "plugin_name": "nemus",
            "command": command, "category": "Imports", "hidden": true, "target": "nemus",
        }),
    );
    id
}

/// Finalise a job: set its registry status and emit `arbor://job-done`.
fn finish_job(app: &AppHandle, job_id: &str, outcome: &std::result::Result<(), String>) {
    let state = app.state::<crate::AppState>();
    let (status, success, error) = match outcome {
        Ok(()) => (JobStatus::Completed { exit_code: 0 }, true, None),
        Err(msg) => (JobStatus::Failed { error: msg.clone() }, false, Some(msg.clone())),
    };
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.set_status(job_id, status);
    }
    let _ = app.emit(
        "arbor://job-done",
        serde_json::json!({ "job_id": job_id, "success": success, "error": error }),
    );
}
