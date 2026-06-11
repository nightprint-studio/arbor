//! BE→FE event payloads + the event-name constants — the **frozen IPC contract**
//! (Onda 4) the front end (Fase 4) builds against.
//!
//! Everything grove pushes to the front end is scoped to the grove window
//! ([`emit`]). Cadence/coalescing is the caller's concern (the audio thread):
//! `transport`/`meters` go out on a fixed ~30 fps tick, `active_haps` only when
//! the sounding set changes, `diagnostics` only after an eval, `log` only above
//! the configured threshold (gated at the source). Keeping every payload here —
//! one typed struct per event, no ad-hoc `json!` — means the wire shape lives in
//! exactly one place, mirrored 1:1 by `src/lib/ipc/grove.ts`.
//!
//! **Field names are snake_case** to match the Rust structs verbatim (the TS
//! mirror reads them as-is). This contract is frozen; extend it additively.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::grove_window::GROVE_WINDOW_LABEL;

/// Diagnostics (errors with span) after a `grove_eval`. Empty `errors` = success.
pub const EVT_DIAGNOSTICS: &str = "grove:diagnostics";
/// Source spans currently sounding, for the live editor highlight (on change).
pub const EVT_ACTIVE_HAPS: &str = "grove:active_haps";
/// Audio telemetry: master + per-track peak, voice count, DSP load (~tick rate).
pub const EVT_METERS: &str = "grove:meters";
/// Transport state (playing, position, tempo) (~tick rate).
pub const EVT_TRANSPORT: &str = "grove:transport";
/// A log line from the running script (`debug`/`info`/… or per-hap `.log`).
pub const EVT_LOG: &str = "grove:log";
/// VSCO 2 download/extract progress (during an install job).
pub const EVT_VSCO_PROGRESS: &str = "grove:vsco_progress";
/// The audio device failed to open on the session thread (terminal for the play).
pub const EVT_AUDIO_ERROR: &str = "grove:audio_error";

/// One located diagnostic. `start`/`end` are byte offsets into the source
/// (`None` when the failure has no known location). `severity` is one of
/// `error` | `warning` | `info` (today the evaluator emits only `error`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub severity: &'static str,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

/// The `grove:diagnostics` payload, also returned by `grove_eval` so the caller
/// gets the result inline and via the event.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GroveDiagnostics {
    pub errors: Vec<Diagnostic>,
}

impl GroveDiagnostics {
    /// A clean (no-errors) result.
    pub fn ok() -> Self {
        GroveDiagnostics { errors: Vec::new() }
    }

    /// A single-error result.
    pub fn one(d: Diagnostic) -> Self {
        GroveDiagnostics { errors: vec![d] }
    }
}

/// The `grove:transport` payload: where the playhead is, the tempo, and whether
/// the scheduler is running. `sample_rate` is carried so the front end can map
/// `frame` → seconds without a side query (it's constant per session).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransportState {
    /// Whether the scheduler is running.
    pub playing: bool,
    /// Fractional cycle position at the current playhead.
    pub cycle: f64,
    /// Absolute output frame at the current playhead.
    pub frame: u64,
    /// Tempo in cycles-per-second in force at the playhead.
    pub cps: f64,
    /// Output sample rate (frames/second) of the live session.
    pub sample_rate: u32,
}

/// The `grove:meters` payload: audio-engine telemetry sampled at the tick rate.
/// Peaks are linear `0.0..~1.0`. `tracks` is indexed by mixer strip (same order
/// as the arrangement's tracks).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Meters {
    /// Master output peak `[left, right]` (post-limiter).
    pub master: [f32; 2],
    /// Per-track post-fader peak `[left, right]`, one entry per mixer strip.
    pub tracks: Vec<[f32; 2]>,
    /// Currently sounding voice count.
    pub voices: u32,
    /// DSP load `0.0..~1.0` (1.0 ≈ the audio callback is using its whole budget).
    pub dsp_load: f32,
}

/// One sounding source range, for the live editor highlight. `start`/`end` are
/// byte offsets into the source; `track` is the mixer-strip index that owns it
/// (so the highlight can be coloured per track).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActiveHap {
    pub start: u32,
    pub end: u32,
    pub track: u32,
}

/// The `grove:active_haps` payload: every source range sounding at the playhead.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActiveHaps {
    pub haps: Vec<ActiveHap>,
}

/// The `grove:log` payload: one (already threshold-gated) log line.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LogLine {
    /// `trace` | `debug` | `info` | `warn` | `error`.
    pub level: String,
    pub message: String,
}

/// The `grove:vsco_progress` payload during a VSCO 2 install job. `pct` is `-1`
/// when the total is unknown (a pre-sizing phase).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct VscoProgress {
    pub job_id: String,
    /// `downloading` | `extracting`.
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub pct: i64,
}

/// The `grove:audio_error` payload: the audio device could not be opened, so the
/// session thread exited (a `play` produced no sound).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AudioErrorEvent {
    pub message: String,
}

/// Emit a grove event to the grove window only. Best-effort: a missing window
/// (closed) is a silent no-op.
pub fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    let _ = app.emit_to(GROVE_WINDOW_LABEL, event, payload);
}
