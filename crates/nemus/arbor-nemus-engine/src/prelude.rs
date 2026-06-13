//! Canonical entry point for `arbor-nemus-engine`'s public API.
//!
//! Workspace convention: reach the public surface through `prelude` rather than
//! per-module paths. The submodules stay `pub` for rustdoc navigation only.

// ── Error ────────────────────────────────────────────────────────────────────
pub use crate::error::{EngineError, Result};

// ── Clock ────────────────────────────────────────────────────────────────────
pub use crate::clock::Epoch;

// ── Scheduling core (pure) ───────────────────────────────────────────────────
pub use crate::schedule::{delay_config_for, schedule_span, voice_event_from_hap};

// ── Transport (real-time) ────────────────────────────────────────────────────
pub use crate::transport::{Transport, LOOKAHEAD_MS};

// ── Offline render ───────────────────────────────────────────────────────────
pub use crate::encode::{Format, RenderSink};
pub use crate::render::{
    render_offline, render_offline_with_progress, BitDepth, RenderConfig, RenderProgress,
    DEFAULT_BIT_DEPTH, DEFAULT_TAIL_MAX_SECS,
};
