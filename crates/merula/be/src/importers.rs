//! importers — audio / MIDI → `.merula` conversion, wired to the merula-transcribe
//! + merula-import crates through the facade.
//!
//! Ported from `src-tauri/src/merula/import.rs`. Three entry points, all off the
//! audio RT path:
//! - [`merula_convert_wav_to_midi`] (D4) — transcribe a WAV and write a `.mid`.
//! - [`merula_import_audio_as_merula`] (D3) — transcribe a WAV and return idiomatic
//!   `.merula` text; the transient MIDI never touches disk.
//! - [`merula_import_midi_as_merula`] (D5) — convert an existing `.mid` to `.merula`
//!   (deterministic; skips transcription).
//!
//! The two transcription runs (D4/D3) are long-running, so they register a hidden
//! job (routed to the merula window) and emit `arbor://job-progress` via the
//! reverse-channel [`JobHandle`]. The transcription itself runs **inline on the
//! dispatcher worker** (each backend request already has its own worker thread —
//! the transcribe + write is blocking CPU/IO, never the audio RT thread, never
//! `tauri::async_runtime`). The job exists so the *Downloads & Exports* overlay can
//! show a live percentage; the call still resolves with the result. D5 is fast
//! enough not to need a job. The chosen backend is the built-in DSP one (the ONNX
//! backend plugs into the same `transcriber_for` seam under the `onnx` feature).

use std::path::PathBuf;

use merula::prelude::{
    midi_to_merula, smf_to_merula, transcriber_for, Backend, DecodedAudio, ImportOptions,
    TranscribeOptions, TranscribePhase, TranscribeProgress, Transcriber,
};
use serde::Deserialize;

use crate::jobs::{category, JobHandle};
use crate::state::MerulaState;

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
    // ── conversion (MIDI → .merula) ──
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

/// Resolve `split_stems`: honour an explicit choice, else default it **on** when a
/// Demucs model is installed (downloading it is the opt-in) and off otherwise. The
/// `onnx`-gated branch reads the installed-model state from the sibling `models`
/// domain; without the feature it is always off (only the DSP backend ships).
fn default_split_stems(o: &ImportOpts) -> bool {
    if let Some(v) = o.split_stems {
        return v;
    }
    #[cfg(feature = "onnx")]
    {
        let cfg = crate::config_cmds::load();
        return crate::models::is_installed(&cfg, crate::models::DEMUCS_ID);
    }
    #[allow(unreachable_code)]
    false
}

/// Pick the transcription backend: the ONNX one (basic-pitch, polyphonic; with
/// Demucs stem separation when that model is also installed) when its model is
/// installed, else the always-available built-in DSP backend.
fn select_transcriber() -> Box<dyn Transcriber> {
    #[cfg(feature = "onnx")]
    {
        use crate::models;
        let cfg = crate::config_cmds::load();
        if let Some(bp) = models::model_path(&cfg, models::BASIC_PITCH_ID) {
            if bp.exists() {
                let demucs = models::model_path(&cfg, models::DEMUCS_ID).filter(|p| p.exists());
                match merula::prelude::OnnxTranscriber::new(&bp, demucs) {
                    Ok(t) => return Box::new(t),
                    Err(e) => {
                        eprintln!("merula-be: ONNX backend init failed ({e}); using DSP fallback")
                    }
                }
            }
        }
    }
    transcriber_for(Backend::BuiltinDsp)
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// D4 — transcribe a WAV and write a `.mid` to `output`. Returns the job id;
/// progress + completion are reported via `arbor://job-progress` / `job-done`.
#[arbor_rpc::handler]
fn merula_convert_wav_to_midi(
    ctx: &MerulaState,
    input: String,
    output: String,
    // The FE-generated `op_id` is accepted for wire-compat but no longer used as the
    // job id: the shell's `__job_register` mints the authoritative id (the registry
    // is shell-side), so the FE tracks the transfer by the id this call returns.
    op_id: Option<String>,
    opts: Option<ImportOpts>,
) -> Result<String, String> {
    let _ = op_id;
    let o = opts.unwrap_or_default();
    let mut topts = o.transcribe();
    topts.split_stems = default_split_stems(&o);
    let in_path = PathBuf::from(input);
    let out_path = PathBuf::from(output);

    let host = ctx
        .host_caller()
        .ok_or_else(|| "merula_convert_wav_to_midi: no reverse channel".to_string())?;
    let job = JobHandle::register(
        host,
        ctx.event_sink(),
        &format!("Convert {} → MIDI", in_path.display()),
        "wav-to-midi",
        category::IMPORTS,
    )?;

    // The transcribe + SMF write is blocking CPU/IO; run it inline on this request's
    // worker (never the audio RT thread). The job's live % rides the progress
    // callback; the call resolves with the job id once registered, but completes the
    // work before returning (faithful to the in-process command, which spawned a
    // thread only because Tauri commands share a worker pool — merula-be requests do
    // not).
    let mut last = -1i32;
    let result: Result<(), String> = (|| {
        let audio = DecodedAudio::load(&in_path).map_err(|e| e.to_string())?;
        let tr = select_transcriber();
        let smf = tr
            .transcribe(&audio, &topts, &mut |p| report(&job, &mut last, p))
            .map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        smf.write(&mut buf).map_err(|e| format!("midi write: {e}"))?;
        std::fs::write(&out_path, buf).map_err(|e| e.to_string())?;
        Ok(())
    })();

    let job_id = job.id.clone();
    match &result {
        Ok(()) => job.finish_ok(),
        Err(e) => job.finish_failed(e.clone()),
    }
    result.map(|()| job_id)
}

/// D3 — transcribe a WAV and return idiomatic `.merula` text (the MIDI stays in
/// memory). Progress rides the job system; the text is returned so the caller can
/// open it in a new tab.
#[arbor_rpc::handler]
fn merula_import_audio_as_merula(
    ctx: &MerulaState,
    input: String,
    // Accepted for wire-compat; see `merula_convert_wav_to_midi` — the returned job
    // id is authoritative now that the registry is shell-side.
    op_id: Option<String>,
    opts: Option<ImportOpts>,
) -> Result<String, String> {
    let _ = op_id;
    let o = opts.unwrap_or_default();
    let mut topts = o.transcribe();
    topts.split_stems = default_split_stems(&o);
    let iopts = o.import();
    let in_path = PathBuf::from(input);

    let host = ctx
        .host_caller()
        .ok_or_else(|| "merula_import_audio_as_merula: no reverse channel".to_string())?;
    let job = JobHandle::register(
        host,
        ctx.event_sink(),
        &format!("Import {} → .merula", in_path.display()),
        "audio-to-merula",
        category::IMPORTS,
    )?;

    let mut last = -1i32;
    let result: Result<String, String> = (|| {
        let audio = DecodedAudio::load(&in_path).map_err(|e| e.to_string())?;
        let tr = select_transcriber();
        let smf = tr
            .transcribe(&audio, &topts, &mut |p| report(&job, &mut last, p))
            .map_err(|e| e.to_string())?;
        smf_to_merula(&smf, &iopts).map_err(|e| e.to_string())
    })();

    match &result {
        Ok(_) => job.finish_ok(),
        Err(e) => job.finish_failed(e.clone()),
    }
    result
}

/// D5 — convert an existing `.mid` file to idiomatic `.merula` text. No
/// transcription; the deterministic path only. Fast enough not to need a job.
#[arbor_rpc::handler]
fn merula_import_midi_as_merula(
    _ctx: &MerulaState,
    input: String,
    opts: Option<ImportOpts>,
) -> Result<String, String> {
    let iopts = opts.unwrap_or_default().import();
    let bytes = std::fs::read(PathBuf::from(input)).map_err(|e| e.to_string())?;
    midi_to_merula(&bytes, &iopts).map_err(|e| e.to_string())
}

// ── Progress ──────────────────────────────────────────────────────────────────

/// Map a transcription progress tick to an overall percent and emit it on the job
/// (only on whole-percent change, so a long run produces ~100 events).
fn report(job: &JobHandle, last: &mut i32, p: TranscribeProgress) {
    let (base, span) = match p.phase {
        TranscribePhase::SeparatingStems => (0.0, 30.0),
        TranscribePhase::DetectingPitch => (30.0, 40.0),
        TranscribePhase::DetectingOnsets => (70.0, 20.0),
        TranscribePhase::Assembling => (90.0, 10.0),
    };
    let pct = (base + span * p.fraction as f64).round() as i32;
    if pct != *last {
        *last = pct;
        job.emit_progress(pct);
    }
}
