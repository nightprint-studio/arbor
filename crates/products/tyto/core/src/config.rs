//! `config` — the typed **product** tyto configuration
//! (`arbor/profiles/<active>/tyto/config.toml`, per-profile) owned
//! **out-of-process** by `tyto-be`.
//!
//! Holds the recorder's persisted defaults (source/fps/audio, quality/bitrate,
//! output container/dir). The **launcher-owned** Tyto settings (the
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
/// before the nested-table fields (`capture` / `encoding` / `output` / `frames`), or `toml`
/// fails with "values must be emitted before tables".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TytoConfig {
    /// Default capture mode a fresh session opens in: `record` | `screenshot`.
    pub default_mode: String,
    /// Default target kind: `monitor` | `window` | `region`.
    pub default_target: String,
    /// What a recording produces: `video` (H.264 mp4) | `frames` (a deduplicated,
    /// timestamped image sequence). The capture pipeline is the same either way —
    /// only the sink differs.
    #[serde(default = "default_record_output")]
    pub record_output: String,
    /// Capture (source/fps/audio) defaults.
    pub capture: TytoCaptureConfig,
    /// Encoding (quality preset + the bitrate it implies) defaults.
    pub encoding: TytoEncodingConfig,
    /// Output (container/dir/filename template) defaults.
    pub output: TytoOutputConfig,
    /// Frame-sequence recording defaults (`record_output = "frames"`).
    #[serde(default)]
    pub frames: TytoFramesConfig,
}

/// Serde default for [`TytoConfig::record_output`].
fn default_record_output() -> String {
    "video".to_string()
}

impl Default for TytoConfig {
    fn default() -> Self {
        Self {
            default_mode:   "record".to_string(),
            default_target: "monitor".to_string(),
            record_output:  default_record_output(),
            capture:  TytoCaptureConfig::default(),
            encoding: TytoEncodingConfig::default(),
            output:   TytoOutputConfig::default(),
            frames:   TytoFramesConfig::default(),
        }
    }
}

impl TytoConfig {
    /// Recompute every derived field, so the value in memory, the value on disk and
    /// the value the encoder uses are the same number. Called on load and before
    /// save — the two moments a config can arrive from somewhere that didn't derive
    /// it (a hand-edited file, an older frontend).
    pub fn normalize(&mut self) {
        self.encoding.bitrate_kbps = preset_bitrate_kbps(&self.encoding.quality);
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
///
/// There is no container/codec field: the video sink is libx264 into mp4 and
/// nothing selects anything else. A setting that names alternatives the engine
/// cannot produce is a promise the code doesn't keep — when a second container
/// really exists, the field comes back with something behind it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TytoEncodingConfig {
    /// Quality preset: `high` | `balanced` | `compact`.
    pub quality: String,
    /// Video bitrate in kbps. **Derived** from `quality` via
    /// [`preset_bitrate_kbps`] and rewritten on every load and save, so the file
    /// is self-describing and the frontend has a number to show without keeping a
    /// copy of the table. Editing it by hand has no effect — change `quality`.
    pub bitrate_kbps: u32,
}

impl Default for TytoEncodingConfig {
    fn default() -> Self {
        Self { quality: "balanced".to_string(), bitrate_kbps: preset_bitrate_kbps("balanced") }
    }
}

/// Video bitrate in kbps for a quality preset — **the** table.
///
/// It lives in `tyto-core` rather than in the encoder or in the UI because both
/// need it and neither owns it: `tyto-be` encodes from it, and the frontend only
/// ever displays the number this produced (it reads [`TytoEncodingConfig::bitrate_kbps`],
/// which is this function's output). Two copies of a table like this drift silently —
/// the UI promises one bitrate and the file gets another, and nothing fails.
///
/// An unrecognised preset reads as `balanced`: a typo in a hand-edited config should
/// record something reasonable, not nothing.
pub fn preset_bitrate_kbps(quality: &str) -> u32 {
    match quality {
        "high" => 24_000,
        "compact" => 6_000,
        _ => 12_000, // balanced
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

/// Frame-sequence defaults — the image-sequence sink of a recording.
///
/// Deliberately NOT folded into [`TytoEncodingConfig`]: that one describes a video
/// container/bitrate, and none of it means anything for a sequence of stills. The
/// sampling rate lives here too rather than reusing `capture.fps`, because the two
/// answer different questions — a 60 fps video is routine, 60 PNG encodes a second
/// of a 4K desktop is not.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TytoFramesConfig {
    /// Image format of each frame: `png` | `jpg` | `webp`.
    pub format: String,
    /// Upper bound on how often the screen is sampled, in frames per second. The
    /// *real* rate is whatever the screen actually changes at: identical frames are
    /// never written (a still screen costs one frame, not `sample_fps` per second).
    pub sample_fps: u32,
    /// Downscale every frame so its width is at most this many pixels
    /// (`0` = keep the captured resolution).
    pub max_width: u32,
}

impl Default for TytoFramesConfig {
    fn default() -> Self {
        Self { format: "png".to_string(), sample_fps: 12, max_width: 0 }
    }
}

// ── Persistence ────────────────────────────────────────────────────────────────

/// tyto's own config file: `arbor/profiles/<active>/tyto/config.toml`. Resolved
/// directly (not pushed by the shell) — `init_active_profile()` ran in `main`.
pub fn config_path() -> PathBuf {
    arbor_core::prelude::tyto_config_path("config.toml")
}

/// Read the tyto config. A missing / unparseable file yields defaults, never an
/// error — recorder settings are non-critical and self-heal to defaults. The result
/// is always [`normalized`](TytoConfig::normalize), so every reader sees the same
/// derived values the encoder will use.
pub fn load() -> TytoConfig {
    let mut cfg = std::fs::read_to_string(config_path())
        .ok()
        .and_then(|text| toml::from_str::<TytoConfig>(&text).ok())
        .unwrap_or_default();
    cfg.normalize();
    cfg
}

/// Persist the tyto config to its own file (pretty TOML), creating the dir if
/// needed. Returns the **normalized** config that was actually written, so a caller
/// (the `set_tyto_config` handler, and through it the frontend) learns the derived
/// values instead of having to recompute them.
pub fn save(cfg: &TytoConfig) -> Result<TytoConfig, String> {
    let mut cfg = cfg.clone();
    cfg.normalize();
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_has_its_own_bitrate() {
        assert_eq!(preset_bitrate_kbps("high"), 24_000);
        assert_eq!(preset_bitrate_kbps("balanced"), 12_000);
        assert_eq!(preset_bitrate_kbps("compact"), 6_000);
    }

    #[test]
    fn an_unknown_preset_reads_as_balanced() {
        assert_eq!(preset_bitrate_kbps("ludicrous"), 12_000, "a typo records something usable");
        assert_eq!(preset_bitrate_kbps(""), 12_000);
    }

    #[test]
    fn normalize_makes_the_bitrate_follow_the_preset() {
        let mut cfg = TytoConfig::default();
        // What a hand-edited file (or an older frontend) can hand us: a preset and a
        // bitrate that disagree.
        cfg.encoding.quality = "high".to_string();
        cfg.encoding.bitrate_kbps = 1;
        cfg.normalize();
        assert_eq!(cfg.encoding.bitrate_kbps, 24_000, "the preset wins, always");
    }

    #[test]
    fn the_default_config_is_already_normalized() {
        let mut cfg = TytoConfig::default();
        let before = cfg.encoding.bitrate_kbps;
        cfg.normalize();
        assert_eq!(cfg.encoding.bitrate_kbps, before);
    }

    #[test]
    fn a_config_without_the_dropped_codec_key_still_parses() {
        // Old files carry `codec = "mp4"`; serde ignores keys the struct no longer
        // has, so removing the field can't break an existing profile.
        let text = r#"
            default_mode = "record"
            default_target = "monitor"
            [encoding]
            quality = "compact"
            codec = "webm"
        "#;
        let mut cfg: TytoConfig = toml::from_str(text).expect("stale keys are ignored");
        cfg.normalize();
        assert_eq!(cfg.encoding.quality, "compact");
        assert_eq!(cfg.encoding.bitrate_kbps, 6_000);
    }
}
