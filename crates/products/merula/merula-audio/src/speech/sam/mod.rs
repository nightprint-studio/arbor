//! SAM (Software Automatic Mouth) speech engine — a faithful Rust port of
//! `discordier/sam-js`. The pipeline is `text → reciter → parser → renderer`,
//! producing 8-bit unsigned PCM at 22050 Hz, then converted to mono `f32`.
//!
//! Vendored under a fair-use determination by the project owner; see the speech
//! module docs.

mod parse_tables;
mod parser;
mod reciter;
mod render_tables;
mod renderer;

/// SAM voice knobs. Defaults match the original (`pitch 64`, `speed 72`,
/// `mouth/throat 128`). `phonetic` skips the reciter and treats the input as a
/// raw phoneme string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SamConfig {
    pub pitch: u8,
    pub speed: u8,
    pub mouth: u8,
    pub throat: u8,
    pub singmode: bool,
    pub phonetic: bool,
}

impl Default for SamConfig {
    fn default() -> Self {
        SamConfig { pitch: 64, speed: 72, mouth: 128, throat: 128, singmode: false, phonetic: false }
    }
}

/// Synthesize `text` into mono `f32` samples (22050 Hz). Returns an empty vec if
/// the text yields no phonemes (mirrors the JS engine returning `false`).
pub fn synthesize(text: &str, cfg: &SamConfig) -> Vec<f32> {
    // Reciter: English text → phoneme string (unless the input is already phonetic).
    let phonemes = if cfg.phonetic {
        text.to_uppercase()
    } else {
        match reciter::text_to_phonemes(text) {
            Some(s) => s,
            None => return Vec::new(),
        }
    };

    let tuples = match parser::parse(&phonemes) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let bytes = renderer::render(
        &tuples,
        cfg.pitch as i32,
        cfg.mouth as i32,
        cfg.throat as i32,
        cfg.speed as i32,
        cfg.singmode,
    );

    // Uint8ArrayToFloat32Array: (byte - 128) / 256.
    bytes.iter().map(|&b| (b as f32 - 128.0) / 256.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reciter_maps_known_words() {
        // The reciter must terminate and produce phonemes for plain English.
        let p = reciter::text_to_phonemes("hello world").unwrap();
        assert!(!p.trim().is_empty());
    }

    #[test]
    fn synthesizes_non_empty_audio() {
        let audio = synthesize("hello", &SamConfig::default());
        assert!(!audio.is_empty(), "expected audio samples for 'hello'");
        // 8-bit centred at 128 → f32 in roughly [-0.5, 0.5).
        assert!(audio.iter().all(|&s| (-0.6..0.6).contains(&s)));
    }

    #[test]
    fn phonetic_input_bypasses_reciter() {
        let audio = synthesize("/HEHLOW", &SamConfig { phonetic: true, ..Default::default() });
        assert!(!audio.is_empty());
    }

    #[test]
    fn empty_text_is_silent() {
        assert!(synthesize("", &SamConfig::default()).is_empty());
    }
}
