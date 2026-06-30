//! Drum detection via energy-onset detection + a zero-crossing-rate timbre cue.
//!
//! Time-domain and FFT-free: a percussive hit is a sharp rise in short-time
//! energy; its brightness (zero-crossing rate) crudely separates a booming kick
//! from a snare from a hat. Rough, but model-free and enough for a baseline drum
//! part that the user can refine in the editor.

use crate::note::{DetNote, DRUM_CHANNEL};

const WIN: usize = 1024;
const HOP: usize = 512;
/// A hit is `flux > local_mean * this`.
const FLUX_RATIO: f32 = 2.5;
/// Local window (in frames) for the adaptive threshold mean.
const LOCAL_WIN: usize = 8;
/// Refractory gap between onsets, in seconds (no double-triggers on one hit).
const REFRACTORY_SEC: f64 = 0.04;
/// Emitted hit length (drums are one-shots; the exact value barely matters).
const HIT_SEC: f64 = 0.05;

// GM percussion keys.
const KICK: u8 = 36;
const SNARE: u8 = 38;
const HAT: u8 = 42;

/// Detect drum hits in `samples`, on the GM drum channel.
pub fn detect_drums(samples: &[f32], sample_rate: u32) -> Vec<DetNote> {
    if samples.len() < WIN {
        return Vec::new();
    }
    let (energy, zcr) = frame_features(samples);

    // Detection function: positive energy flux (rectified first difference).
    let mut flux = vec![0.0f32; energy.len()];
    for i in 1..energy.len() {
        flux[i] = (energy[i] - energy[i - 1]).max(0.0);
    }

    let hop_sec = HOP as f64 / sample_rate as f64;
    let refractory = (REFRACTORY_SEC / hop_sec).ceil() as i64;
    let mut last = -refractory - 1;
    let mut notes = Vec::new();

    for i in 1..flux.len().saturating_sub(1) {
        let lo = i.saturating_sub(LOCAL_WIN);
        let hi = (i + LOCAL_WIN).min(flux.len());
        let mean = flux[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        let is_peak = flux[i] >= flux[i - 1] && flux[i] >= flux[i + 1];
        if is_peak && flux[i] > mean * FLUX_RATIO + 1e-6 && (i as i64 - last) >= refractory {
            last = i as i64;
            notes.push(DetNote {
                start_sec: i as f64 * hop_sec,
                dur_sec: HIT_SEC,
                pitch: drum_for_zcr(zcr[i]),
                vel: 100,
                channel: DRUM_CHANNEL,
            });
        }
    }
    notes
}

/// Per-frame short-time energy and zero-crossing rate.
fn frame_features(samples: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut energy = Vec::new();
    let mut zcr = Vec::new();
    let mut pos = 0;
    while pos + WIN <= samples.len() {
        let f = &samples[pos..pos + WIN];
        energy.push(f.iter().map(|s| s * s).sum::<f32>());
        let crossings = f
            .windows(2)
            .filter(|w| (w[0] <= 0.0) != (w[1] <= 0.0))
            .count();
        zcr.push(crossings as f32 / WIN as f32);
        pos += HOP;
    }
    (energy, zcr)
}

/// Map brightness (ZCR) to a GM drum: dull → kick, mid → snare, bright → hat.
fn drum_for_zcr(zcr: f32) -> u8 {
    if zcr < 0.05 {
        KICK
    } else if zcr < 0.15 {
        SNARE
    } else {
        HAT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a buffer with `n` sharp clicks evenly spaced over `secs`.
    fn clicks(n: usize, secs: f64, sr: u32) -> Vec<f32> {
        let total = (secs * sr as f64) as usize;
        let mut buf = vec![0.0f32; total];
        for k in 0..n {
            let at = k * total / n;
            // A short bright burst (high ZCR) so it reads as a hit.
            for j in 0..200 {
                if at + j < total {
                    buf[at + j] = if j % 2 == 0 { 0.9 } else { -0.9 };
                }
            }
        }
        buf
    }

    #[test]
    fn finds_roughly_the_right_number_of_onsets() {
        let notes = detect_drums(&clicks(4, 2.0, 44_100), 44_100);
        // Onset detection is approximate; just assert it found the hits, not noise.
        assert!(
            (3..=5).contains(&notes.len()),
            "expected ~4 onsets, got {}",
            notes.len()
        );
        assert!(notes.iter().all(|n| n.channel == DRUM_CHANNEL));
    }

    #[test]
    fn silence_has_no_onsets() {
        assert!(detect_drums(&vec![0.0; 44_100], 44_100).is_empty());
    }
}
