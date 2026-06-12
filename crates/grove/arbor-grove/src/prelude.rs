//! The one curated entry point to the whole grove API.
//!
//! `use arbor_grove::prelude::*;` brings in the full public surface of the four
//! grove crates. This is a **hand-curated** list (not four globs) for one reason:
//! the per-crate `Result` aliases collide. So every public item is re-exported by
//! name here **except** those four aliases, which are replaced by the facade's
//! unified [`GroveError`](crate::error::GroveError) + [`Result`](crate::error::Result).
//!
//! Maintenance: when a grove crate adds a public item to *its* prelude, add it
//! here too (same-turn, like any prelude). Shared types (`ControlMap`, `Pattern`,
//! `SourceKind`, `TimeSpan`, …) are listed **once**, from `pattern`, since lang
//! and audio only re-export the identical item.

// ── Unified error (replaces the four per-crate `Result` aliases) ──────────────
pub use crate::error::{GroveError, Result};

// ── arbor-grove-pattern — the pure algebra ───────────────────────────────────
// Core types, the four distinct combinator families, and the deterministic RNG.
// `Result` intentionally omitted (see above); `PatternError` kept by name.
pub use arbor_grove_pattern::prelude::{
    arrange, audio, cat, choose, cycles, euclid_with, fast_with, fastcat, isaw, par, parse_note,
    polymeter, pure, rand, sample, saw, section, section_layout, seq, silence, sine, slow_with,
    slowcat, square, stack, time_to_index, time_to_rand, timecat, track, track_with_sections,
    tracks, tri, ControlMap, Hap, Param, Pattern, PatternError, Scale, Section, SectionSpan,
    SourceKind, SourceSpan, TempoMap, Time, TimeSpan, Track, Tracks, MIDDLE_C,
};

// ── arbor-grove-lang — text ↔ Pattern ────────────────────────────────────────
// AST, evaluator, injected plumbing, emitter/materialiser. `ControlMap`/`Pattern`/
// `SourceSpan`/`TimeSpan` are re-exported by lang too, but listed above (identical
// items); `Result` omitted; `LangError`/`LangErrorKind` kept by name.
pub use arbor_grove_lang::prelude::{
    emit, emit_expr, evaluate, materialize_island, materialize_source, parse, reference, BinOp, Ctx,
    DslEntry, DslKind, DslParam, Env, EvalConfig, EvalOutput, Expr, ExprKind, FnDef, Func, Ident,
    Import, Island, IslandKind, Item, LangError, LangErrorKind, Leaf, LetBind, LogLevel, LogSink,
    Mini, MiniArg, MiniKind, NoImports, Postfix, Program, SilentLog, SourceLoader, Transform, UnOp,
    Value,
};

// ── arbor-grove-audio — the DSP / real-time backend ──────────────────────────
// The engine↔audio seam (frozen core + Onda 2 additive mixer/EQ/comp/delay/
// reverb surface), the renderer + real-time stream, the sound registry, and the
// offline/test recorder. `SourceKind` listed above (identical); `Result` omitted;
// `AudioError` kept by name.
pub use arbor_grove_audio::prelude::{
    open_output_stream, AudioCommand, AudioError, AudioSink, CompSettings, DelayConfig, EqBand,
    EqBandKind, Frame, InstrumentInfo, InstrumentKind, MeterSnapshot, MeterTap, OutputStream,
    Registry, Renderer, ReverbIr, StreamSink, SynthPreset, TrackConfig, VoiceEvent, VoiceId,
    VoiceParams, VoiceSource, Waveform, RecordingSink, DEFAULT_BLOCK_FRAMES, DEFAULT_SAMPLE_RATE,
    DEFAULT_VOICE_CAPACITY, MAX_METER_TRACKS,
};

// ── arbor-grove-engine — the timing runtime ──────────────────────────────────
// Clock, the pure scheduling core, the real-time transport, and the offline
// render driver. `Result` omitted; `EngineError` kept by name.
pub use arbor_grove_engine::prelude::{
    delay_config_for, render_offline, schedule_span, voice_event_from_hap, BitDepth, Epoch,
    EngineError, RenderConfig, Transport, DEFAULT_BIT_DEPTH, DEFAULT_TAIL_MAX_SECS, LOOKAHEAD_MS,
};
