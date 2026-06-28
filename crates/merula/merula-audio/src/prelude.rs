//! Canonical entry point for `merula-audio`'s public API.
//!
//! Workspace convention: reach the public surface through `prelude` rather than
//! per-module paths. The submodules stay `pub` for rustdoc navigation only.

// ── Error ────────────────────────────────────────────────────────────────────
pub use crate::error::{AudioError, Result};

// ── Shared audio defaults (sample rate, processing block size) ─────────────────
pub use crate::defaults::{DEFAULT_BLOCK_FRAMES, DEFAULT_SAMPLE_RATE};

// ── The engine↔audio contract (frozen core + Onda 2 additive mixer surface) ───
pub use crate::seam::{
    AudioCommand, AudioSink, CompSettings, DelayConfig, EqBand, EqBandKind, Frame, ReverbIr,
    TrackConfig, VoiceEvent, VoiceId, VoiceParams, VoiceSource,
};

// ── Non-real-time file decode (mono f32 + source rate) ───────────────────────
pub use crate::decode::DecodedAudio;

// ── DSP core + real-time output ──────────────────────────────────────────────
pub use crate::renderer::{Renderer, DEFAULT_VOICE_CAPACITY};
pub use crate::stream::{
    list_output_devices, open_output_stream, AudioDevice, OutputStream, StreamSink,
};

// ── Out-of-band telemetry tap (level meters, voices, DSP load) ────────────────
// Written by the real-time callback, read by the shell; additive, not part of
// the frozen command seam.
pub use crate::meters::{MeterSnapshot, MeterTap, MAX_METER_TRACKS};

// ── Sound registry (manifest → voice resolution) ─────────────────────────────
// The engine/shell build a `Registry` (load a TOML manifest, or install synth
// presets) and hand it to the `Renderer`; resolution itself is internal.
pub use crate::registry::{
    list_manifest_instruments, InstrumentInfo, InstrumentKind, Registry, SynthPreset,
};
pub use crate::synth::{NoiseColor, SynthShape, Waveform};

// ── Speech synthesis (text → spoken-word sample source) ──────────────────────
// Rendered offline into a `DecodedAudio` (like a decoded file), then played
// through the normal `Sample` path. The `speech(...)` DSL source builds on this.
pub use crate::speech::{
    synthesize_speech, synthesize_speech_spec, SpeechEngineKind, SpeechParams,
};

// ── Test / offline recorder sink ─────────────────────────────────────────────
pub use crate::testing::RecordingSink;

// ── Re-exported so consumers naming a file source's playback kind on the seam
// don't need a second `use` from the pattern crate. ──────────────────────────
pub use merula_pattern::prelude::SourceKind;
