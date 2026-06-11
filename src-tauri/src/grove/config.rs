//! Typed grove configuration — the `[grove]` section of Arbor's global
//! `config.toml` (never `localStorage`; hard rule #11).
//!
//! Holds the knobs the engine reads at session start (octave, tempo, log gating)
//! and offline-render defaults, plus an optional override for where the VSCO 2
//! sample bank is stored. Converted into the grove crates' own config types
//! ([`EvalConfig`], [`RenderConfig`]) at the call sites.

use serde::{Deserialize, Serialize};

use arbor_grove::prelude::{BitDepth, EvalConfig, LogLevel, RenderConfig};

/// Persisted grove settings (global, `~/.config/arbor/config.toml` → `[grove]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GroveConfig {
    /// Octave a bare note / degree `0` lands in. Default `4` (middle C).
    pub default_octave: i32,
    /// Starting tempo in cycles-per-second when a script omits `cps(...)`.
    pub default_cps: f64,
    /// Lowest log level surfaced to the console (`trace`|`debug`|`info`|`warn`|`error`).
    pub log_threshold: String,
    /// Offline-render defaults.
    pub render: GroveRenderConfig,
    /// Override for the VSCO 2 install directory. `None` → the default under the
    /// OS data dir (`<data>/arbor/grove/vsco`).
    pub vsco_dir: Option<String>,
}

impl Default for GroveConfig {
    fn default() -> Self {
        GroveConfig {
            default_octave: 4,
            default_cps: 0.5,
            log_threshold: "info".to_string(),
            render: GroveRenderConfig::default(),
            vsco_dir: None,
        }
    }
}

/// Offline-render defaults (mirrors the engine's `RenderConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GroveRenderConfig {
    /// Output sample rate (frames/s).
    pub sample_rate: u32,
    /// `"int24"` (default) or `"float32"`.
    pub bit_depth: String,
    /// Trailing tail (release/reverb) captured after the arrangement, in seconds.
    pub tail_max_secs: f32,
}

impl Default for GroveRenderConfig {
    fn default() -> Self {
        GroveRenderConfig {
            sample_rate: 48_000,
            bit_depth: "int24".to_string(),
            tail_max_secs: 4.0,
        }
    }
}

impl GroveConfig {
    /// The evaluator config derived from these settings.
    pub fn eval_config(&self) -> EvalConfig {
        EvalConfig {
            default_octave: self.default_octave,
            log_threshold: parse_level(&self.log_threshold),
        }
    }

    /// The log gate threshold as a [`LogLevel`].
    pub fn log_level(&self) -> LogLevel {
        parse_level(&self.log_threshold)
    }
}

impl GroveRenderConfig {
    /// The engine render config derived from these settings.
    pub fn render_config(&self) -> RenderConfig {
        RenderConfig {
            sample_rate: self.sample_rate,
            bit_depth: if self.bit_depth.eq_ignore_ascii_case("float32") {
                BitDepth::Float32
            } else {
                BitDepth::Int24
            },
            tail_max_secs: self.tail_max_secs,
        }
    }
}

/// Parse a log-level keyword, defaulting to `info` on anything unrecognised.
fn parse_level(s: &str) -> LogLevel {
    LogLevel::parse(&s.to_ascii_lowercase()).unwrap_or(LogLevel::Info)
}
