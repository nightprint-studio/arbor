//! Typed voice settings (`arbor/profiles/<active>/voice/config.toml`), owned by `voice-be`.
//! Models are heavy and machine-wide, so they live under `arbor/data/voice/models`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::wake::DEFAULT_PHRASES;

/// `<profile>/voice/config.toml`.
pub fn voice_config_path() -> PathBuf {
    arbor_core::prelude::product_dir("voice").join("config.toml")
}

/// `arbor/data/voice/models` — shared across profiles, not duplicated.
pub fn voice_models_dir() -> PathBuf {
    arbor_core::prelude::arbor_global_data_dir().join("voice").join("models")
}

/// Always-listening wake-word settings. **Off by default**: arming keeps the microphone open
/// continuously and costs CPU.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WakeConfig {
    pub enabled: bool,
    /// Phrases that wake the assistant (matched fuzzily at the start of an utterance).
    pub phrases: Vec<String>,
    /// Keep listening while the app is in the background (default: only while focused).
    pub in_background: bool,
    /// Disarm automatically after this many minutes without a wake (0 = never).
    pub auto_disarm_minutes: u32,
    /// Silence that ends a wake-initiated request, in milliseconds.
    pub silence_ms: u32,
    /// Model id used to check for the phrase (empty = the smallest installed).
    pub detector_model: String,
}

impl Default for WakeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            phrases: DEFAULT_PHRASES.iter().map(|s| s.to_string()).collect(),
            in_background: false,
            auto_disarm_minutes: 30,
            silence_ms: 1200,
            detector_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VoiceConfig {
    /// Catalogue id of the active model (empty = none chosen yet).
    pub model: String,
    /// A user-supplied ggml file; wins over `model` when set.
    pub custom_model_path: String,
    /// `auto` or an ISO 639-1 code (`it`, `en`, …).
    pub language: String,
    /// cpal device name; empty = system default.
    pub microphone: String,
    /// Show text while the user is still speaking (a pause finishes a piece, which is transcribed
    /// in the background). Falls back to "on release" by itself when the model is too slow.
    pub live_text: bool,
    /// Feed project vocabulary to the recogniser as `initial_prompt`.
    pub vocabulary_hints: bool,
    /// Minutes of inactivity before `voice-be` frees the model and exits.
    pub idle_minutes: u32,
    /// Hard cap on one recording, in seconds.
    pub max_seconds: u32,
    /// Open the large voice overlay for push-to-talk / wake requests.
    pub overlay: bool,
    /// Declared after the scalars: TOML needs values before tables.
    pub wake: WakeConfig,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            custom_model_path: String::new(),
            language: "auto".into(),
            microphone: String::new(),
            live_text: true,
            vocabulary_hints: true,
            idle_minutes: 5,
            max_seconds: 120,
            overlay: true,
            wake: WakeConfig::default(),
        }
    }
}

impl VoiceConfig {
    /// Missing / unparseable file → defaults, so reads never break.
    pub fn load() -> Self {
        std::fs::read_to_string(voice_config_path())
            .ok()
            .and_then(|s| toml::from_str::<Self>(&s).ok())
            .unwrap_or_default()
            .sanitized()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = voice_config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
    }

    /// This config with `patch` (a JSON object holding only the fields to change) applied; nested
    /// objects (`wake`) merge field by field. Callers send patches rather than whole configs, so a
    /// page holding a stale copy can never write an old `model` back over a newer one.
    pub fn patched(&self, patch: &serde_json::Value) -> Result<Self, String> {
        fn merge(into: &mut serde_json::Value, patch: &serde_json::Value) {
            match (into, patch) {
                (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                    for (k, v) in b {
                        merge(a.entry(k.clone()).or_insert(serde_json::Value::Null), v);
                    }
                }
                (slot, v) => *slot = v.clone(),
            }
        }
        let mut value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        merge(&mut value, patch);
        serde_json::from_value::<Self>(value).map(Self::sanitized).map_err(|e| format!("invalid voice settings: {e}"))
    }

    /// Clamp values a hand-edited file might push out of range.
    pub fn sanitized(mut self) -> Self {
        self.max_seconds = self.max_seconds.clamp(5, 600);
        self.idle_minutes = self.idle_minutes.clamp(1, 240);
        self.wake.silence_ms = self.wake.silence_ms.clamp(400, 5000);
        if self.language.trim().is_empty() {
            self.language = "auto".into();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_is_off_by_default() {
        assert!(!VoiceConfig::default().wake.enabled);
        assert!(!VoiceConfig::default().wake.in_background);
    }

    #[test]
    fn toml_round_trip_and_partial_files() {
        let cfg = VoiceConfig { language: "it".into(), ..Default::default() };
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert_eq!(toml::from_str::<VoiceConfig>(&text).unwrap(), cfg);
        let partial: VoiceConfig = toml::from_str("language = \"en\"").unwrap();
        assert_eq!(partial.max_seconds, 120);
    }

    #[test]
    fn a_patch_changes_only_what_it_names() {
        let base = VoiceConfig { model: "small".into(), language: "it".into(), ..Default::default() };
        let out = base.patched(&serde_json::json!({ "language": "en", "wake": { "enabled": true } })).unwrap();
        assert_eq!(out.model, "small");
        assert_eq!(out.language, "en");
        assert!(out.wake.enabled);
        assert_eq!(out.wake.phrases, base.wake.phrases);
        assert!(base.patched(&serde_json::json!({ "max_seconds": "x" })).is_err());
    }

    #[test]
    fn sanitize_clamps() {
        let mut c = VoiceConfig::default();
        c.max_seconds = 99_999;
        c.idle_minutes = 0;
        let c = c.sanitized();
        assert_eq!(c.max_seconds, 600);
        assert_eq!(c.idle_minutes, 1);
    }
}
