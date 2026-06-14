//! The one curated entry point to the whole nemus API.
//!
//! `use arbor_nemus::prelude::*;` brings in the full public surface of the four
//! nemus crates. This is a **hand-curated** list (not four globs) for one reason:
//! the per-crate `Result` aliases collide. So every public item is re-exported by
//! name here **except** those four aliases, which are replaced by the facade's
//! unified [`NemusError`](crate::error::NemusError) + [`Result`](crate::error::Result).
//!
//! Maintenance: when a nemus crate adds a public item to *its* prelude, add it
//! here too (same-turn, like any prelude). Shared types (`ControlMap`, `Pattern`,
//! `SourceKind`, `TimeSpan`, …) are listed **once**, from `pattern`, since lang
//! and audio only re-export the identical item.

// ── Unified error (replaces the four per-crate `Result` aliases) ──────────────
pub use crate::error::{NemusError, Result};

// ── arbor-nemus-pattern — the pure algebra ───────────────────────────────────
// Core types, the four distinct combinator families, and the deterministic RNG.
// `Result` intentionally omitted (see above); `PatternError` kept by name.
pub use arbor_nemus_pattern::prelude::{
    arrange, audio, cat, choose, cycles, euclid_with, fast_with, fastcat, isaw, mode_table, par,
    parse_note, polymeter, pure, rand, sample, saw, section, section_layout, seq, silence, sine,
    slow_with, slowcat, square, stack, time_to_index, time_to_rand, timecat, track,
    track_with_sections, tracks, tri, ControlMap, Hap, HoldSpec, Param, Pattern, PatternError,
    Scale, ScaleMode, Section, SectionSpan, SourceKind, SourceSpan, TempoMap, Time, TimeSpan,
    Track, Tracks, MIDDLE_C,
};

// ── arbor-nemus-lang — text ↔ Pattern ────────────────────────────────────────
// AST, evaluator, injected plumbing, emitter/materialiser. `ControlMap`/`Pattern`/
// `SourceSpan`/`TimeSpan` are re-exported by lang too, but listed above (identical
// items); `Result` omitted; `LangError`/`LangErrorKind` kept by name.
pub use arbor_nemus_lang::prelude::{
    emit, emit_expr, evaluate, materialize_island, materialize_source, parse, reference, BinOp, Ctx,
    DslEntry, DslKind, DslParam, Env, EvalConfig, EvalOutput, Expr, ExprKind, FnDef, Func, Ident,
    Import, Island, IslandKind, Item, LangError, LangErrorKind, Leaf, LetBind, LogLevel, LogSink,
    Mini, MiniArg, MiniKind, NoImports, Postfix, Program, SilentLog, SourceLoader, Transform, UnOp,
    Value,
};

// ── arbor-nemus-audio — the DSP / real-time backend ──────────────────────────
// The engine↔audio seam (frozen core + Onda 2 additive mixer/EQ/comp/delay/
// reverb surface), the renderer + real-time stream, the sound registry, and the
// offline/test recorder. `SourceKind` listed above (identical); `Result` omitted;
// `AudioError` kept by name.
pub use arbor_nemus_audio::prelude::{
    list_manifest_instruments, list_output_devices, open_output_stream, AudioCommand, AudioDevice,
    AudioError, AudioSink, CompSettings, DecodedAudio, DelayConfig, EqBand, EqBandKind, Frame,
    InstrumentInfo, InstrumentKind, MeterSnapshot, MeterTap, OutputStream, Registry, Renderer,
    ReverbIr, StreamSink, SynthPreset, TrackConfig, VoiceEvent, VoiceId, VoiceParams, VoiceSource,
    Waveform, RecordingSink, DEFAULT_BLOCK_FRAMES, DEFAULT_SAMPLE_RATE, DEFAULT_VOICE_CAPACITY,
    MAX_METER_TRACKS,
};

// ── arbor-nemus-engine — the timing runtime ──────────────────────────────────
// Clock, the pure scheduling core, the real-time transport, and the offline
// render driver. `Result` omitted; `EngineError` kept by name.
pub use arbor_nemus_engine::prelude::{
    analyze_levels, delay_config_for, export_midi, render_offline, render_offline_with_progress,
    schedule_span, voice_event_from_hap, BitDepth, ClipWindow, Epoch, EngineError, Format,
    LevelAnalysis, MidiExportSummary, RenderConfig, RenderOutcome, RenderProgress, RenderSink,
    Transport, DEFAULT_BIT_DEPTH, DEFAULT_TAIL_MAX_SECS, LOOKAHEAD_MS,
};

// ── arbor-nemus-import — deterministic MIDI → .nemus (the "faithful" path) ────
// L1 transcode + L2 heuristics + idiomatic emit. `Result` omitted; `ImportError`
// kept by name. `Note`/`NoteTrack`/`Song` are this crate's own model (no clash
// with the pattern algebra).
pub use arbor_nemus_import::prelude::{
    degree_of, detect_key, midi_to_nemus, midi_to_song, quantize_song, recognize_chord,
    smf_to_nemus, song_to_nemus, sound_for_key, transcode_bytes, transcode_smf, Chord, DetectedKey,
    ImportError, ImportOptions, Note, NoteTrack, Song,
};

// ── arbor-nemus-transcribe — WAV → MIDI behind the `Transcriber` seam ─────────
// The trait + options/progress, the built-in DSP backend, the backend factory,
// and the note→MIDI writer. `Result` omitted; `TranscribeError` kept by name.
// `DecodedAudio` listed once above (audio block).
pub use arbor_nemus_transcribe::prelude::{
    notes_to_smf, transcriber_for, Backend, DetNote, DspTranscriber, ProgressFn, TranscribeError,
    TranscribeOptions, TranscribePhase, TranscribeProgress, Transcriber, DRUM_CHANNEL,
};
#[cfg(feature = "onnx")]
pub use arbor_nemus_transcribe::prelude::OnnxTranscriber;
