//! The ONNX transcription backend (feature `onnx`).
//!
//! [`OnnxTranscriber`] runs **basic-pitch** for polyphonic pitch and the DSP
//! onset detector for the drum part (basic-pitch is pitched-only). When
//! `split_stems` is requested and a **Demucs** model is installed, it first
//! separates the mix into stems: drums are detected on the isolated kit and pitch
//! on the drum-free melodic sum — markedly cleaner than running on the raw mix.

pub mod basic_pitch;
pub mod demucs;

use std::path::{Path, PathBuf};

use arbor_nemus_audio::prelude::DecodedAudio;
use midly::Smf;
use ort::execution_providers::{CPUExecutionProvider, DirectMLExecutionProvider};
use ort::session::{builder::GraphOptimizationLevel, Session};

use crate::dsp::onset;
use crate::error::{Result, TranscribeError};
use crate::midi_out::notes_to_smf;
use crate::note::DetNote;
use crate::transcriber::{
    ProgressFn, TranscribeOptions, TranscribePhase, TranscribeProgress, Transcriber,
};

// ── Shared ONNX session setup (used by every model loader) ───────────────────

/// Wrap an `ort` error as a backend error.
pub(crate) fn oerr<E: std::fmt::Display>(e: E) -> TranscribeError {
    TranscribeError::Backend(format!("onnx: {e}"))
}

/// Open a session from `path`, preferring the **DirectML** execution provider
/// (GPU on Windows) and falling back to CPU.
///
/// Both EPs are registered best-effort: if DirectML can't initialise (no
/// compatible GPU / driver), `ort` skips it and uses the CPU provider, so this
/// never fails for lack of a GPU — it's a transparent speed-up. The model
/// download already bundles the DirectML-enabled onnxruntime, so there's nothing
/// extra to ship.
pub(crate) fn open_session(path: &Path) -> Result<Session> {
    Session::builder()
        .map_err(oerr)?
        .with_execution_providers([
            DirectMLExecutionProvider::default().build(),
            CPUExecutionProvider::default().build(),
        ])
        .map_err(oerr)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(oerr)?
        .commit_from_file(path)
        .map_err(oerr)
}

/// ONNX-backed transcriber: basic-pitch (pitch) + DSP onset (drums), optionally
/// preceded by Demucs stem separation.
pub struct OnnxTranscriber {
    bp: basic_pitch::Model,
    /// Loaded Demucs model when installed — enables the `split_stems` path.
    demucs: Option<demucs::Model>,
}

impl std::fmt::Debug for OnnxTranscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OnnxTranscriber")
    }
}

impl OnnxTranscriber {
    /// Load the basic-pitch model (required) and, if present, the Demucs stem
    /// separator. Both are loaded up front so a bad model fails fast.
    pub fn new(basic_pitch_path: &Path, demucs_path: Option<PathBuf>) -> Result<Self> {
        let demucs = match demucs_path {
            Some(p) => Some(demucs::load(&p)?),
            None => None,
        };
        Ok(OnnxTranscriber {
            bp: basic_pitch::load(basic_pitch_path)?,
            demucs,
        })
    }
}

impl Transcriber for OnnxTranscriber {
    fn name(&self) -> &str {
        if self.demucs.is_some() {
            "ONNX (basic-pitch + Demucs)"
        } else {
            "ONNX (basic-pitch)"
        }
    }

    fn transcribe(
        &self,
        audio: &DecodedAudio,
        opts: &TranscribeOptions,
        progress: &mut ProgressFn<'_>,
    ) -> Result<Smf<'static>> {
        let mut notes = Vec::new();

        // Stem-split path: detectors run on isolated stems.
        let separator = if opts.split_stems { self.demucs.as_ref() } else { None };
        if let Some(dm) = separator {
            progress(TranscribeProgress::new(TranscribePhase::SeparatingStems, 0.0));
            let stems = demucs::separate(dm, &audio.samples, audio.sample_rate, |f| {
                progress(TranscribeProgress::new(TranscribePhase::SeparatingStems, f));
            })?;
            collect_drums(&mut notes, &stems.drums, demucs::SR, opts, progress);
            if opts.detect_pitch {
                progress(TranscribeProgress::new(TranscribePhase::DetectingPitch, 0.0));
                notes.extend(basic_pitch::infer(&self.bp, &stems.melodic, demucs::SR, |f| {
                    progress(TranscribeProgress::new(TranscribePhase::DetectingPitch, f));
                })?);
            }
        } else {
            // Mix path: basic-pitch on the whole signal, drums via onset on the mix.
            if opts.detect_pitch {
                progress(TranscribeProgress::new(TranscribePhase::DetectingPitch, 0.0));
                notes.extend(basic_pitch::infer(&self.bp, &audio.samples, audio.sample_rate, |f| {
                    progress(TranscribeProgress::new(TranscribePhase::DetectingPitch, f));
                })?);
            }
            collect_drums(&mut notes, &audio.samples, audio.sample_rate, opts, progress);
        }

        if notes.is_empty() {
            return Err(TranscribeError::NoContent);
        }
        progress(TranscribeProgress::new(TranscribePhase::Assembling, 1.0));
        Ok(notes_to_smf(&notes, opts.tempo_bpm, opts.ppq))
    }
}

/// Append drum notes detected on `samples`, reporting the onset phase.
fn collect_drums(
    notes: &mut Vec<DetNote>,
    samples: &[f32],
    sample_rate: u32,
    opts: &TranscribeOptions,
    progress: &mut ProgressFn<'_>,
) {
    if !opts.detect_drums {
        return;
    }
    progress(TranscribeProgress::new(TranscribePhase::DetectingOnsets, 0.0));
    notes.extend(onset::detect_drums(samples, sample_rate));
    progress(TranscribeProgress::new(TranscribePhase::DetectingOnsets, 1.0));
}
