//! The default synth voice: a band-unlimited oscillator + an [`Adsr`] envelope.
//!
//! This is merula's **fallback sound** — what a `Named` source plays when the
//! registry can't resolve it to an SFZ instrument, and the built-in
//! `synth.*` presets (`synth.bass`, `synth.pad`, `synth.pluck`). It is pitched
//! by the voice's `note`; unpitched triggers fall back to a fixed reference.
//!
//! Everything here is allocation-free once constructed: an oscillator is a phase
//! accumulator, the envelope four line segments. The RT renderer ticks them one
//! sample at a time inside the voice DSP chain (`crate::voice`).

/// The classic four oscillator shapes. Saw/square are **band-limited** via
/// PolyBLEP (their hard discontinuities are the alias source); sine and triangle
/// are continuous and alias-free by construction.
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
    /// Evaluate the waveform at phase `p ∈ [0, 1)`, output in ~`[-1, 1]`.
    ///
    /// `dt` is the phase increment per sample (`freq / sample_rate`); it scales
    /// the PolyBLEP correction band that rounds the saw/square discontinuities so
    /// they don't fold high harmonics back as aliasing. A naive saw/square sounds
    /// clean in isolation but turns to harsh "digital interference" across the
    /// register, especially in the bass — this is the fix. Sine/triangle ignore
    /// `dt` (no discontinuity to correct).
    fn sample(self, p: f32, dt: f32) -> f32 {
        match self {
            // Naive ramp minus the BLEP residual at the wrap discontinuity.
            Waveform::Saw => (2.0 * p - 1.0) - poly_blep(p, dt),
            Waveform::Square => {
                let naive = if p < 0.5 { 1.0 } else { -1.0 };
                // Correct both edges: the rising one at p≈0, the falling one at
                // p≈0.5 (evaluated at the phase shifted half a cycle).
                let half = if p < 0.5 { p + 0.5 } else { p - 0.5 };
                naive + poly_blep(p, dt) - poly_blep(half, dt)
            }
            Waveform::Sine => (p * std::f32::consts::TAU).sin(),
            Waveform::Triangle => 4.0 * (p - 0.5).abs() - 1.0,
        }
    }
}

/// PolyBLEP (polynomial band-limited step) residual at phase `t ∈ [0, 1)` for a
/// discontinuity, scaled to the phase increment `dt`. Returns the correction to
/// add/subtract around a step edge so it is band-limited instead of instantaneous
/// (the classic 2-sample fit). Zero outside the `dt`-wide window around the wrap.
fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    if t < dt {
        // Just after the edge: rising half of the fit.
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        // Just before the wrap: falling half of the fit.
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// The colour of a noise generator — the spectral tilt that distinguishes the
/// classic noise sources. All are pitch-independent (a noise voice ignores its
/// note); they're shaped by the voice's filters + envelope like any other tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseColor {
    /// Flat spectrum — the raw random source. Bright, hissy (cymbals, FX).
    White,
    /// −3 dB/oct tilt — equal energy per octave. Natural, "full" hiss.
    Pink,
    /// −6 dB/oct tilt — bass-heavy, dark (wind, rumble).
    Brown,
    /// Sparse random impulses — vinyl-style crackle / dust.
    Crackle,
}

/// What a synth voice actually generates. Broader than a single [`Waveform`]:
/// merula's built-in palette also exposes a detuned-saw **supersaw** and the
/// **noise** colours, so a `synth.*` preset (or a bare Strudel-style name like
/// `supersaw` / `white`) selects one of these uniformly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SynthShape {
    /// A single band-limited oscillator of the given shape.
    Wave(Waveform),
    /// A stack of slightly-detuned saws — wide, lush (trance leads / pads).
    Supersaw,
    /// A noise generator of the given colour (pitch-independent).
    Noise(NoiseColor),
}

impl SynthShape {
    /// A human label for the sound-bank UI / diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            SynthShape::Wave(Waveform::Saw) => "sawtooth",
            SynthShape::Wave(Waveform::Square) => "square",
            SynthShape::Wave(Waveform::Sine) => "sine",
            SynthShape::Wave(Waveform::Triangle) => "triangle",
            SynthShape::Supersaw => "supersaw",
            SynthShape::Noise(NoiseColor::White) => "white noise",
            SynthShape::Noise(NoiseColor::Pink) => "pink noise",
            SynthShape::Noise(NoiseColor::Brown) => "brown noise",
            SynthShape::Noise(NoiseColor::Crackle) => "crackle",
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
        Oscillator::with_phase(waveform, freq, sample_rate, 0.0)
    }

    /// Build an oscillator with an explicit initial phase `∈ [0, 1)`. Used by the
    /// supersaw to spread its detuned voices so they don't start phase-aligned
    /// (which would sum to a click and lose the wide, swirly character).
    pub fn with_phase(waveform: Waveform, freq: f32, sample_rate: f32, phase: f32) -> Self {
        Oscillator {
            waveform,
            phase: phase - phase.floor(),
            step: freq / sample_rate,
        }
    }

    /// Produce the next (band-limited) sample and advance the phase.
    pub fn next_sample(&mut self) -> f32 {
        let s = self.waveform.sample(self.phase, self.step);
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        s
    }

    /// Retune to `freq` Hz **keeping the current phase** — a glide for monophonic
    /// legato, where re-pitching mid-note must not reset the phase (that would
    /// click).
    pub fn set_freq(&mut self, freq: f32, sample_rate: f32) {
        self.step = freq / sample_rate;
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

/// A bank of slightly-detuned saw oscillators summed to one wide voice — the
/// classic "supersaw". Detuning is symmetric around the played pitch; the
/// voices start at spread phases so the onset is wide instead of a single click.
#[derive(Clone, Copy, Debug)]
pub struct SuperSaw {
    oscs: [Oscillator; SUPERSAW_VOICES],
    /// Output scale so the summed voices stay roughly within `[-1, 1]`.
    norm: f32,
}

/// How many detuned saws make up a supersaw. Odd so one voice sits dead-centre.
const SUPERSAW_VOICES: usize = 7;

/// Per-voice detune in cents, symmetric about 0 (the centre voice is in tune).
/// A moderate spread — wide enough to swirl, narrow enough to stay musical.
const SUPERSAW_DETUNE_CENTS: [f32; SUPERSAW_VOICES] =
    [-22.0, -14.0, -7.0, 0.0, 7.0, 14.0, 22.0];

impl SuperSaw {
    /// Build a supersaw centred on `freq` Hz.
    pub fn new(freq: f32, sample_rate: f32) -> Self {
        // Spread the start phases evenly so the stack opens wide, not in a spike.
        let oscs = std::array::from_fn(|i| {
            let cents = SUPERSAW_DETUNE_CENTS[i];
            let detuned = freq * 2.0_f32.powf(cents / 1200.0);
            let phase = i as f32 / SUPERSAW_VOICES as f32;
            Oscillator::with_phase(Waveform::Saw, detuned, sample_rate, phase)
        });
        SuperSaw {
            oscs,
            // Saws are uncorrelated once detuned, so energy grows ~√N; normalise
            // by that, with generous headroom so transient phase alignments stay
            // bounded (the raw √N-normalised sum can still spike well above 1).
            norm: 0.45 / (SUPERSAW_VOICES as f32).sqrt(),
        }
    }

    /// Sum the detuned voices for the next sample.
    pub fn next_sample(&mut self) -> f32 {
        let mut sum = 0.0;
        for osc in &mut self.oscs {
            sum += osc.next_sample();
        }
        sum * self.norm
    }

    /// Re-centre the detuned stack on `freq` Hz, each voice keeping its phase (a
    /// legato glide). Mirrors [`new`](SuperSaw::new)'s detune layout.
    pub fn set_freq(&mut self, freq: f32, sample_rate: f32) {
        for (i, osc) in self.oscs.iter_mut().enumerate() {
            let detuned = freq * 2.0_f32.powf(SUPERSAW_DETUNE_CENTS[i] / 1200.0);
            osc.set_freq(detuned, sample_rate);
        }
    }
}

/// A noise generator: a fast deterministic PRNG shaped to the requested
/// [`NoiseColor`]. Seeded per voice so repeated hits decorrelate (and offline
/// renders stay reproducible). Allocation-free; ticked one sample at a time.
#[derive(Clone, Copy, Debug)]
pub struct NoiseGen {
    color: NoiseColor,
    /// xorshift32 state (kept non-zero).
    rng: u32,
    /// Pink-noise filter memory (Paul Kellet's economy method).
    pink: [f32; 7],
    /// Brown-noise running integrator.
    brown: f32,
}

impl NoiseGen {
    /// Build a noise generator of `color`, seeded from the voice id.
    pub fn new(color: NoiseColor, seed: u64) -> Self {
        // Fold the 64-bit id into a non-zero 32-bit xorshift seed.
        let mixed = (seed ^ (seed >> 32)) as u32;
        NoiseGen {
            color,
            rng: mixed | 1,
            pink: [0.0; 7],
            brown: 0.0,
        }
    }

    /// Next xorshift32 word.
    fn next_u32(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    /// A flat-spectrum white sample in `[-1, 1)`.
    fn white(&mut self) -> f32 {
        (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Produce the next noise sample for this colour.
    pub fn next_sample(&mut self) -> f32 {
        match self.color {
            NoiseColor::White => self.white(),
            NoiseColor::Pink => {
                // Paul Kellet's economy pink filter: six one-pole sections plus a
                // delayed white term, summed and scaled back to ~unit range.
                let w = self.white();
                let p = &mut self.pink;
                p[0] = 0.99886 * p[0] + w * 0.0555179;
                p[1] = 0.99332 * p[1] + w * 0.0750759;
                p[2] = 0.96900 * p[2] + w * 0.1538520;
                p[3] = 0.86650 * p[3] + w * 0.3104856;
                p[4] = 0.55000 * p[4] + w * 0.5329522;
                p[5] = -0.7616 * p[5] - w * 0.0168980;
                let out = p[0] + p[1] + p[2] + p[3] + p[4] + p[5] + p[6] + w * 0.5362;
                p[6] = w * 0.115926;
                out * 0.11
            }
            NoiseColor::Brown => {
                // Leaky integrator: −6 dB/oct. The leak keeps it from wandering to
                // a DC rail; the gain brings the quiet result back to unit range.
                let w = self.white();
                self.brown = (self.brown + 0.02 * w).clamp(-1.0, 1.0);
                self.brown * 1.8
            }
            NoiseColor::Crackle => {
                // Sparse impulses: most samples are silent, an occasional spike is
                // the "crackle". A second draw sets the spike's height/sign.
                if self.next_u32() < (u32::MAX as f64 * CRACKLE_DENSITY) as u32 {
                    self.white()
                } else {
                    0.0
                }
            }
        }
    }
}

/// Fraction of samples that fire an impulse in [`NoiseColor::Crackle`].
const CRACKLE_DENSITY: f64 = 0.012;

/// The sound generator backing a [`SynthVoice`]: a single oscillator, a detuned
/// supersaw stack, or a noise source. All produce one mono sample per tick.
#[derive(Clone, Copy, Debug)]
enum Tone {
    Osc(Oscillator),
    Super(SuperSaw),
    Noise(NoiseGen),
}

impl Tone {
    fn next_sample(&mut self) -> f32 {
        match self {
            Tone::Osc(o) => o.next_sample(),
            Tone::Super(s) => s.next_sample(),
            Tone::Noise(n) => n.next_sample(),
        }
    }

    /// Retune a pitched tone in place (keeping phase) for a legato glide. Noise
    /// has no pitch, so it's a no-op there.
    fn set_freq(&mut self, freq: f32, sample_rate: f32) {
        match self {
            Tone::Osc(o) => o.set_freq(freq, sample_rate),
            Tone::Super(s) => s.set_freq(freq, sample_rate),
            Tone::Noise(_) => {}
        }
    }
}

/// A complete default-synth voice generator: a tone source + its own envelope.
///
/// Produced by [`crate::registry`] when resolving a synth preset (or as the
/// universal fallback). The amplitude envelope here is the voice's dynamics
/// shape; the [`crate::voice`] DSP chain multiplies it by the post-`vel` gain.
#[derive(Clone, Copy, Debug)]
pub struct SynthVoice {
    tone: Tone,
    env: Adsr,
}

impl SynthVoice {
    /// Build a synth voice for `shape` playing `freq` Hz with the given ADSR.
    /// `seed` (the voice id) decorrelates noise voices; tonal shapes ignore it.
    pub fn new(shape: SynthShape, freq: f32, env: Adsr, sample_rate: f32, seed: u64) -> Self {
        let tone = match shape {
            SynthShape::Wave(w) => Tone::Osc(Oscillator::new(w, freq, sample_rate)),
            SynthShape::Supersaw => Tone::Super(SuperSaw::new(freq, sample_rate)),
            SynthShape::Noise(c) => Tone::Noise(NoiseGen::new(c, seed)),
        };
        SynthVoice { tone, env }
    }

    /// Begin the release phase of the amplitude envelope.
    pub fn release(&mut self) {
        self.env.release();
    }

    /// Retune the tone to `freq` Hz **without re-triggering the envelope** — the
    /// monophonic-legato glide. The oscillator keeps its phase and the [`Adsr`]
    /// keeps sustaining, so the pitch changes with no amplitude dip or click.
    pub fn set_pitch(&mut self, freq: f32, sample_rate: f32) {
        self.tone.set_freq(freq, sample_rate);
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

    /// Produce the next mono sample (tone × envelope).
    pub fn next_sample(&mut self) -> f32 {
        let s = self.tone.next_sample();
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
    fn poly_blep_zero_away_from_edges_active_near_them() {
        let dt = 0.01;
        // Mid-cycle: no discontinuity to correct.
        assert_eq!(poly_blep(0.5, dt), 0.0);
        assert_eq!(poly_blep(0.25, dt), 0.0);
        // Just after the wrap and just before it: a non-zero correction.
        assert!(poly_blep(0.0, dt) != 0.0);
        assert!(poly_blep(0.999, dt) != 0.0);
        // A degenerate increment is a no-op (avoids divide-by-zero).
        assert_eq!(poly_blep(0.0, 0.0), 0.0);
    }

    #[test]
    fn supersaw_sums_detuned_voices_and_stays_bounded() {
        // A second of supersaw at A4 must stay bounded (no runaway / NaN) and
        // actually move (the detuned stack is never silent once running).
        let mut s = SuperSaw::new(440.0, SR);
        let mut peak = 0.0_f32;
        for _ in 0..SR as usize {
            let v = s.next_sample();
            assert!(v.is_finite(), "supersaw produced a non-finite sample");
            peak = peak.max(v.abs());
        }
        assert!(peak > 0.1, "supersaw should be audible, peak was {peak}");
        assert!(peak <= 1.2, "supersaw should stay roughly bounded, peak was {peak}");
    }

    #[test]
    fn noise_colours_are_bounded_and_audible() {
        for color in [
            NoiseColor::White,
            NoiseColor::Pink,
            NoiseColor::Brown,
            NoiseColor::Crackle,
        ] {
            let mut n = NoiseGen::new(color, 0x1234_5678);
            let mut peak = 0.0_f32;
            let mut energy = 0.0_f64;
            for _ in 0..SR as usize {
                let v = n.next_sample();
                assert!(v.is_finite(), "{color:?} produced a non-finite sample");
                peak = peak.max(v.abs());
                energy += (v * v) as f64;
            }
            // Even sparse crackle fires often enough over a second to register.
            assert!(energy > 0.0, "{color:?} should produce signal");
            assert!(peak <= 2.0, "{color:?} should stay bounded, peak was {peak}");
        }
    }

    #[test]
    fn noise_seed_decorrelates_repeated_voices() {
        // Two different seeds (voice ids) must not produce the identical stream —
        // that's what keeps repeated hi-hat hits from sounding mechanically equal.
        let mut a = NoiseGen::new(NoiseColor::White, 1);
        let mut b = NoiseGen::new(NoiseColor::White, 2);
        let mut differing = 0;
        for _ in 0..256 {
            if a.next_sample() != b.next_sample() {
                differing += 1;
            }
        }
        assert!(differing > 200, "distinct seeds should diverge, differed {differing}/256");
    }

    #[test]
    fn oscillator_phase_wraps_and_stays_bounded() {
        // PolyBLEP rounds the saw edge; the corrected output can graze a hair past
        // ±1 right at the wrap (band-limited Gibbs), so allow a small margin while
        // still asserting it stays bounded (no runaway / NaN).
        let mut osc = Oscillator::new(Waveform::Saw, 1_000.0, SR);
        for _ in 0..SR as usize {
            let s = osc.next_sample();
            assert!((-1.05..=1.05).contains(&s), "sample out of range: {s}");
        }
    }
}
