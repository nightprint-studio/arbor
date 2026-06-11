//! Hand-rolled DSP building blocks: per-voice [`Biquad`] filters (`lpf`/`hpf`),
//! the [`shape`] waveshaper and [`crush`] bitcrusher, a stereo [`Reverb`] send
//! bus, and the master [`Limiter`].
//!
//! These are deliberately self-contained (no `fundsp` API coupling) so the DSP
//! is allocation-free, stable, and reviewable in isolation. The voice chain
//! (`crate::voice`) owns the per-voice filters; the renderer (`crate::renderer`)
//! owns the shared reverb bus and the master limiter.

use std::f32::consts::PI;

/// A transposed-direct-form-II biquad — the workhorse for `lpf` / `hpf`.
///
/// Coefficients are recomputed only when the cutoff changes (the renderer keeps
/// cutoff constant per voice for a block, so this is once-per-voice-per-trigger).
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// An identity (pass-through) biquad.
    pub fn identity() -> Self {
        Biquad {
            b0: 1.0,
            ..Default::default()
        }
    }

    /// Configure as a 2nd-order low-pass (RBJ cookbook), Q = 0.707.
    pub fn lowpass(cutoff: f32, sample_rate: f32) -> Self {
        let mut q = Self::default();
        q.set_lowpass(cutoff, sample_rate);
        q
    }

    /// Configure as a 2nd-order high-pass (RBJ cookbook), Q = 0.707.
    pub fn highpass(cutoff: f32, sample_rate: f32) -> Self {
        let mut q = Self::default();
        q.set_highpass(cutoff, sample_rate);
        q
    }

    /// Recompute as low-pass, preserving the filter state (z1/z2).
    pub fn set_lowpass(&mut self, cutoff: f32, sample_rate: f32) {
        let (cos, alpha) = rbj_terms(cutoff, sample_rate);
        let b1 = 1.0 - cos;
        let b0 = b1 / 2.0;
        self.set_coeffs(b0, b1, b0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha);
    }

    /// Recompute as high-pass, preserving the filter state.
    pub fn set_highpass(&mut self, cutoff: f32, sample_rate: f32) {
        let (cos, alpha) = rbj_terms(cutoff, sample_rate);
        let b0 = (1.0 + cos) / 2.0;
        let b1 = -(1.0 + cos);
        self.set_coeffs(b0, b1, b0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha);
    }

    fn set_coeffs(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// Process one sample.
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Shared RBJ intermediate terms (`cos w0`, `alpha`) for a Q = 0.707 filter.
fn rbj_terms(cutoff: f32, sample_rate: f32) -> (f32, f32) {
    // Clamp the cutoff below Nyquist to keep the bilinear transform stable.
    let nyq = sample_rate * 0.5;
    let fc = cutoff.clamp(10.0, nyq * 0.99);
    let w0 = 2.0 * PI * fc / sample_rate;
    let cos = w0.cos();
    let sin = w0.sin();
    // Q = 1/sqrt(2) → Butterworth (maximally flat). alpha = sin(w0) / (2·Q).
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let alpha = sin / (2.0 * q);
    (cos, alpha)
}

/// A `tanh` soft-clip waveshaper. `amount ∈ 0..1` maps to a drive factor; `0`
/// is a no-op. Output is normalised so the curve stays roughly unity at low
/// signal levels.
pub fn shape(x: f32, amount: f32) -> f32 {
    if amount <= 0.0 {
        return x;
    }
    let a = amount.clamp(0.0, 1.0);
    // Drive from 1× (no shaping) up to ~25× at full amount.
    let drive = 1.0 + a * 24.0;
    (x * drive).tanh() / drive.tanh()
}

/// Bitcrush to `bits` of resolution by quantising the amplitude. `bits` is
/// clamped to a sane `1..16` range; higher is effectively transparent.
pub fn crush(x: f32, bits: f32) -> f32 {
    let b = bits.clamp(1.0, 16.0);
    let levels = 2.0_f32.powf(b);
    let half = levels / 2.0;
    // Map [-1,1] → integer steps → back.
    (x * half).round() / half
}

/// A stereo Schroeder-style reverb (4 combs + 2 all-passes per channel), used as
/// the shared `room` send bus. Pre-sized at construction; `process` is
/// allocation-free. A fixed, pleasant medium room — `room` is a *send amount*,
/// so per-voice variation is handled by how much dry signal is fed in.
pub struct Reverb {
    combs_l: [Comb; 4],
    combs_r: [Comb; 4],
    allpass_l: [Allpass; 2],
    allpass_r: [Allpass; 2],
}

impl std::fmt::Debug for Reverb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reverb").finish_non_exhaustive()
    }
}

impl Reverb {
    /// Build the reverb network for `sample_rate`. Delay lengths are the classic
    /// Freeverb tunings, scaled from their 44.1 kHz reference.
    pub fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 44_100.0;
        let comb_tunings = [1116, 1188, 1277, 1356];
        let stereo_spread = 23;
        let allpass_tunings = [556, 441];
        let feedback = 0.84;
        let damp = 0.2;

        let mk_comb = |len: usize| Comb::new((len as f32 * scale) as usize, feedback, damp);
        let mk_ap = |len: usize| Allpass::new((len as f32 * scale) as usize, 0.5);

        Reverb {
            combs_l: comb_tunings.map(mk_comb),
            combs_r: comb_tunings.map(|t| mk_comb(t + stereo_spread)),
            allpass_l: allpass_tunings.map(mk_ap),
            allpass_r: allpass_tunings.map(|t| mk_ap(t + stereo_spread)),
        }
    }

    /// Process one stereo frame of the wet bus. Input is the summed send signal.
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut l = 0.0;
        let mut r = 0.0;
        for c in &mut self.combs_l {
            l += c.process(input[0]);
        }
        for c in &mut self.combs_r {
            r += c.process(input[1]);
        }
        for ap in &mut self.allpass_l {
            l = ap.process(l);
        }
        for ap in &mut self.allpass_r {
            r = ap.process(r);
        }
        // Comb sum scaling (4 combs → ~0.25) plus a little headroom.
        [l * 0.22, r * 0.22]
    }
}

/// A lowpass-damped feedback comb filter (one Freeverb comb).
#[derive(Clone, Debug)]
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
    damp: f32,
    filter_store: f32,
}

impl Comb {
    fn new(len: usize, feedback: f32, damp: f32) -> Self {
        Comb {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            feedback,
            damp,
            filter_store: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let out = self.buf[self.idx];
        // One-pole lowpass in the feedback path (damping).
        self.filter_store = out * (1.0 - self.damp) + self.filter_store * self.damp;
        self.buf[self.idx] = input + self.filter_store * self.feedback;
        self.idx = (self.idx + 1) % self.buf.len();
        out
    }
}

/// A Schroeder all-pass filter (diffusion).
#[derive(Clone, Debug)]
struct Allpass {
    buf: Vec<f32>,
    idx: usize,
    gain: f32,
}

impl Allpass {
    fn new(len: usize, gain: f32) -> Self {
        Allpass {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            gain,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buf[self.idx];
        let out = -input + buffered;
        self.buf[self.idx] = input + buffered * self.gain;
        self.idx = (self.idx + 1) % self.buf.len();
        out
    }
}

/// A simple look-back-free peak limiter with smooth gain release. Keeps the
/// master bus under `ceiling` without the hard-clip "crunch": when a peak
/// exceeds the ceiling the gain drops instantly, then recovers exponentially.
#[derive(Clone, Copy, Debug)]
pub struct Limiter {
    ceiling: f32,
    /// Current gain reduction multiplier `0..1` (1 = no reduction).
    gain: f32,
    /// Per-sample recovery coefficient toward unity.
    release_coeff: f32,
}

impl Limiter {
    /// Build a limiter at `ceiling` (linear, e.g. `0.95`) with a release time in
    /// seconds for `sample_rate`.
    pub fn new(ceiling: f32, release_secs: f32, sample_rate: f32) -> Self {
        let n = (release_secs * sample_rate).max(1.0);
        Limiter {
            ceiling,
            gain: 1.0,
            // Exponential approach: reach ~63% of the way to unity in `n` samples.
            release_coeff: (-1.0 / n).exp(),
        }
    }

    /// Process one stereo frame, applying smoothed gain reduction.
    pub fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let peak = frame[0].abs().max(frame[1].abs());
        let target = if peak * self.gain > self.ceiling {
            self.ceiling / peak
        } else {
            1.0
        };
        if target < self.gain {
            // Attack is instantaneous (clamp this sample's peak immediately).
            self.gain = target;
        } else {
            // Release: ease back toward unity.
            self.gain = target + (self.gain - target) * self.release_coeff;
        }
        [frame[0] * self.gain, frame[1] * self.gain]
    }
}
