//! Pure decisions about one transcription run: how long it may take, how much of whisper's
//! encoder window to use, how many threads to give it.

use std::time::Duration;

/// Hard ceiling for a transcription of `audio_secs` seconds: a generous multiple of the audio plus
/// a fixed allowance for loading and warming up, never less than 30 s. A run that exceeds it is
/// stopped and reported as an error instead of leaving the UI waiting.
pub fn transcription_timeout(audio_secs: f32) -> Duration {
    let secs = 20.0 + audio_secs.max(0.0) * 8.0;
    Duration::from_secs_f32(secs.max(30.0))
}

/// Whisper's encoder always processes a 30 s window (1500 frames, 50 per second) unless told
/// otherwise, so a 3 s clip costs as much as a 30 s one. Limiting the context to the clip (plus
/// headroom) is the single biggest speed-up for short dictation segments. `None` (full window)
/// for clips that fill it anyway.
pub fn audio_ctx_for(audio_secs: f32) -> Option<i32> {
    if audio_secs >= 25.0 {
        return None;
    }
    let frames = (audio_secs.max(0.0) * 50.0).ceil() as i32 + 150;
    Some(((frames + 63) / 64 * 64).clamp(384, 1500))
}

/// Threads for a given physical core count: all of them up to 8 (ggml's matrix kernels scale with
/// real cores and lose to SMT contention beyond that), at least one.
pub fn threads_for_cores(physical: usize) -> usize {
    physical.clamp(1, 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_has_a_floor_and_scales() {
        assert_eq!(transcription_timeout(0.0), Duration::from_secs(30));
        assert_eq!(transcription_timeout(5.0), Duration::from_secs(60));
        assert!(transcription_timeout(60.0) > Duration::from_secs(400));
    }

    #[test]
    fn short_clips_get_a_short_context() {
        let short = audio_ctx_for(2.0).unwrap();
        let longer = audio_ctx_for(12.0).unwrap();
        assert!(short < longer && longer < 1500);
        assert_eq!(short % 64, 0);
        assert!(audio_ctx_for(0.1).unwrap() >= 384);
        assert_eq!(audio_ctx_for(26.0), None);
    }

    #[test]
    fn threads_follow_physical_cores() {
        assert_eq!(threads_for_cores(0), 1);
        assert_eq!(threads_for_cores(4), 4);
        assert_eq!(threads_for_cores(32), 8);
    }
}
