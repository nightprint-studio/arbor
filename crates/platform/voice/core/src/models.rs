//! The Whisper (ggml) model catalogue, hardware recommendation and speed verdicts.
//!
//! Names, sizes and SHA-256 digests were taken from the Hugging Face LFS metadata of
//! `ggerganov/whisper.cpp` (the official ggml repository), so downloads are verified against a
//! digest pinned here rather than one fetched from the same host as the file.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const REPO_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

/// One downloadable model.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
    /// English-only model (smaller / better in English, useless for other languages).
    pub english_only: bool,
    /// Relative CPU cost per second of audio (tiny = 1).
    pub cost: f32,
    /// Rough accuracy, 1 (poor) – 5 (best).
    pub accuracy: u8,
    pub quantized: bool,
}

macro_rules! m {
    ($id:expr, $label:expr, $file:expr, $size:expr, $sha:expr, $en:expr, $cost:expr, $acc:expr, $q:expr) => {
        ModelSpec {
            id: $id,
            label: $label,
            file: $file,
            size_bytes: $size,
            sha256: $sha,
            english_only: $en,
            cost: $cost,
            accuracy: $acc,
            quantized: $q,
        }
    };
}

pub const CATALOGUE: &[ModelSpec] = &[
    m!("tiny", "Tiny", "ggml-tiny.bin", 77_691_713, "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21", false, 1.0, 1, false),
    m!("tiny-q5_1", "Tiny (compact)", "ggml-tiny-q5_1.bin", 32_152_673, "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7", false, 0.95, 1, true),
    m!("tiny.en", "Tiny (English)", "ggml-tiny.en.bin", 77_704_715, "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f", true, 1.0, 2, false),
    m!("base", "Base", "ggml-base.bin", 147_951_465, "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe", false, 2.4, 2, false),
    m!("base-q5_1", "Base (compact)", "ggml-base-q5_1.bin", 59_707_625, "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898", false, 2.3, 2, true),
    m!("base.en", "Base (English)", "ggml-base.en.bin", 147_964_211, "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002", true, 2.4, 3, false),
    m!("small", "Small", "ggml-small.bin", 487_601_967, "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b", false, 7.1, 3, false),
    m!("small-q5_1", "Small (compact)", "ggml-small-q5_1.bin", 190_085_487, "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb", false, 6.8, 3, true),
    m!("small.en", "Small (English)", "ggml-small.en.bin", 487_614_201, "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d", true, 7.1, 4, false),
    m!("medium-q5_0", "Medium (compact)", "ggml-medium-q5_0.bin", 539_212_467, "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f", false, 20.0, 4, true),
    m!("medium", "Medium", "ggml-medium.bin", 1_533_763_059, "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208", false, 21.0, 4, false),
    m!("large-v3-turbo-q5_0", "Large v3 Turbo (compact)", "ggml-large-v3-turbo-q5_0.bin", 574_041_195, "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2", false, 27.0, 5, true),
    m!("large-v3-turbo", "Large v3 Turbo", "ggml-large-v3-turbo.bin", 1_624_555_275, "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69", false, 28.0, 5, false),
    m!("large-v3-q5_0", "Large v3 (compact)", "ggml-large-v3-q5_0.bin", 1_081_140_203, "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1", false, 33.0, 5, true),
    m!("large-v3", "Large v3", "ggml-large-v3.bin", 3_095_033_483, "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2", false, 34.0, 5, false),
];

pub fn find_model(id: &str) -> Option<&'static ModelSpec> {
    CATALOGUE.iter().find(|m| m.id == id)
}

/// Download URL of a catalogue entry.
pub fn asset_url(spec: &ModelSpec) -> String {
    format!("{REPO_BASE}{}", spec.file)
}

/// Need the file size plus 10 % headroom free before starting a download.
pub fn free_space_ok(free_bytes: u64, spec: &ModelSpec) -> bool {
    free_bytes >= spec.size_bytes + spec.size_bytes / 10
}

/// What the machine can do, as detected by `voice-be`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct HardwareProfile {
    /// **Physical** cores: what the matrix kernels actually scale with (hyper-threads add ~30 %).
    pub cores: usize,
    /// Logical processors, for display only.
    pub logical: usize,
    pub ram_gb: f32,
    /// GPU acceleration (Metal) is compiled in and available.
    pub gpu: bool,
}

/// Measured real-time factors (`took / audio`) by model id, from real runs on this machine.
pub type Measured = HashMap<String, f32>;

/// A model keeps up with speech when it transcribes at least ~2.5x faster than it is spoken.
pub const COMFORT_RTF: f32 = 0.4;
/// The most a recommended model may cost per second of speech: slower ones cannot show live text.
pub const RECOMMEND_MAX_RTF: f32 = 0.65;
/// Beyond this, transcribing while the user still speaks falls behind: wait for the release.
pub const STREAMING_MAX_RTF: f32 = 0.7;
/// Real-time factor of `tiny` (cost 1) on 4 physical cores for a 5 s dictation clip. The costs in
/// the catalogue come from timing the real encoder architectures (random weights, optimised build)
/// on an i7-1165G7 with 4 threads over a full 30 s window: tiny 1.1 s, base 2.4, small 8.5,
/// medium 25, large/turbo 44; a short clip pays about a quarter of that, plus the decoder.
const RTF_PER_COST_4_THREADS: f32 = 0.085;

/// Expected real-time factor of `spec` on `hw` for a short dictation clip, before any measurement.
pub fn estimated_rtf(spec: &ModelSpec, hw: &HardwareProfile) -> f32 {
    let threads = hw.cores.clamp(1, 8) as f32;
    let gpu = if hw.gpu { 0.25 } else { 1.0 };
    spec.cost * RTF_PER_COST_4_THREADS * (4.0 / threads) * gpu
}

/// How far this machine departs from the estimate, learned from the models that were actually run
/// (geometric mean of measured / estimated, clamped so one odd run cannot swing everything).
pub fn calibration(hw: &HardwareProfile, measured: &Measured) -> f32 {
    let ratios: Vec<f32> = measured
        .iter()
        .filter_map(|(id, rtf)| find_model(id).map(|m| *rtf / estimated_rtf(m, hw).max(1e-3)))
        .filter(|r| r.is_finite() && *r > 0.0)
        .collect();
    if ratios.is_empty() {
        return 1.0;
    }
    let mean_ln = ratios.iter().map(|r| r.ln()).sum::<f32>() / ratios.len() as f32;
    mean_ln.exp().clamp(0.25, 8.0)
}

/// The real-time factor to plan with, and whether it was measured. A measurement always beats the
/// estimate; unmeasured models are estimated and scaled by what the measured ones revealed.
pub fn expected_rtf(spec: &ModelSpec, hw: &HardwareProfile, measured: &Measured) -> (f32, bool) {
    match measured.get(spec.id) {
        Some(rtf) => (*rtf, true),
        None => (estimated_rtf(spec, hw) * calibration(hw, measured), false),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Recommendation {
    pub id: &'static str,
    pub reason: String,
}

/// Whether live (by-segment) transcription is comfortable at this real-time factor.
pub fn streaming_comfortable(rtf: f32) -> bool {
    rtf <= STREAMING_MAX_RTF
}

fn language_ok(m: &ModelSpec, language: &str) -> bool {
    let non_english = language != "auto" && language != "en";
    let min_accuracy = if non_english { 3 } else { 1 };
    m.accuracy >= min_accuracy && !(m.english_only && language != "en")
}

/// Pick the most accurate model that keeps up with speech on `hw` for `language`.
///
/// "Keeps up" is a real-time factor <= [`RECOMMEND_MAX_RTF`], from a measurement when there is one and
/// from the hardware estimate otherwise. CPU-only machines therefore land on Small / Base: the
/// Large and Turbo encoders take seconds per phrase on a laptop. Non-English languages never get
/// a tiny/base model (poor there). RAM must hold the file twice over.
pub fn recommend(hw: &HardwareProfile, language: &str, measured: &Measured) -> Recommendation {
    let ram = hw.ram_gb as f64 * 1_073_741_824.0;
    let candidates: Vec<(&ModelSpec, f32, bool)> = CATALOGUE
        .iter()
        .filter(|m| language_ok(m, language) && (m.size_bytes as f64) * 2.0 < ram)
        .map(|m| {
            let (rtf, was_measured) = expected_rtf(m, hw, measured);
            (m, rtf, was_measured)
        })
        .collect();
    // Most accurate first; among equals the full-precision one, then the faster.
    let best = candidates.iter().filter(|(_, rtf, _)| *rtf <= RECOMMEND_MAX_RTF).max_by(|a, b| {
        a.0.accuracy
            .cmp(&b.0.accuracy)
            .then(b.0.quantized.cmp(&a.0.quantized))
            .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    match best {
        Some((m, rtf, was_measured)) => Recommendation {
            id: m.id,
            reason: format!(
                "Keeps up with your voice on this PC: {} ({:.1} s of work per second of speech{}).",
                if *was_measured { "measured" } else { "estimated" },
                rtf,
                if hw.gpu { ", with GPU acceleration" } else { "" }
            ),
        },
        None => {
            let lightest = candidates
                .iter()
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(m, rtf, _)| (*m, *rtf))
                .unwrap_or((&CATALOGUE[0], 0.1));
            Recommendation {
                id: lightest.0.id,
                reason: format!(
                    "The lightest model that suits {}; on this PC it will not feel instant (about {:.1} s of work per second of speech).",
                    if language == "auto" { "the languages you may speak" } else { "your language" },
                    lightest.1
                ),
            }
        }
    }
}

/// How a measured run feels.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Comfort {
    Instant,
    Comfortable,
    Slow,
    TooSlow,
}

/// Verdict from the real-time factor (`took / audio`).
pub fn comfort(audio_secs: f32, took_secs: f32) -> Comfort {
    let rtf = took_secs / audio_secs.max(0.1);
    if rtf <= 0.15 {
        Comfort::Instant
    } else if rtf <= 0.4 {
        Comfort::Comfortable
    } else if rtf <= 1.0 {
        Comfort::Slow
    } else {
        Comfort::TooSlow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(cores: usize, ram: f32, gpu: bool) -> HardwareProfile {
        HardwareProfile { cores, logical: cores * 2, ram_gb: ram, gpu }
    }

    fn pick(h: HardwareProfile, lang: &str) -> &'static str {
        recommend(&h, lang, &Measured::new()).id
    }

    #[test]
    fn catalogue_is_consistent() {
        for m in CATALOGUE {
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(m.file.starts_with("ggml-") && m.file.ends_with(".bin"));
            assert!(asset_url(m).starts_with("https://huggingface.co/"));
        }
        let mut ids: Vec<_> = CATALOGUE.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOGUE.len());
    }

    /// The table that decides what the picker badges, per hardware class and language.
    #[test]
    fn recommendation_table() {
        // 4 physical cores, 16 GB, CPU only (the reported machine): Small, never Medium/Large/Turbo.
        assert_eq!(pick(hw(4, 16.0, false), "it"), "small");
        assert_eq!(pick(hw(4, 16.0, false), "auto"), "small");
        assert_eq!(pick(hw(4, 16.0, false), "en"), "small.en");
        // 8 physical cores: still Small (Medium needs ~0.9 s per second of speech).
        assert_eq!(pick(hw(8, 32.0, false), "it"), "small");
        assert_eq!(pick(hw(16, 64.0, false), "en"), "small.en");
        // 2 cores: nothing keeps up in Italian; the lightest acceptable one.
        assert_eq!(pick(hw(2, 4.0, false), "it"), "small-q5_1");
        // Two cores in English can still use Base.
        assert_eq!(pick(hw(2, 4.0, false), "en"), "base.en");
        // A GPU opens the bigger models.
        assert_eq!(pick(hw(4, 16.0, true), "it"), "large-v3-turbo");
        assert_eq!(pick(hw(8, 32.0, true), "it"), "large-v3-turbo");
        // Little memory excludes big files whatever the CPU.
        assert!(find_model(pick(hw(32, 1.0, true), "auto")).unwrap().size_bytes < 600_000_000);
    }

    #[test]
    fn cpu_only_never_recommends_a_model_that_cannot_keep_up() {
        for cores in [1, 2, 4, 6, 8, 12, 32] {
            for lang in ["auto", "en", "it", "de"] {
                let h = hw(cores, 64.0, false);
                let r = recommend(&h, lang, &Measured::new());
                let m = find_model(r.id).unwrap();
                if estimated_rtf(m, &h) > RECOMMEND_MAX_RTF {
                    // Only the "nothing fits" fallback may exceed it, and then it is the lightest allowed.
                    assert!(r.reason.starts_with("The lightest"), "{cores} {lang} {}", r.id);
                }
                assert!(!m.id.starts_with("large"), "{cores} {lang}");
            }
        }
    }

    #[test]
    fn italian_never_gets_tiny_or_base() {
        let r = recommend(&hw(2, 4.0, false), "it", &Measured::new());
        let m = find_model(r.id).unwrap();
        assert!(m.accuracy >= 3 && !m.english_only, "{}", r.id);
    }

    #[test]
    fn a_measurement_overrides_the_estimate() {
        let h = hw(4, 16.0, false);
        // Small turned out slower than estimated: the picker steps down.
        let mut measured = Measured::new();
        measured.insert("small-q5_1".into(), 0.9);
        let r = recommend(&h, "auto", &measured);
        assert_ne!(r.id, "small-q5_1");
        assert!(estimated_rtf(find_model(r.id).unwrap(), &h) * calibration(&h, &measured) <= RECOMMEND_MAX_RTF);
        // Medium turned out much faster than estimated: it is allowed.
        let mut fast = Measured::new();
        fast.insert("medium-q5_0".into(), 0.3);
        let r = recommend(&h, "it", &fast);
        assert!(find_model(r.id).unwrap().accuracy >= 4, "{}", r.id);
    }

    #[test]
    fn calibration_scales_unmeasured_models() {
        let h = hw(4, 16.0, false);
        let small = find_model("small").unwrap();
        // A measurement twice the estimate means this PC is twice as slow as assumed.
        let slow: Measured = [("small-q5_1".to_string(), estimated_rtf(find_model("small-q5_1").unwrap(), &h) * 2.0)].into();
        let (rtf, measured) = expected_rtf(small, &h, &slow);
        assert!(!measured);
        assert!((rtf - estimated_rtf(small, &h) * 2.0).abs() < 0.01);
        assert_eq!(calibration(&h, &Measured::new()), 1.0);
    }

    #[test]
    fn streaming_threshold() {
        assert!(streaming_comfortable(0.5));
        assert!(!streaming_comfortable(1.2));
    }

    #[test]
    fn disk_check_has_headroom() {
        let m = find_model("small").unwrap();
        assert!(!free_space_ok(m.size_bytes, m));
        assert!(free_space_ok(m.size_bytes * 2, m));
    }

    #[test]
    fn comfort_thresholds() {
        assert_eq!(comfort(10.0, 1.0), Comfort::Instant);
        assert_eq!(comfort(10.0, 2.0), Comfort::Comfortable);
        assert_eq!(comfort(10.0, 8.0), Comfort::Slow);
        assert_eq!(comfort(10.0, 20.0), Comfort::TooSlow);
    }
}
