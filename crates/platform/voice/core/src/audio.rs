//! Audio helpers: whisper wants 16 kHz mono f32, devices give anything.

/// Whisper's fixed input rate.
pub const SAMPLE_RATE: u32 = 16_000;

/// Average interleaved channels into one.
pub fn downmix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|f| f.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Resample mono audio from `from_hz` to 16 kHz. Linear interpolation, preceded by a box
/// average when decimating (a cheap anti-alias). Quality is plenty for speech recognition and
/// it costs no dependency.
pub fn resample_to_16k(mono: &[f32], from_hz: u32) -> Vec<f32> {
    if from_hz == SAMPLE_RATE || mono.is_empty() || from_hz == 0 {
        return mono.to_vec();
    }
    let ratio = from_hz as f64 / SAMPLE_RATE as f64;
    let out_len = (mono.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    let window = ratio.ceil().max(1.0) as usize;
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos as usize;
        if ratio > 1.0 {
            let end = (idx + window).min(mono.len());
            let slice = &mono[idx..end];
            out.push(slice.iter().sum::<f32>() / slice.len().max(1) as f32);
        } else {
            let frac = (pos - idx as f64) as f32;
            let a = mono[idx];
            let b = *mono.get(idx + 1).unwrap_or(&a);
            out.push(a + (b - a) * frac);
        }
    }
    out
}

/// Root-mean-square of a frame.
pub fn frame_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

/// RMS mapped to a 0..1 meter value (−60 dBFS → 0, 0 dBFS → 1).
pub fn level_from_rms(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * rms.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        assert_eq!(downmix_to_mono(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(downmix_to_mono(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn resample_changes_length_proportionally() {
        let one_sec_48k = vec![0.25f32; 48_000];
        let out = resample_to_16k(&one_sec_48k, 48_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.25).abs() < 1e-6));
        let up = resample_to_16k(&vec![0.5f32; 8_000], 8_000);
        assert_eq!(up.len(), 16_000);
    }

    #[test]
    fn resample_is_identity_at_16k() {
        let v = vec![0.1, -0.2, 0.3];
        assert_eq!(resample_to_16k(&v, 16_000), v);
    }

    #[test]
    fn level_is_monotonic_and_clamped() {
        assert_eq!(level_from_rms(0.0), 0.0);
        assert!(level_from_rms(0.01) < level_from_rms(0.1));
        assert_eq!(level_from_rms(5.0), 1.0);
    }
}
