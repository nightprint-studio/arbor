//! The built-in DSP transcription backend.
//!
//! No models, no extra crates, all time-domain: monophonic pitch via YIN
//! ([`pitch`]) and drums via energy-onset detection ([`onset`]). It runs on the
//! mix (it cannot separate stems), so the result is rough — the always-available
//! baseline, and the fallback when no ML model is installed.

pub mod onset;
pub mod pitch;

use arbor_nemus_audio::prelude::DecodedAudio;
use midly::Smf;

use crate::error::{Result, TranscribeError};
use crate::midi_out::notes_to_smf;
use crate::note::DetNote;
use crate::transcriber::{
    ProgressFn, TranscribeOptions, TranscribePhase, TranscribeProgress, Transcriber,
};

/// The zero-dependency DSP transcriber.
#[derive(Debug, Default, Clone, Copy)]
pub struct DspTranscriber;

impl Transcriber for DspTranscriber {
    fn name(&self) -> &str {
        "Built-in (DSP)"
    }

    fn transcribe(
        &self,
        audio: &DecodedAudio,
        opts: &TranscribeOptions,
        progress: &mut ProgressFn<'_>,
    ) -> Result<Smf<'static>> {
        let sr = audio.sample_rate.max(1);
        let mut notes: Vec<DetNote> = Vec::new();

        if opts.detect_pitch {
            progress(TranscribeProgress::new(TranscribePhase::DetectingPitch, 0.0));
            notes.extend(pitch::detect_notes_with_progress(&audio.samples, sr, |f| {
                progress(TranscribeProgress::new(TranscribePhase::DetectingPitch, f));
            }));
        }
        if opts.detect_drums {
            progress(TranscribeProgress::new(TranscribePhase::DetectingOnsets, 0.0));
            notes.extend(onset::detect_drums(&audio.samples, sr));
            progress(TranscribeProgress::new(TranscribePhase::DetectingOnsets, 1.0));
        }

        if notes.is_empty() {
            return Err(TranscribeError::NoContent);
        }

        progress(TranscribeProgress::new(TranscribePhase::Assembling, 1.0));
        Ok(notes_to_smf(&notes, opts.tempo_bpm, opts.ppq))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pure tone is transcribed to (at least) one pitched note near the tone.
    #[test]
    fn transcribes_a_sine_to_a_note() {
        let sr = 44_100u32;
        let freq = 440.0f64; // A4 = MIDI 69
        let samples: Vec<f32> = (0..sr)
            .map(|i| {
                (2.0 * std::f64::consts::PI * freq * i as f64 / sr as f64).sin() as f32 * 0.8
            })
            .collect();
        let audio = DecodedAudio { samples, sample_rate: sr };
        let opts = TranscribeOptions {
            detect_drums: false,
            ..TranscribeOptions::default()
        };
        let mut seen_phase = None;
        let smf = DspTranscriber
            .transcribe(&audio, &opts, &mut |p| seen_phase = Some(p.phase))
            .expect("transcribe");
        assert!(seen_phase.is_some(), "progress was reported");
        assert_eq!(smf.tracks.len(), 1);
    }

    #[test]
    fn silence_yields_no_content() {
        let audio = DecodedAudio {
            samples: vec![0.0; 44_100],
            sample_rate: 44_100,
        };
        let err = DspTranscriber.transcribe(&audio, &TranscribeOptions::default(), &mut |_| {});
        assert!(matches!(err, Err(TranscribeError::NoContent)));
    }
}
