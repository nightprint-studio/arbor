//! Typed nemus configuration — nemus's own `%APPDATA%\nemus\config.toml`
//! (separate from Arbor's `config.toml`; never `localStorage`, hard rule #11).
//!
//! Holds the knobs the engine reads at session start (octave, tempo, log gating)
//! and offline-render defaults, plus optional overrides for where the sample
//! banks are stored. Converted into the nemus crates' own config types
//! ([`EvalConfig`], [`RenderConfig`]) at the call sites.
//!
//! Persistence lives here too ([`load`]/[`save`]): nemus reads/writes its own
//! file under [`nemus_config_dir`](arbor_core::prelude::nemus_config_dir), and a
//! one-time [`migrate_if_needed`] seeds it from Arbor's legacy `[nemus]` section
//! the first time after the split.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use arbor_nemus::prelude::{
    BitDepth, EvalConfig, LogLevel, RenderConfig, DEFAULT_BIT_DEPTH, DEFAULT_SAMPLE_RATE,
    DEFAULT_TAIL_MAX_SECS,
};

/// Persisted nemus settings (global, `%APPDATA%\nemus\config.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NemusConfig {
    /// Octave a bare note / degree `0` lands in. Default `4` (middle C).
    pub default_octave: i32,
    /// Starting tempo in cycles-per-second when a script omits `cps(...)`.
    pub default_cps: f64,
    /// Lowest log level surfaced to the console (`trace`|`debug`|`info`|`warn`|`error`).
    pub log_threshold: String,
    /// Override for the VSCO 2 install directory. `None` → the default under the
    /// nemus data dir (`<nemus-data>/vsco`).
    pub vsco_dir: Option<String>,
    /// Override for the directory holding downloadable sample packs (Dirt-Samples,
    /// drum machines, GM). `None` → the default (`<nemus-data>/packs`).
    pub packs_dir: Option<String>,
    /// Override for the directory holding downloadable ONNX transcription models.
    /// `None` → the default (`<nemus-data>/models`).
    pub models_dir: Option<String>,
    /// Override for the basic-pitch ONNX download URL. `None` → the built-in
    /// default (see `nemus::models`). Set this if the default artifact moves.
    pub basic_pitch_url: Option<String>,
    /// Override for the Demucs ONNX download URL. `None` → the built-in default.
    pub demucs_url: Option<String>,
    /// Chosen audio output device, by cpal device name. `None` → the host default
    /// (also the fallback if the named device is no longer present).
    pub output_device: Option<String>,
    /// Offline-render defaults. Declared last: as a nested TOML table it must
    /// follow every scalar field, or `toml` serialization fails once an override
    /// above is set ("values must be emitted before tables").
    pub render: NemusRenderConfig,
}

impl Default for NemusConfig {
    fn default() -> Self {
        NemusConfig {
            default_octave: 4,
            default_cps: 0.5,
            log_threshold: "info".to_string(),
            vsco_dir: None,
            packs_dir: None,
            models_dir: None,
            basic_pitch_url: None,
            demucs_url: None,
            output_device: None,
            render: NemusRenderConfig::default(),
        }
    }
}

/// Offline-render defaults (mirrors the engine's `RenderConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NemusRenderConfig {
    /// Output sample rate (frames/s).
    pub sample_rate: u32,
    /// `"int24"` (default) or `"float32"`.
    pub bit_depth: String,
    /// Trailing tail (release/reverb) captured after the arrangement, in seconds.
    pub tail_max_secs: f32,
}

impl Default for NemusRenderConfig {
    fn default() -> Self {
        NemusRenderConfig {
            sample_rate: DEFAULT_SAMPLE_RATE,
            bit_depth: bit_depth_str(DEFAULT_BIT_DEPTH).to_string(),
            tail_max_secs: DEFAULT_TAIL_MAX_SECS,
        }
    }
}

/// The config-string spelling of a [`BitDepth`] (the inverse of the parse in
/// [`NemusRenderConfig::render_config`]), so the default lives in one place.
fn bit_depth_str(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::Int24 => "int24",
        BitDepth::Float32 => "float32",
    }
}

impl NemusConfig {
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

impl NemusRenderConfig {
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

// ── Persistence ────────────────────────────────────────────────────────────────

/// nemus's own config file: `%APPDATA%\nemus\config.toml`.
pub fn config_path() -> PathBuf {
    arbor_core::prelude::nemus_config_path("config.toml")
}

/// Read the nemus config. A missing / unparseable file yields defaults (after a
/// one-time migration attempt from Arbor's legacy `[nemus]` section), never an
/// error — nemus config is non-critical and self-heals to defaults.
pub fn load() -> NemusConfig {
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        if let Ok(cfg) = toml::from_str::<NemusConfig>(&text) {
            return cfg;
        }
    }
    // First read after the split (or a corrupt file): try to inherit the old
    // settings, then fall back to defaults.
    migrate_if_needed().unwrap_or_default()
}

/// Persist the nemus config to its own file, creating the dir if needed.
pub fn save(cfg: &NemusConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// One-time migration: if nemus has no config file yet but Arbor's global
/// `config.toml` still carries a `[nemus]` section (the pre-split location),
/// lift it into nemus's own file and return it. Returns `None` when there's
/// nothing to migrate (already split, or never configured). Idempotent: once
/// nemus's file exists, [`load`] never reaches here.
pub fn migrate_if_needed() -> Option<NemusConfig> {
    if config_path().exists() {
        return None;
    }
    /// Just enough of Arbor's config to pluck the legacy `[nemus]` table.
    #[derive(Deserialize)]
    struct LegacyArborConfig {
        nemus: Option<NemusConfig>,
    }
    let arbor_cfg = arbor_core::prelude::arbor_config_path("config.toml");
    let text = std::fs::read_to_string(arbor_cfg).ok()?;
    let legacy: LegacyArborConfig = toml::from_str(&text).ok()?;
    let nemus = legacy.nemus?;
    // Best-effort: a write failure just means we'll retry the migration next
    // launch (the legacy section stays put until Arbor rewrites its config).
    let _ = save(&nemus);
    Some(nemus)
}
