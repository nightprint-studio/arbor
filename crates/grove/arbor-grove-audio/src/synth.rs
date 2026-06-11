//! The default synth voice: a band-unlimited oscillator + an [`Adsr`] envelope.
//!
//! This is grove's **fallback sound** — what a `Named` source plays when the
//! registry can't resolve it to an SFZ instrument, and the built-in
//! `synth.*` presets (`synth.bass`, `synth.pad`, `synth.pluck`). It is pitched
//! by the voice's `note`; unpitched triggers fall back to a fixed reference.
//!
//! Everything here is allocation-free once constructed: an oscillator is a phase
//! accumulator, the envelope four line segments. The RT renderer ticks them one
//! sample at a time inside the voice DSP chain (`crate::voice`).

/// The classic four oscillator shapes. Saw/square use a naive (aliasing) form —
/// adequate for a live-coding default; band-limiting is a later refinement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    /// Bright, buzzy — the default for basses/leads.
    Saw,
    /// Hollow, square — good for plucks/chip tones.
    Square,
    /// Pure tone, no harmonics.
    Sine,
    /// Soft, few harmonics — pads.
    Triangle,
}

impl Waveform {
    /// Evaluate the waveform at phase `p ∈ [0, 1)`, output in `[-1, 1]`.
    fn sample(self, p: f32) -> f32 {
        match self {
            Waveform::Saw => 2.0 * p - 1.0,
            Waveform::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Sine => (p * std::f32::consts::TAU).sin(),
            Waveform::Triangle => 4.0 * (p - 0.5).abs() - 1.0,
        }
    }
}

/// A single phase-accumulating oscillator at a fixed frequency.
#[derive(Clone, Copy, Debug)]
pub struct Oscillator {
    waveform: Waveform,
    phase: f32,
    /// Phase increment per sample (`freq / sample_rate`).
    step: f32,
}

impl Oscillator {
    /// Build an oscillator at `freq` Hz for `sample_rate`.
    pub fn new(waveform: Waveform, freq: f32, sample_rate: f32) -> Self {
        Oscillator {
            waveform,
            phase: 0.0,
            step: freq / sample_rate,
        }
    }

    /// Produce the next sample and advance the phase.
    pub fn next_sample(&mut self) -> f32 {
        let s = self.waveform.sample(self.phase);
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        s
    }
}

/// The four stages an [`Adsr`] envelope moves through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    Done,
}

/// Absolute level below which a releasing envelope is considered finished and
/// the voice is freed. ~-80 dB: inaudible, and low enough that the final block's
/// peak stays well under any practical silence floor.
const RELEASE_FLOOR: f32 = 1.0e-4;

/// A linear attack / linear decay / **exponential release** ADSR envelope,
/// ticked one sample at a time. Times are in seconds, sustain is a level in
/// `0..1`.
///
/// Attack and decay run automatically on trigger; the envelope then holds at
/// `sustain` until [`release`](Adsr::release) is called (the voice's
/// `dur_frames` deadline, or a natural one-shot end), after which it ramps to
/// silence over the release time and reports [`is_done`](Adsr::is_done).
#[derive(Clone, Copy, Debug)]
pub struct Adsr {
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    sample_rate: f32,
    stage: Stage,
    /// Current envelope level `0..1`.
    level: f32,
    /// Per-sample multiplier for the exponential release, computed when
    /// `release()` is called: `k^n = 1e-3` over `n` = release samples, so the
    /// level reaches ~-60 dB after the release time, then frees below
    /// [`RELEASE_FLOOR`]. Decays from wherever the level was (a voice can be
    /// released mid-attack), so no jump.
    release_coeff: f32,
    /// Progress through the current timed stage, in samples.
    pos: f32,
}

impl Adsr {
    /// Build an envelope. Times are clamped to a tiny floor so a zero stage
    /// still advances (no divide-by-zero, no stuck level).
    pub fn new(attack: f32, decay: f32, sustain: f32, release: f32, sample_rate: f32) -> Self {
        Adsr {
            attack: attack.max(0.0),
            decay: decay.max(0.0),
            sustain: sustain.clamp(0.0, 1.0),
            release: release.max(0.0),
            sample_rate,
            stage: Stage::Attack,
            level: 0.0,
            release_coeff: 0.0,
            pos: 0.0,
        }
    }

    /// Samples for a stage time, with a one-sample floor.
    fn stage_samples(&self, secs: f32) -> f32 {
        (secs * self.sample_rate).max(1.0)
    }

    /// Move to the release stage, decaying exponentially from the current level.
    /// Idempotent.
    pub fn release(&mut self) {
        if self.stage != Stage::Release && self.stage != Stage::Done {
            // k such that k^n = 1e-3 (≈-60 dB) over n = release samples.
            let n = self.stage_samples(self.release);
            self.release_coeff = (1.0e-3_f32).powf(1.0 / n);
            self.stage = Stage::Release;
            self.pos = 0.0;
        }
    }

    /// Whether the envelope has finished its release and outputs silence.
    pub fn is_done(&self) -> bool {
        self.stage == Stage::Done
    }

    /// Whether the envelope is in (or past) its release stage.
    pub fn is_releasing(&self) -> bool {
        matches!(self.stage, Stage::Release | Stage::Done)
    }

    /// The current envelope level `0..1` (for voice-stealing).
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Advance one sample and return the new level `0..1`.
    pub fn next_level(&mut self) -> f32 {
        match self.stage {
            Stage::Attack => {
                let n = self.stage_samples(self.attack);
                self.level = (self.pos / n).min(1.0);
                self.pos += 1.0;
                if self.pos >= n {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                    self.pos = 0.0;
                }
            }
            Stage::Decay => {
                let n = self.stage_samples(self.decay);
                let t = (self.pos / n).min(1.0);
                self.level = 1.0 + (self.sustain - 1.0) * t;
                self.pos += 1.0;
                if self.pos >= n {
                    self.level = self.sustain;
                    self.stage = Stage::Sustain;
                    self.pos = 0.0;
                }
            }
            Stage::Sustain => {
                self.level = self.sustain;
            }
            Stage::Release => {
                // Exponential decay toward zero, freed once inaudible. A linear
                // ramp to 0 leaves a block-sized step (~level·block_len/n) at the
                // cutoff — an audible click, and never below a tight silence
                // floor when sampled per block; the exponential tail shrinks in
                // proportion to the level, so the final block is genuinely quiet.
                self.level *= self.release_coeff;
                if self.level <= RELEASE_FLOOR {
                    self.level = 0.0;
                    self.stage = Stage::Done;
                }
            }
            Stage::Done => {
                self.level = 0.0;
            }
        }
        self.level
    }
}

/// A complete default-synth voice generator: oscillator + its own envelope.
///
/// Produced by [`crate::registry`] when resolving a synth preset (or as the
/// universal fallback). The amplitude envelope here is the voice's dynamics
/// shape; the [`crate::voice`] DSP chain multiplies it by the post-`vel` gain.
#[derive(Clone, Copy, Debug)]
pub struct SynthVoice {
    osc: Oscillator,
    env: Adsr,
}

impl SynthVoice {
    /// Build a synth voice playing `freq` Hz with the given waveform and ADSR.
    pub fn new(waveform: Waveform, freq: f32, env: Adsr, sample_rate: f32) -> Self {
        SynthVoice {
            osc: Oscillator::new(waveform, freq, sample_rate),
            env,
        }
    }

    /// Begin the release phase of the amplitude envelope.
    pub fn release(&mut self) {
        self.env.release();
    }

    /// Whether the envelope has rung out (the voice can be reclaimed).
    pub fn is_done(&self) -> bool {
        self.env.is_done()
    }

    /// The envelope's current level (used by voice-stealing to find the
    /// quietest releasing voice).
    pub fn env_level(&self) -> f32 {
        self.env.level()
    }

    /// Whether the amplitude envelope is in its release stage.
    pub fn is_releasing(&self) -> bool {
        self.env.is_releasing()
    }

    /// Produce the next mono sample (oscillator × envelope).
    pub fn next_sample(&mut self) -> f32 {
        let s = self.osc.next_sample();
        s * self.env.next_level()
    }
}

/// Convert a MIDI-style semitone (`A4 = 69` → 440 Hz) to frequency in Hz.
pub fn midi_to_freq(midi: f32) -> f32 {
    440.0 * 2.0_f32.powf((midi - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn envelope_settles_to_sustain() {
        // Instant attack + decay, sustain 0.5: a few samples in, it holds at 0.5.
        let mut env = Adsr::new(0.0, 0.0, 0.5, 0.1, SR);
        let mut level = 0.0;
        for _ in 0..8 {
            level = env.next_level();
        }
        assert!((level - 0.5).abs() < 1e-6, "should hold at sustain, got {level}");
        assert!(!env.is_done());
    }

    #[test]
    fn release_decays_monotonically_to_done() {
        // sustain 1.0, 50 ms release: the exponential ramp falls without ever
        // rising, reaches `Done`, and lands at exactly 0.
        let mut env = Adsr::new(0.0, 0.0, 1.0, 0.05, SR);
        for _ in 0..4 {
            env.next_level(); // settle to sustain
        }
        assert!((env.level() - 1.0).abs() < 1e-6);
        env.release();
        let mut prev = env.level();
        let mut finished = false;
        for _ in 0..20_000 {
            let l = env.next_level();
            assert!(l <= prev + 1e-6, "release must not rise: {l} > {prev}");
            prev = l;
            if env.is_done() {
                finished = true;
                break;
            }
        }
        assert!(finished, "release must finish within budget");
        assert_eq!(env.level(), 0.0);
    }

    #[test]
    fn release_eases_from_current_level_no_jump() {
        // Long attack, released partway up: the exponential release starts from the
        // current level — a small step down, never a jump up toward sustain.
        let mut env = Adsr::new(1.0, 0.0, 1.0, 0.05, SR);
        for _ in 0..100 {
            env.next_level();
        }
        let before = env.level();
        assert!(before > 0.0 && before < 1.0, "should be mid-attack, got {before}");
        env.release();
        let after = env.next_level();
        assert!(after < before, "release steps down");
        assert!(after > before * 0.9, "but eases, no jump");
    }

    #[test]
    fn midi_to_freq_reference_pitches() {
        assert!((midi_to_freq(69.0) - 440.0).abs() < 1e-3); // A4
        assert!((midi_to_freq(60.0) - 261.625_58).abs() < 1e-2); // middle C
        // An octave up doubles the frequency.
        assert!((midi_to_freq(81.0) - 880.0).abs() < 1e-2);
    }

    #[test]
    fn oscillator_phase_wraps_and_stays_bounded() {
        let mut osc = Oscillator::new(Waveform::Saw, 1_000.0, SR);
        for _ in 0..SR as usize {
            let s = osc.next_sample();
            assert!((-1.0..=1.0).contains(&s), "sample out of range: {s}");
        }
    }
}
