//! Canonical entry point for `arbor-grove-audio`'s public API.
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

// ── DSP core + real-time output ──────────────────────────────────────────────
pub use crate::renderer::{Renderer, DEFAULT_VOICE_CAPACITY};
pub use crate::stream::{open_output_stream, OutputStream, StreamSink};

// ── Out-of-band telemetry tap (level meters, voices, DSP load) ────────────────
// Written by the real-time callback, read by the shell; additive, not part of
// the frozen command seam.
pub use crate::meters::{MeterSnapshot, MeterTap, MAX_METER_TRACKS};

// ── Sound registry (manifest → voice resolution) ─────────────────────────────
// The engine/shell build a `Registry` (load a TOML manifest, or install synth
// presets) and hand it to the `Renderer`; resolution itself is internal.
pub use crate::registry::{InstrumentInfo, InstrumentKind, Registry, SynthPreset};
pub use crate::synth::{NoiseColor, SynthShape, Waveform};

// ── Test / offline recorder sink ─────────────────────────────────────────────
pub use crate::testing::RecordingSink;

// ── Re-exported so consumers naming a file source's playback kind on the seam
// don't need a second `use` from the pattern crate. ──────────────────────────
pub use arbor_grove_pattern::prelude::SourceKind;
