//! Measured speed of each model on this machine, kept next to the models (they are machine-wide).
//! A measurement is a real-time factor (`took / audio`) from a real transcription; the newest runs
//! weigh most, so a PC that gets busier or faster is followed. The recommendation reads it and a
//! measurement always beats the hardware estimate.

use std::path::PathBuf;

use crate::config::voice_models_dir;
use crate::models::{find_model, Measured};

/// `arbor/data/voice/speed.json`, beside the `models` folder.
pub fn bench_path() -> PathBuf {
    voice_models_dir().parent().map(|p| p.join("speed.json")).unwrap_or_else(|| PathBuf::from("speed.json"))
}

/// Missing / unreadable file -> nothing measured yet.
pub fn load_measured() -> Measured {
    std::fs::read_to_string(bench_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Blend a new run into the stored value: 60 % new, 40 % old. Unknown models, absurd values and
/// clips shorter than 1.5 s (dominated by fixed costs) are ignored.
pub fn blend(measured: &mut Measured, id: &str, audio_secs: f32, rtf: f32) -> bool {
    if find_model(id).is_none() || audio_secs < 1.5 || !rtf.is_finite() || rtf <= 0.0 || rtf > 50.0 {
        return false;
    }
    let next = match measured.get(id) {
        Some(old) => rtf * 0.6 + old * 0.4,
        None => rtf,
    };
    measured.insert(id.to_string(), next);
    true
}

/// Record a run and persist. Best effort: a read-only disk just means no learning.
pub fn record_measurement(id: &str, audio_secs: f32, rtf: f32) -> Measured {
    let mut measured = load_measured();
    if blend(&mut measured, id, audio_secs, rtf) {
        let path = bench_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(&measured) {
            let _ = std::fs::write(&path, text);
        }
    }
    measured
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blending_favours_the_newest_run() {
        let mut m = Measured::new();
        assert!(blend(&mut m, "small", 5.0, 1.0));
        assert!(blend(&mut m, "small", 5.0, 0.5));
        assert!((m["small"] - 0.7).abs() < 1e-4);
    }

    #[test]
    fn junk_is_ignored() {
        let mut m = Measured::new();
        assert!(!blend(&mut m, "nope", 5.0, 0.5));
        assert!(!blend(&mut m, "small", 0.5, 0.5));
        assert!(!blend(&mut m, "small", 5.0, f32::NAN));
        assert!(!blend(&mut m, "small", 5.0, 0.0));
        assert!(m.is_empty());
    }
}
