//! BE→FE event payloads + the event-name constants.
//!
//! Everything grove pushes to the front end is scoped to the grove window
//! ([`emit`]). Cadence/throttling is the caller's concern (the audio thread):
//! `transport`/`meters` go out on a fixed tick, `active_haps` only on change,
//! `diagnostics` only after an eval. Keeping the payloads here (not inline) means
//! the IPC shape lives in one place — the seam we freeze in Onda 4.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::grove_window::GROVE_WINDOW_LABEL;

/// Diagnostics (errors with span) after a `grove_eval`. Empty `errors` = success.
pub const EVT_DIAGNOSTICS: &str = "grove:diagnostics";
/// Source spans currently sounding, for the live editor highlight (on change).
pub const EVT_ACTIVE_HAPS: &str = "grove:active_haps";
/// Output level meter `[l, r]` (linear peak), ~tick rate.
pub const EVT_METERS: &str = "grove:meters";
/// Transport state (playing + cycle position), ~tick rate.
pub const EVT_TRANSPORT: &str = "grove:transport";
/// A log line from the running script (`debug`/`info`/… or per-hap `.log`).
pub const EVT_LOG: &str = "grove:log";

/// One located diagnostic. `start`/`end` are byte offsets into the source
/// (`None` when the failure has no known location).
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

/// The `grove:transport` payload.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TransportState {
    /// Whether the scheduler is running.
    pub playing: bool,
    /// Fractional cycle position at the current playhead.
    pub cycle: f64,
    /// Absolute output frame at the current playhead.
    pub frame: u64,
}

/// The `grove:meters` payload (linear peak `0.0..~1.0`).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Meters {
    pub l: f32,
    pub r: f32,
}

/// The `grove:active_haps` payload: byte ranges currently sounding.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActiveHaps {
    pub spans: Vec<[u32; 2]>,
}

/// Emit a grove event to the grove window only. Best-effort: a missing window
/// (closed) is a silent no-op.
pub fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    let _ = app.emit_to(GROVE_WINDOW_LABEL, event, payload);
}
