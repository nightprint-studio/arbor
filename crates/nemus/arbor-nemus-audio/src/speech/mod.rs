//! Speech synthesis — text → spoken-word audio, usable as a one-shot sample
//! source in the nemus DSL (`speech("…")`).
//!
//! Speech is rendered **offline** (never in the RT callback) into a
//! [`DecodedAudio`] buffer, exactly like a decoded sample file; the shell caches
//! it and the renderer plays it through the normal `Sample` path, so the usual
//! transforms (`chop`, `speed`, `slow`, …) apply for free.
//!
//! ## Engines
//!
//! - [`SpeechEngineKind::Sam`] — Software Automatic Mouth: a retro, inherently
//!   "electronic" formant synth. Pure compute, deterministic, English-centric.
//!   A faithful port of `discordier/sam-js` (vendored under a fair-use
//!   determination by the project owner).
//! - [`SpeechEngineKind::System`] — the host OS text-to-speech (WinRT
//!   `SpeechSynthesizer` on Windows; other platforms fall back to SAM for now).
//!   Higher intelligibility + multilingual via `voice`/`lang`.

pub mod sam;
mod system;

use arbor_nemus_pattern::prelude::{SpeechEngine, SpeechSpec};

use crate::decode::DecodedAudio;

/// SAM renders at a fixed 22050 Hz; the sampler resamples to the device rate.
const SAM_SAMPLE_RATE: u32 = 22050;

/// Which speech engine renders the text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpeechEngineKind {
    /// Software Automatic Mouth — the electronic-voice default.
    #[default]
    Sam,
    /// The host operating system's text-to-speech.
    System,
}

/// Parameters for a speech-synthesis request. Defaults give the SAM default
/// voice. Engine-specific knobs that don't apply to the chosen engine are
/// ignored here (the DSL front-end flags incompatible combinations as warnings).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpeechParams {
    pub engine: SpeechEngineKind,
    /// SAM pitch (0–255, default 64).
    pub pitch: u8,
    /// SAM utterance rate — the DSL's `.rate()` (0–255, default 72). Named to
    /// avoid colliding with the playback-`speed` transform applied downstream.
    pub rate: u8,
    /// SAM mouth (F1) formant openness (0–255, default 128).
    pub mouth: u8,
    /// SAM throat (F2) formant openness (0–255, default 128).
    pub throat: u8,
    /// System-engine voice name (substring match against the OS voices).
    pub voice: Option<String>,
    /// System-engine BCP-47 language tag (e.g. `"fr-FR"`).
    pub lang: Option<String>,
    /// SAM "sing" mode: hold a monotone pitch instead of the speech contour.
    pub singmode: bool,
    /// Treat the input as a raw phoneme string instead of English text.
    pub phonetic: bool,
}

/// SAM-default parameters (the `SpeechParams::default()` derive zeroes the SAM
/// knobs, which is wrong — SAM wants 64/72/128/128). Use this for a fresh SAM
/// request.
impl SpeechParams {
    /// A SAM request with the original SAM knob defaults.
    pub fn sam_defaults() -> Self {
        SpeechParams {
            engine: SpeechEngineKind::Sam,
            pitch: 64,
            rate: 72,
            mouth: 128,
            throat: 128,
            voice: None,
            lang: None,
            singmode: false,
            phonetic: false,
        }
    }
}

/// Render `text` to a decoded audio buffer using the selected engine. Runs
/// offline (allocates, does real work) — call it on a worker thread, never in
/// the audio callback. An empty result means the text produced no audio.
pub fn synthesize_speech(text: &str, params: &SpeechParams) -> DecodedAudio {
    let (samples, sample_rate) = match params.engine {
        SpeechEngineKind::Sam => (sam_render(text, params), SAM_SAMPLE_RATE),
        SpeechEngineKind::System => {
            // The OS engine returns its own rate; fall back to SAM (so non-Windows
            // and any failure still speaks, just with the electronic voice).
            let (s, r) = system::synthesize(text, params);
            if s.is_empty() {
                (sam_render(text, params), SAM_SAMPLE_RATE)
            } else {
                (s, r)
            }
        }
    };
    DecodedAudio { samples, sample_rate }
}

/// Render a pattern-layer [`SpeechSpec`] (the DSL request) to audio. Centralises
/// the `SpeechSpec → SpeechParams` mapping so every caller (the live shell and
/// the offline render) shares one conversion.
pub fn synthesize_speech_spec(spec: &SpeechSpec) -> DecodedAudio {
    let params = SpeechParams {
        engine: match spec.engine {
            SpeechEngine::Sam => SpeechEngineKind::Sam,
            SpeechEngine::System => SpeechEngineKind::System,
        },
        pitch: spec.pitch,
        rate: spec.rate,
        mouth: spec.mouth,
        throat: spec.throat,
        voice: spec.voice.clone(),
        lang: spec.lang.clone(),
        singmode: spec.singmode,
        phonetic: spec.phonetic,
    };
    synthesize_speech(&spec.text, &params)
}

/// Drive the SAM engine from the engine-agnostic params.
fn sam_render(text: &str, params: &SpeechParams) -> Vec<f32> {
    sam::synthesize(
        text,
        &sam::SamConfig {
            pitch: params.pitch,
            speed: params.rate,
            mouth: params.mouth,
            throat: params.throat,
            singmode: params.singmode,
            phonetic: params.phonetic,
        },
    )
}
