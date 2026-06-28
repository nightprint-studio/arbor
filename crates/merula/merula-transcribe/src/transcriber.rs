//! The [`Transcriber`] trait — the swappable seam — plus its options and the
//! progress reporting it emits during a (potentially long) run.

use merula_audio::prelude::DecodedAudio;
use midly::Smf;

use crate::error::Result;

/// Knobs shared by every backend. A backend ignores what doesn't apply to it
/// (the DSP backend can't separate stems, for instance).
#[derive(Clone, Copy, Debug)]
pub struct TranscribeOptions {
    /// Request stem separation (Demucs) before pitch detection — cleaner but
    /// slower. ML backends honour it; the DSP backend runs on the mix regardless.
    pub split_stems: bool,
    /// Detect a melodic/harmonic part.
    pub detect_pitch: bool,
    /// Detect drums (onset detection → GM drum map).
    pub detect_drums: bool,
    /// Tempo (BPM) stamped into the output MIDI. We don't infer tempo; the caller
    /// supplies it (default 120) and the downstream converter derives `cps`.
    pub tempo_bpm: f64,
    /// Ticks-per-quarter resolution of the output MIDI.
    pub ppq: u16,
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        TranscribeOptions {
            split_stems: false,
            detect_pitch: true,
            detect_drums: true,
            tempo_bpm: 120.0,
            ppq: 480,
        }
    }
}

/// The stage a transcription is in, for progress UIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscribePhase {
    /// Separating stems (ML backends only).
    SeparatingStems,
    /// Estimating pitch / notes.
    DetectingPitch,
    /// Detecting drum onsets.
    DetectingOnsets,
    /// Writing the MIDI.
    Assembling,
}

/// A progress tick: which phase, and how far through it (`0..=1`).
#[derive(Clone, Copy, Debug)]
pub struct TranscribeProgress {
    pub phase: TranscribePhase,
    pub fraction: f32,
}

impl TranscribeProgress {
    pub fn new(phase: TranscribePhase, fraction: f32) -> Self {
        TranscribeProgress {
            phase,
            fraction: fraction.clamp(0.0, 1.0),
        }
    }
}

/// A callback invoked as a transcription advances. The shell forwards these as
/// job-progress events to the merula window.
pub type ProgressFn<'a> = dyn FnMut(TranscribeProgress) + 'a;

/// A WAV → MIDI transcriber. The interchange is a [`midly::Smf`] — owned
/// (`'static`), so it can be returned, kept in memory, and either handed to the
/// deterministic `.merula` converter or written to disk.
pub trait Transcriber: Send + Sync {
    /// A human-facing backend name (shown in the UI / logs).
    fn name(&self) -> &str;

    /// Transcribe already-decoded audio to MIDI, reporting progress. Long-running
    /// and CPU-bound — callers run it on a worker thread. Must never panic on bad
    /// input: return [`crate::error::TranscribeError`] instead.
    fn transcribe(
        &self,
        audio: &DecodedAudio,
        opts: &TranscribeOptions,
        progress: &mut ProgressFn<'_>,
    ) -> Result<Smf<'static>>;

    /// Convenience: decode a file and transcribe it.
    fn transcribe_file(
        &self,
        path: &std::path::Path,
        opts: &TranscribeOptions,
        progress: &mut ProgressFn<'_>,
    ) -> Result<Smf<'static>> {
        let audio = DecodedAudio::load(path)?;
        self.transcribe(&audio, opts, progress)
    }
}
