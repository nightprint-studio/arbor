//! Evaluation configuration — the knobs the shell sets from Arbor's config.

use crate::inject::LogLevel;

/// Settings that influence evaluation but aren't part of the source.
#[derive(Clone, Copy, Debug)]
pub struct EvalConfig {
    /// Octave a bare note name (`c`) or scale degree `0` lands in. Default `4`
    /// (middle C), configurable globally/per-file (`design/grove/mini-notation.md`).
    pub default_octave: i32,
    /// Only log messages at or above this level are emitted (gating).
    pub log_threshold: LogLevel,
}

impl Default for EvalConfig {
    fn default() -> Self {
        EvalConfig {
            default_octave: 4,
            log_threshold: LogLevel::Info,
        }
    }
}
