//! Speech detection on 20 ms frames: an adaptive energy gate, the end-of-speech state machine
//! and the budget that keeps the wake-word detector from running continuously.
//!
//! Deliberately cheap: one RMS per frame and a few comparisons, so an armed-but-silent
//! microphone costs essentially nothing. Everything is pure and driven by the caller's frames.

/// Frame length every function in this module assumes.
pub const FRAME_MS: u32 = 20;

/// Adaptive energy gate. The noise floor follows the quiet frames, so a fan or a hiss raises the
/// bar instead of triggering it.
#[derive(Debug, Clone)]
pub struct SpeechGate {
    floor: f32,
    abs_min: f32,
    ratio: f32,
}

impl Default for SpeechGate {
    fn default() -> Self {
        Self { floor: 0.003, abs_min: 0.006, ratio: 3.0 }
    }
}

impl SpeechGate {
    /// Current decision threshold (RMS).
    pub fn threshold(&self) -> f32 {
        self.abs_min.max(self.floor * self.ratio)
    }

    /// Classify a frame and learn from it when it is quiet.
    pub fn is_voiced(&mut self, rms: f32) -> bool {
        let voiced = rms > self.threshold();
        if !voiced {
            self.floor = (self.floor * 0.95 + rms * 0.05).min(0.02);
        }
        voiced
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointEvent {
    None,
    /// Enough consecutive voiced frames: an utterance began.
    SpeechStart,
    /// Silence outlasted the hangover. `voiced_ms` is how much of it was actual speech.
    SpeechEnd { voiced_ms: u32 },
}

#[derive(Debug, Clone, Copy)]
pub struct EndpointerConfig {
    /// Consecutive voiced frames needed to call it speech (rejects clicks).
    pub start_frames: u32,
    /// Silence that ends the utterance.
    pub end_silence_ms: u32,
}

impl Default for EndpointerConfig {
    fn default() -> Self {
        Self { start_frames: 5, end_silence_ms: 1200 }
    }
}

/// End-of-speech detection. Feed it one RMS per 20 ms frame.
#[derive(Debug, Clone)]
pub struct Endpointer {
    cfg: EndpointerConfig,
    gate: SpeechGate,
    in_speech: bool,
    run_voiced: u32,
    silence_frames: u32,
    voiced_frames: u32,
}

impl Endpointer {
    pub fn new(cfg: EndpointerConfig) -> Self {
        Self { cfg, gate: SpeechGate::default(), in_speech: false, run_voiced: 0, silence_frames: 0, voiced_frames: 0 }
    }

    /// An endpointer that starts already inside an utterance — for a request whose first words
    /// were heard before the caller knew it was one.
    pub fn new_in_speech(cfg: EndpointerConfig) -> Self {
        let mut ep = Self::new(cfg);
        ep.in_speech = true;
        ep.voiced_frames = cfg.start_frames;
        ep
    }

    pub fn in_speech(&self) -> bool {
        self.in_speech
    }

    pub fn push(&mut self, rms: f32) -> EndpointEvent {
        let voiced = self.gate.is_voiced(rms);
        if !self.in_speech {
            self.run_voiced = if voiced { self.run_voiced + 1 } else { 0 };
            if self.run_voiced >= self.cfg.start_frames {
                self.in_speech = true;
                self.silence_frames = 0;
                self.voiced_frames = self.run_voiced;
                return EndpointEvent::SpeechStart;
            }
            return EndpointEvent::None;
        }
        if voiced {
            self.silence_frames = 0;
            self.voiced_frames += 1;
            return EndpointEvent::None;
        }
        self.silence_frames += 1;
        if self.silence_frames * FRAME_MS >= self.cfg.end_silence_ms {
            let voiced_ms = self.voiced_frames * FRAME_MS;
            self.in_speech = false;
            self.run_voiced = 0;
            self.silence_frames = 0;
            self.voiced_frames = 0;
            return EndpointEvent::SpeechEnd { voiced_ms };
        }
        EndpointEvent::None
    }
}

/// Whether a finished recording holds enough speech to be worth transcribing — the guard against
/// the hallucinated "Thank you for watching" Whisper produces from silence.
pub fn contains_speech(frame_rms: &[f32], min_voiced_ms: u32) -> bool {
    let mut gate = SpeechGate::default();
    let voiced = frame_rms.iter().filter(|r| gate.is_voiced(**r)).count() as u32;
    voiced * FRAME_MS >= min_voiced_ms
}

/// Rate limit for the wake-word detector, so continuous chatter (a TV, a call) cannot keep a
/// model busy: a minimum gap between runs and a ceiling per minute.
#[derive(Debug, Clone)]
pub struct WakeBudget {
    min_gap_ms: u64,
    max_per_minute: usize,
    runs: Vec<u64>,
}

impl Default for WakeBudget {
    fn default() -> Self {
        Self { min_gap_ms: 3_000, max_per_minute: 8, runs: Vec::new() }
    }
}

impl WakeBudget {
    /// Ask to run the detector at `now_ms`; records the run when granted.
    pub fn allow(&mut self, now_ms: u64) -> bool {
        self.runs.retain(|t| now_ms.saturating_sub(*t) < 60_000);
        if self.runs.last().is_some_and(|t| now_ms.saturating_sub(*t) < self.min_gap_ms) {
            return false;
        }
        if self.runs.len() >= self.max_per_minute {
            return false;
        }
        self.runs.push(now_ms);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(ep: &mut Endpointer, rms: f32, frames: u32) -> Vec<EndpointEvent> {
        (0..frames).map(|_| ep.push(rms)).filter(|e| *e != EndpointEvent::None).collect()
    }

    #[test]
    fn silence_never_triggers() {
        let mut ep = Endpointer::new(EndpointerConfig::default());
        assert!(feed(&mut ep, 0.001, 500).is_empty());
    }

    #[test]
    fn a_click_is_not_speech() {
        let mut ep = Endpointer::new(EndpointerConfig::default());
        assert!(feed(&mut ep, 0.2, 2).is_empty());
        assert!(feed(&mut ep, 0.001, 50).is_empty());
    }

    #[test]
    fn speech_starts_then_ends_after_hangover() {
        let mut ep = Endpointer::new(EndpointerConfig { start_frames: 5, end_silence_ms: 1200 });
        assert_eq!(feed(&mut ep, 0.1, 50), vec![EndpointEvent::SpeechStart]);
        // 59 silent frames (1180 ms) is not yet the end; the 60th is.
        assert!(feed(&mut ep, 0.001, 59).is_empty());
        assert_eq!(feed(&mut ep, 0.001, 1), vec![EndpointEvent::SpeechEnd { voiced_ms: 1000 }]);
        assert!(!ep.in_speech());
    }

    #[test]
    fn a_short_pause_does_not_end_the_utterance() {
        let mut ep = Endpointer::new(EndpointerConfig::default());
        feed(&mut ep, 0.1, 20);
        assert!(feed(&mut ep, 0.001, 30).is_empty());
        assert!(feed(&mut ep, 0.1, 20).is_empty());
        assert!(ep.in_speech());
    }

    #[test]
    fn gate_adapts_to_steady_noise() {
        let mut g = SpeechGate::default();
        let before = g.threshold();
        for _ in 0..400 {
            g.is_voiced(0.005);
        }
        assert!(g.threshold() >= before);
        assert!(!g.is_voiced(0.005));
        assert!(g.is_voiced(0.2));
    }

    #[test]
    fn recording_without_speech_is_rejected() {
        assert!(!contains_speech(&vec![0.001; 200], 300));
        let mut loud = vec![0.001; 50];
        loud.extend(vec![0.1; 30]);
        assert!(contains_speech(&loud, 300));
    }

    #[test]
    fn wake_budget_spaces_and_caps_runs() {
        let mut b = WakeBudget::default();
        assert!(b.allow(0));
        assert!(!b.allow(1_000));
        assert!(b.allow(3_000));
        let mut t = 3_000;
        let mut granted = 2;
        for _ in 0..18 {
            t += 3_000;
            if b.allow(t) {
                granted += 1;
            }
        }
        assert_eq!(granted, 8);
        assert!(b.allow(t + 120_000));
    }
}
