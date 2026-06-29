//! Monophonic pitch detection via **YIN** (de Cheveigné & Kawahara, 2002).
//!
//! Per frame: the difference function, its cumulative-mean normalisation, the
//! first dip below an absolute threshold as the period, then **parabolic
//! interpolation** of that minimum for sub-sample accuracy. Consecutive frames
//! agreeing on a (rounded) MIDI note merge into one note. Monophonic and
//! time-domain — rough on a full mix, but model-free.
//!
//! **Decimation.** Pitch lives in `F_MIN..F_MAX` (70–1200 Hz), far below the
//! source's Nyquist, so we box-average the signal down to ~[`TARGET_RATE`] before
//! analysis. YIN's difference function is `O(tau_max · frame)` per frame and both
//! scale with the rate, so decimating by `M` cuts the work by ~`M²` (≈25× at
//! 44.1 kHz, where `M ≈ 5`). The target rate is chosen so YIN still *resolves* the
//! period at `F_MAX` (see [`TARGET_RATE`]) — decimate harder and high notes fold
//! an octave low — and parabolic interpolation then recovers sub-sample precision.

use crate::note::DetNote;

/// Pitch search range. Below/above this we don't trust a monophonic estimate.
const F_MIN: f64 = 70.0;
const F_MAX: f64 = 1200.0;
/// Decimate to roughly this rate before YIN. Must give YIN enough samples per
/// period to *resolve* the fundamental at `F_MAX`, not merely to represent the
/// signal: the difference function's minimum at the true period has to be deep
/// and well-placed, or the first sub-threshold dip lands on 2× the period and the
/// note reads an octave low. `2·F_MAX` (Nyquist) is far too coarse for that — a
/// `F_MAX` tone then has < 4 samples/period; ~6–8 is what keeps high notes from
/// octave-folding. `9000` gives ≈7.4 samples/period at 1200 Hz while still
/// decimating 44.1/48 kHz ~5×.
const TARGET_RATE: f64 = 9000.0;
/// Analysis window / hop in **time**, so they map to the same musical resolution
/// at any (decimated) rate. ~46 ms window, ~11.6 ms hop (≈86 frames/s).
const WIN_SEC: f64 = 0.046;
const HOP_SEC: f64 = 0.0116;
/// YIN absolute threshold: the first `d'` dip under this is the period.
const YIN_THRESHOLD: f32 = 0.15;
/// Fallback acceptance: take the global `d'` minimum only if it's this confident.
const FALLBACK_MAX: f32 = 0.30;
/// Shortest run of agreeing frames that counts as a note (filters jitter).
const MIN_FRAMES: usize = 3;
/// Default note velocity (the DSP backend doesn't estimate dynamics).
const DEFAULT_VEL: u8 = 80;

/// Detect monophonic notes in `samples`, on channel 0.
pub fn detect_notes(samples: &[f32], sample_rate: u32) -> Vec<DetNote> {
    detect_notes_with_progress(samples, sample_rate, |_| {})
}

/// As [`detect_notes`], reporting frame-loop progress (`0..=1`) periodically so
/// the shell can drive a live bar.
pub fn detect_notes_with_progress(
    samples: &[f32],
    sample_rate: u32,
    mut on_progress: impl FnMut(f32),
) -> Vec<DetNote> {
    if samples.is_empty() {
        return Vec::new();
    }
    let sr = sample_rate as f64;

    // Decimate to ~TARGET_RATE (box-average = cheap low-pass + downsample).
    let m = (sr / TARGET_RATE).round().max(1.0) as usize;
    let dec = decimate(samples, m);
    let rate = sr / m as f64;

    let frame = (WIN_SEC * rate).round() as usize;
    let hop = ((HOP_SEC * rate).round() as usize).max(1);
    let tau_min = ((rate / F_MAX).floor() as usize).max(2);
    let tau_max = ((rate / F_MIN).ceil() as usize).min(frame.saturating_sub(1));
    if dec.len() < frame || tau_min >= tau_max {
        return Vec::new();
    }
    let hop_sec = hop as f64 / rate;

    let total = dec.len() as f32;
    let mut frames: Vec<Option<i32>> = Vec::new();
    let mut pos = 0;
    let mut since_report = 0;
    while pos + frame <= dec.len() {
        frames.push(yin_frame(&dec[pos..pos + frame], rate, tau_min, tau_max));
        pos += hop;
        since_report += 1;
        if since_report >= 64 {
            since_report = 0;
            on_progress(pos as f32 / total);
        }
    }
    on_progress(1.0);
    group_into_notes(&frames, hop_sec)
}

/// Box-average decimation by `m` (each output is the mean of `m` input samples).
/// `m == 1` is a passthrough copy.
fn decimate(samples: &[f32], m: usize) -> Vec<f32> {
    if m <= 1 {
        return samples.to_vec();
    }
    let n = samples.len() / m;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let base = i * m;
        let mut sum = 0.0f32;
        for j in 0..m {
            sum += samples[base + j];
        }
        out.push(sum / m as f32);
    }
    out
}

/// Estimate the MIDI note of one (decimated) frame, or `None` when unvoiced/out
/// of range. `rate` is the decimated sample rate.
fn yin_frame(x: &[f32], rate: f64, tau_min: usize, tau_max: usize) -> Option<i32> {
    let n = x.len();

    // (1) Difference function.
    let mut d = vec![0.0f32; tau_max + 1];
    for (tau, slot) in d.iter_mut().enumerate().take(tau_max + 1).skip(1) {
        let mut sum = 0.0f32;
        for j in 0..(n - tau) {
            let diff = x[j] - x[j + tau];
            sum += diff * diff;
        }
        *slot = sum;
    }

    // (2) Cumulative mean normalised difference.
    let mut dp = vec![1.0f32; tau_max + 1];
    let mut running = 0.0f32;
    for tau in 1..=tau_max {
        running += d[tau];
        dp[tau] = if running > 0.0 {
            d[tau] * tau as f32 / running
        } else {
            1.0
        };
    }

    // (3) Absolute threshold: first dip under it, descended to its local min.
    let mut tau = None;
    let mut t = tau_min;
    while t <= tau_max {
        if dp[t] < YIN_THRESHOLD {
            let mut k = t;
            while k < tau_max && dp[k + 1] < dp[k] {
                k += 1;
            }
            tau = Some(k);
            break;
        }
        t += 1;
    }

    // Fallback: the global minimum in range, if confident enough.
    let tau = tau.or_else(|| {
        let mut best_t = tau_min;
        let mut best_v = dp[tau_min];
        for (t, &v) in dp.iter().enumerate().take(tau_max + 1).skip(tau_min) {
            if v < best_v {
                best_v = v;
                best_t = t;
            }
        }
        (best_v < FALLBACK_MAX).then_some(best_t)
    })?;

    // (4) Parabolic interpolation around the minimum → sub-sample period, the key
    // to keeping pitch accurate at the low (decimated) rate.
    let period = parabolic(&dp, tau, tau_min, tau_max);
    let f0 = rate / period;
    if !(F_MIN * 0.9..=F_MAX * 1.1).contains(&f0) {
        return None;
    }
    let midi = (69.0 + 12.0 * (f0 / 440.0).log2()).round() as i32;
    (0..=127).contains(&midi).then_some(midi)
}

/// Refine an integer period `tau` to sub-sample precision by fitting a parabola
/// to `dp[tau-1..=tau+1]`. Falls back to the integer at the search edges.
fn parabolic(dp: &[f32], tau: usize, lo: usize, hi: usize) -> f64 {
    if tau <= lo || tau >= hi {
        return tau as f64;
    }
    let s0 = dp[tau - 1] as f64;
    let s1 = dp[tau] as f64;
    let s2 = dp[tau + 1] as f64;
    let denom = s0 + s2 - 2.0 * s1;
    if denom.abs() < 1e-12 {
        return tau as f64;
    }
    // Clamp the correction to ±1 sample as a safety against a flat/degenerate fit.
    tau as f64 + ((s0 - s2) / (2.0 * denom)).clamp(-1.0, 1.0)
}

/// Merge runs of frames agreeing on a MIDI note into [`DetNote`]s. `hop_sec` is
/// the time between consecutive frames.
fn group_into_notes(frames: &[Option<i32>], hop_sec: f64) -> Vec<DetNote> {
    let mut notes = Vec::new();
    let mut i = 0;
    while i < frames.len() {
        let Some(m) = frames[i] else {
            i += 1;
            continue;
        };
        let start = i;
        let mut j = i + 1;
        while j < frames.len() && frames[j] == Some(m) {
            j += 1;
        }
        let len = j - start;
        if len >= MIN_FRAMES {
            notes.push(DetNote {
                start_sec: start as f64 * hop_sec,
                dur_sec: len as f64 * hop_sec,
                pitch: m as u8,
                vel: DEFAULT_VEL,
                channel: 0,
            });
        }
        i = j;
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, secs: f64, sr: u32) -> Vec<f32> {
        let n = (secs * sr as f64) as usize;
        (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / sr as f64).sin() as f32 * 0.8)
            .collect()
    }

    fn dominant(notes: &[DetNote]) -> &DetNote {
        notes.iter().max_by(|a, b| a.dur_sec.total_cmp(&b.dur_sec)).unwrap()
    }

    #[test]
    fn detects_a440_as_midi_69() {
        let notes = detect_notes(&sine(440.0, 0.7, 44_100), 44_100);
        assert!(!notes.is_empty(), "a clear tone must yield a note");
        assert!((dominant(&notes).pitch as i32 - 69).abs() <= 1, "expected ~69, got {}", dominant(&notes).pitch);
        assert_eq!(dominant(&notes).channel, 0);
    }

    #[test]
    fn detects_octave_up() {
        let notes = detect_notes(&sine(880.0, 0.7, 44_100), 44_100);
        assert!((dominant(&notes).pitch as i32 - 81).abs() <= 1, "expected ~81, got {}", dominant(&notes).pitch);
    }

    #[test]
    fn parabolic_keeps_high_notes_accurate_after_decimation() {
        // ~988 Hz (B5) — near F_MAX, where integer-tau at the decimated rate would
        // be coarse without parabolic interpolation. Expect B5 = MIDI 83 (±1).
        let notes = detect_notes(&sine(987.77, 0.7, 44_100), 44_100);
        assert!(!notes.is_empty());
        assert!((dominant(&notes).pitch as i32 - 83).abs() <= 1, "expected ~83, got {}", dominant(&notes).pitch);
    }

    #[test]
    fn detects_at_48k_too() {
        let notes = detect_notes(&sine(440.0, 0.7, 48_000), 48_000);
        assert!(!notes.is_empty());
        assert!((dominant(&notes).pitch as i32 - 69).abs() <= 1);
    }

    #[test]
    fn silence_has_no_notes() {
        assert!(detect_notes(&vec![0.0; 44_100], 44_100).is_empty());
    }
}
