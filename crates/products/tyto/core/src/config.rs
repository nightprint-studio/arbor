//! `config` — the typed **product** tyto configuration
//! (`arbor/profiles/<active>/tyto/config.toml`, per-profile) owned
//! **out-of-process** by `tyto-be`.
//!
//! Holds the recorder's persisted defaults (source/fps/audio, codec/bitrate/
//! quality, output container/dir). The **launcher-owned** Tyto settings (the
//! opt-in OS-global shortcut + accelerator) deliberately stay in the shell config
//! (`AppConfig::tyto` → profile.toml) — window/OS-integration policy the launcher
//! reads even when tyto-be isn't running.
//!
//! Like `sitta-core`'s config, the path is **not** pushed by the shell: tyto-be
//! resolves [`tyto_config_path`](arbor_core::prelude::tyto_config_path) itself,
//! since `init_active_profile()` ran in `main` before any handler is served.
//!
//! [`load`] is infallible-by-design: a missing / unparseable file yields
//! [`TytoConfig::default`] so operational reads never break. The
//! `get/set_tyto_config` handlers stay in tyto-be and call back into [`load`] /
//! [`save`] here.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Persisted tyto settings (product, per-profile `…/tyto/config.toml`).
///
/// Field order matters for TOML serialization: every scalar field is declared
/// before the nested-table fields (`capture` / `encoding` / `output`), or `toml`
/// fails with "values must be emitted before tables".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TytoConfig {
    /// Default capture mode a fresh session opens in: `record` | `screenshot`.
    pub default_mode: String,
    /// Default target kind: `monitor` | `window` | `region`.
    pub default_target: String,
    /// Capture (source/fps/audio) defaults.
    pub capture: TytoCaptureConfig,
    /// Encoding (codec/bitrate/quality) defaults.
    pub encoding: TytoEncodingConfig,
    /// Output (container/dir/filename template) defaults.
    pub output: TytoOutputConfig,
}

impl Default for TytoConfig {
    fn default() -> Self {
        Self {
            default_mode:   "record".to_string(),
            default_target: "monitor".to_string(),
            capture:  TytoCaptureConfig::default(),
            encoding: TytoEncodingConfig::default(),
            output:   TytoOutputConfig::default(),
        }
    }
}

/// Capture-source defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TytoCaptureConfig {
    /// Frames per second for video capture (`30` | `60`).
    pub fps: u32,
    /// Capture system audio by default.
    pub system_audio: bool,
    /// Preferred microphone input id (empty = none / OS default off).
    pub mic_id: String,
    /// Seconds of on-screen 3-2-1 countdown before a video recording actually
    /// starts (`0` = off). Screenshots are never delayed. Purely a launch-side
    /// affordance — the engine never sees it (the FE runs the overlay, then calls
    /// `start_recording`).
    pub countdown_secs: u32,
}

impl Default for TytoCaptureConfig {
    fn default() -> Self {
        Self { fps: 60, system_audio: true, mic_id: String::new(), countdown_secs: 3 }
    }
}

/// Encoding defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TytoEncodingConfig {
    /// Quality preset: `high` | `balanced` | `compact`.
    pub quality: String,
    /// Video bitrate in kbps (derived from `quality` in the UI; persisted so a
    /// custom override survives).
    pub bitrate_kbps: u32,
    /// Container/codec family: e.g. `mp4` (h264) | `webm` (vp9).
    pub codec: String,
}

impl Default for TytoEncodingConfig {
    fn default() -> Self {
        Self { quality: "balanced".to_string(), bitrate_kbps: 12_000, codec: "mp4".to_string() }
    }
}

/// Output defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TytoOutputConfig {
    /// Directory captures are written to (empty = the OS Videos/Tyto dir).
    pub dir: String,
    /// Filename template. Default `tyto_%Y%m%d_%H%M%S`.
    pub filename_template: String,
    /// Screenshot image format: `png` | `jpg` | `webp` (default `png`). Only the
    /// still-image captures honour this; recordings use the encoding container.
    #[serde(default = "default_screenshot_format")]
    pub screenshot_format: String,
    /// Copy a screenshot to the OS clipboard right after it's saved (default `true`).
    /// Screenshots only — recordings are never copied. The copy happens in `tyto-be`
    /// (where the pixels are), not the Tauri shell.
    #[serde(default = "default_copy_to_clipboard")]
    pub copy_screenshot_to_clipboard: bool,
}

/// Serde default for [`TytoOutputConfig::screenshot_format`].
fn default_screenshot_format() -> String {
    "png".to_string()
}

/// Serde default for [`TytoOutputConfig::copy_screenshot_to_clipboard`].
fn default_copy_to_clipboard() -> bool {
    true
}

impl Default for TytoOutputConfig {
    fn default() -> Self {
        Self {
            dir: String::new(),
            filename_template: "tyto_%Y%m%d_%H%M%S".to_string(),
            screenshot_format: default_screenshot_format(),
            copy_screenshot_to_clipboard: default_copy_to_clipboard(),
        }
    }
}

// ── Persistence ────────────────────────────────────────────────────────────────

/// tyto's own config file: `arbor/profiles/<active>/tyto/config.toml`. Resolved
/// directly (not pushed by the shell) — `init_active_profile()` ran in `main`.
pub fn config_path() -> PathBuf {
    arbor_core::prelude::tyto_config_path("config.toml")
}

/// Read the tyto config. A missing / unparseable file yields defaults, never an
/// error — recorder settings are non-critical and self-heal to defaults.
pub fn load() -> TytoConfig {
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        if let Ok(cfg) = toml::from_str::<TytoConfig>(&text) {
            return cfg;
        }
    }
    TytoConfig::default()
}

/// Persist the tyto config to its own file (pretty TOML), creating the dir if needed.
pub fn save(cfg: &TytoConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}
