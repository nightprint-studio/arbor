//! Typed merula configuration — merula's own `%APPDATA%\merula\config.toml`
//! (separate from Arbor's `config.toml`; never `localStorage`, hard rule #11).
//!
//! Holds the knobs the engine reads at session start (octave, tempo, log gating)
//! and offline-render defaults, plus optional overrides for where the sample
//! banks are stored. Converted into the merula crates' own config types
//! ([`EvalConfig`], [`RenderConfig`]) at the call sites.
//!
//! Persistence lives here too ([`load`]/[`save`]): merula reads/writes its own
//! file under [`merula_config_dir`](arbor_core::prelude::merula_config_dir).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use merula::prelude::{
    BitDepth, EvalConfig, Format, LogLevel, RenderConfig, DEFAULT_BIT_DEPTH, DEFAULT_SAMPLE_RATE,
    DEFAULT_TAIL_MAX_SECS,
};

/// Persisted merula settings (global, `%APPDATA%\merula\config.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MerulaConfig {
    /// Octave a bare note / degree `0` lands in. Default `4` (middle C).
    pub default_octave: i32,
    /// Starting tempo in cycles-per-second when a script omits `cps(...)`.
    pub default_cps: f64,
    /// Lowest log level surfaced to the console (`trace`|`debug`|`info`|`warn`|`error`).
    pub log_threshold: String,
    /// Override for the VSCO 2 install directory. `None` → the default under the
    /// merula data dir (`<merula-data>/vsco`).
    pub vsco_dir: Option<String>,
    /// Override for the directory holding downloadable sample packs (Dirt-Samples,
    /// drum machines, GM). `None` → the default (`<merula-data>/packs`).
    pub packs_dir: Option<String>,
    /// Override for the directory holding downloadable ONNX transcription models.
    /// `None` → the default (`<merula-data>/models`).
    pub models_dir: Option<String>,
    /// Override for the basic-pitch ONNX download URL. `None` → the built-in
    /// default (see `merula::models`). Set this if the default artifact moves.
    pub basic_pitch_url: Option<String>,
    /// Override for the Demucs ONNX download URL. `None` → the built-in default.
    pub demucs_url: Option<String>,
    /// Chosen audio output device, by cpal device name. `None` → the host default
    /// (also the fallback if the named device is no longer present).
    pub output_device: Option<String>,
    /// How far the transport "step back / step forward" buttons move the playhead,
    /// in cycles (bars). Default `1.0`.
    pub skip_step_cycles: f64,
    /// Offline-render defaults. Declared last: as a nested TOML table it must
    /// follow every scalar field, or `toml` serialization fails once an override
    /// above is set ("values must be emitted before tables").
    pub render: MerulaRenderConfig,
}

impl Default for MerulaConfig {
    fn default() -> Self {
        MerulaConfig {
            default_octave: 4,
            default_cps: 0.5,
            log_threshold: "info".to_string(),
            vsco_dir: None,
            packs_dir: None,
            models_dir: None,
            basic_pitch_url: None,
            demucs_url: None,
            output_device: None,
            skip_step_cycles: 1.0,
            render: MerulaRenderConfig::default(),
        }
    }
}

/// Offline-render defaults (mirrors the engine's `RenderConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MerulaRenderConfig {
    /// Output sample rate (frames/s).
    pub sample_rate: u32,
    /// `"int24"` (default) or `"float32"`.
    pub bit_depth: String,
    /// Trailing tail (release/reverb) captured after the arrangement, in seconds.
    pub tail_max_secs: f32,
    /// Default output container/codec: `"wav"` (default) or `"ogg"`. Remembered
    /// across sessions as the last format the user exported with; the export
    /// dialog seeds its Format picker from it.
    pub format: String,
}

impl Default for MerulaRenderConfig {
    fn default() -> Self {
        MerulaRenderConfig {
            sample_rate: DEFAULT_SAMPLE_RATE,
            bit_depth: bit_depth_str(DEFAULT_BIT_DEPTH).to_string(),
            tail_max_secs: DEFAULT_TAIL_MAX_SECS,
            format: "wav".to_string(),
        }
    }
}

/// The config-string spelling of a [`BitDepth`] (the inverse of the parse in
/// [`MerulaRenderConfig::render_config`]), so the default lives in one place.
fn bit_depth_str(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::Int24 => "int24",
        BitDepth::Float32 => "float32",
    }
}

impl MerulaConfig {
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

impl MerulaRenderConfig {
    /// The engine render config derived from these settings. `format` is the
    /// saved default; a per-export OGG/WAV choice is overlaid in
    /// `render::resolve_config`.
    pub fn render_config(&self) -> RenderConfig {
        RenderConfig {
            sample_rate: self.sample_rate,
            bit_depth: if self.bit_depth.eq_ignore_ascii_case("float32") {
                BitDepth::Float32
            } else {
                BitDepth::Int24
            },
            tail_max_secs: self.tail_max_secs,
            format: if self.format.eq_ignore_ascii_case("ogg") {
                Format::Ogg
            } else {
                Format::Wav
            },
            // Normalization is a per-export choice, overlaid in
            // `render::resolve_config` — not a persisted default.
            normalize: None,
        }
    }
}

/// Parse a log-level keyword, defaulting to `info` on anything unrecognised.
fn parse_level(s: &str) -> LogLevel {
    LogLevel::parse(&s.to_ascii_lowercase()).unwrap_or(LogLevel::Info)
}

// ── Persistence ────────────────────────────────────────────────────────────────

/// merula's own config file: `%APPDATA%\merula\config.toml`.
pub fn config_path() -> PathBuf {
    arbor_core::prelude::merula_config_path("config.toml")
}

/// Read the merula config. A missing / unparseable file yields defaults, never an
/// error — merula config is non-critical and self-heals to defaults.
pub fn load() -> MerulaConfig {
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        if let Ok(cfg) = toml::from_str::<MerulaConfig>(&text) {
            return cfg;
        }
    }
    MerulaConfig::default()
}

/// Persist the merula config to its own file, creating the dir if needed.
pub fn save(cfg: &MerulaConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

// ── One-shot migration from the legacy "nemus" product name ──────────────────

/// Move the legacy `nemus` config + data dirs to their `merula` locations, once.
///
/// The product was renamed from **nemus** to **merula**; its settings and the
/// (potentially multi-GB) sample banks / VSCO bank lived under `<config>/nemus`
/// and `<data>/nemus`. Rename those to the merula dirs so nothing is lost on the
/// first run after the rename. A no-op once the merula dir exists; non-destructive
/// on error (the legacy dir is left in place). The legacy path is derived from the
/// merula dir's parent, so it tracks whatever OS root `dirs` resolves.
pub fn migrate_legacy_dirs() {
    use arbor_core::prelude::{merula_config_dir, merula_data_dir};
    for current in [merula_config_dir(), merula_data_dir()] {
        let Some(legacy) = current.parent().map(|base| base.join("nemus")) else {
            continue;
        };
        if legacy.is_dir() && !current.exists() {
            if let Err(e) = std::fs::rename(&legacy, &current) {
                eprintln!("merula: legacy dir migration {legacy:?} -> {current:?} failed: {e}");
            }
        }
    }
}

