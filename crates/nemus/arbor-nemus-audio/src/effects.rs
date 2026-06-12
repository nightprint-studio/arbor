//! Hand-rolled DSP building blocks: per-voice [`Biquad`] filters (`lpf`/`hpf`
//! plus parametric-EQ bands), the [`shape`] waveshaper and [`crush`] bitcrusher,
//! a parametric [`EqChain`], a feed-forward [`Compressor`], a [`ConvReverb`]
//! convolution send bus, a per-track [`DelayLine`], and the master [`Limiter`].
//!
//! These are deliberately self-contained (no `fundsp` API coupling) so the DSP
//! is allocation-free, stable, and reviewable in isolation. The voice chain
//! (`crate::voice`) owns the per-voice filters; the renderer (`crate::renderer`)
//! owns the strip EQ/comp/delay inserts, the shared convolution reverb bus, and
//! the master strip + limiter.

use std::f32::consts::PI;

use crate::seam::{CompSettings, EqBand, EqBandKind, Frame};

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

    /// Build a parametric-EQ section from one [`EqBand`] (RBJ cookbook). Peak /
    /// shelf bands honour `gain_db`; hpf/lpf bands ignore it. `q` sets the
    /// bandwidth (peak) / slope (shelf) / resonance (hpf/lpf).
    pub fn eq_band(band: &EqBand, sample_rate: f32) -> Self {
        let mut q = Self::default();
        q.set_eq_band(band, sample_rate);
        q
    }

    /// Recompute this biquad as the given EQ band, preserving filter state.
    pub fn set_eq_band(&mut self, band: &EqBand, sample_rate: f32) {
        let nyq = sample_rate * 0.5;
        let fc = band.freq.clamp(10.0, nyq * 0.99);
        let qf = band.q.max(0.05);
        let w0 = 2.0 * PI * fc / sample_rate;
        let cos = w0.cos();
        let sin = w0.sin();
        let alpha = sin / (2.0 * qf);
        match band.kind {
            EqBandKind::Hpf => {
                let b0 = (1.0 + cos) / 2.0;
                let b1 = -(1.0 + cos);
                self.set_coeffs(b0, b1, b0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha);
            }
            EqBandKind::Lpf => {
                let b1 = 1.0 - cos;
                let b0 = b1 / 2.0;
                self.set_coeffs(b0, b1, b0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha);
            }
            EqBandKind::Peak => {
                let a = 10.0_f32.powf(band.gain_db / 40.0);
                self.set_coeffs(
                    1.0 + alpha * a,
                    -2.0 * cos,
                    1.0 - alpha * a,
                    1.0 + alpha / a,
                    -2.0 * cos,
                    1.0 - alpha / a,
                );
            }
            EqBandKind::LowShelf => {
                let a = 10.0_f32.powf(band.gain_db / 40.0);
                let beta = 2.0 * a.sqrt() * alpha;
                self.set_coeffs(
                    a * ((a + 1.0) - (a - 1.0) * cos + beta),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - beta),
                    (a + 1.0) + (a - 1.0) * cos + beta,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - beta,
                );
            }
            EqBandKind::HighShelf => {
                let a = 10.0_f32.powf(band.gain_db / 40.0);
                let beta = 2.0 * a.sqrt() * alpha;
                self.set_coeffs(
                    a * ((a + 1.0) + (a - 1.0) * cos + beta),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - beta),
                    (a + 1.0) - (a - 1.0) * cos + beta,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - beta,
                );
            }
        }
    }

    fn set_coeffs(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// Clear the filter delay line (z-state), keeping the coefficients.
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
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

/// Equal-power pan law: `pan ∈ [0,1]` → (left, right) gains. At centre both
/// gains are `√½ ≈ 0.707`, so the summed power is constant across the sweep.
/// Used both for per-voice pan (`crate::voice`) and per-strip balance
/// (`crate::renderer`).
pub(crate) fn equal_power_pan(pan: f32) -> (f32, f32) {
    let p = pan.clamp(0.0, 1.0);
    let angle = p * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

/// A **look-ahead** peak limiter keeping the master under `ceiling`.
///
/// The naive version reduced gain *instantaneously* at each over-ceiling sample.
/// Under a hot, dense mix (many summed voices + a reverb send) the peak envelope
/// is jagged, so that per-sample gain stepping modulates the signal at audio rate
/// — audible as distortion, worst exactly where a `room` send adds decorrelated
/// energy. Instead we delay the signal by a short look-ahead and **ramp** the gain
/// smoothly toward the reduction the incoming peak needs, reaching it by the time
/// that peak emerges. Smooth gain → no audio-rate modulation → clean limiting even
/// when slammed. A peak-hold over the look-ahead keeps a lone spike ducked until
/// it passes through.
#[derive(Clone, Debug)]
pub struct Limiter {
    ceiling: f32,
    /// Current gain-reduction multiplier `0..1` (1 = no reduction).
    gain: f32,
    /// Per-sample smoothing toward a deeper reduction (attack, over the look-ahead).
    attack_coeff: f32,
    /// Per-sample recovery toward unity (release).
    release_coeff: f32,
    /// Look-ahead delay ring (the output is this many samples behind the detector).
    delay: Vec<[f32; 2]>,
    pos: usize,
    /// Peak held across the look-ahead window so a spike stays ducked until output.
    held_peak: f32,
    hold: usize,
}

impl Limiter {
    /// Build a limiter at `ceiling` (linear, e.g. `0.95`) with a release time in
    /// seconds for `sample_rate`.
    pub fn new(ceiling: f32, release_secs: f32, sample_rate: f32) -> Self {
        // ~1.5 ms look-ahead — enough to ramp gain down before a peak reaches the
        // output, imperceptible as latency.
        let look = ((sample_rate * 0.0015) as usize).clamp(16, 1024);
        let rel = (release_secs * sample_rate).max(1.0);
        Limiter {
            ceiling,
            gain: 1.0,
            // Reach ~98% of the reduction across the look-ahead window (smooth, but
            // settled before the peak emerges, so overshoot stays well under 1.0).
            attack_coeff: (-4.0 / look as f32).exp(),
            release_coeff: (-1.0 / rel).exp(),
            delay: vec![[0.0; 2]; look],
            pos: 0,
            held_peak: 0.0,
            hold: 0,
        }
    }

    /// Reset the gain-reduction state + look-ahead buffer (transport stop / panic).
    pub fn reset(&mut self) {
        self.gain = 1.0;
        self.delay.iter_mut().for_each(|s| *s = [0.0; 2]);
        self.pos = 0;
        self.held_peak = 0.0;
        self.hold = 0;
    }

    /// Process one stereo frame. `frame` is the (future) detector input; the
    /// returned frame is the look-ahead-delayed signal with the smoothed gain.
    pub fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        // Peak-hold the detector over the look-ahead so a spike stays ducked until
        // it reaches the output.
        let peak = frame[0].abs().max(frame[1].abs());
        if peak >= self.held_peak {
            self.held_peak = peak;
            self.hold = self.delay.len();
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.held_peak = peak;
        }
        let target = if self.held_peak > self.ceiling { self.ceiling / self.held_peak } else { 1.0 };
        // Smooth toward the target: attack when ducking deeper, release when easing.
        let coeff = if target < self.gain { self.attack_coeff } else { self.release_coeff };
        self.gain = target + (self.gain - target) * coeff;

        let out = self.delay[self.pos];
        self.delay[self.pos] = frame;
        self.pos += 1;
        if self.pos >= self.delay.len() {
            self.pos = 0;
        }
        [out[0] * self.gain, out[1] * self.gain]
    }
}

/// A stereo parametric-EQ chain: one biquad section per [`EqBand`], per channel.
/// Empty band list = bypass (pass-through). Re-built wholesale when the strip's
/// band list changes ([`AudioCommand::SetTrackEq`](crate::seam::AudioCommand)), so
/// per-sample [`process`](EqChain::process) is allocation-free.
#[derive(Clone, Debug, Default)]
pub struct EqChain {
    /// One biquad per band, per channel: `[left, right]`.
    sections: Vec<[Biquad; 2]>,
}

impl EqChain {
    /// Build the section chain for `bands` at `sample_rate`. An empty list is a
    /// pass-through EQ.
    pub fn new(bands: &[EqBand], sample_rate: f32) -> Self {
        let sections = bands
            .iter()
            .map(|b| {
                let s = Biquad::eq_band(b, sample_rate);
                [s, s]
            })
            .collect();
        EqChain { sections }
    }

    /// Whether the chain has any active band.
    pub fn is_active(&self) -> bool {
        !self.sections.is_empty()
    }

    /// Clear every section's filter state (transport stop / panic), keeping the
    /// band coefficients so the EQ stays configured.
    pub fn reset(&mut self) {
        for sec in &mut self.sections {
            sec[0].reset();
            sec[1].reset();
        }
    }

    /// Process one stereo frame through every section in series.
    pub fn process(&mut self, frame: Frame) -> Frame {
        let mut l = frame[0];
        let mut r = frame[1];
        for sec in &mut self.sections {
            l = sec[0].process(l);
            r = sec[1].process(r);
        }
        [l, r]
    }
}

/// A standard feed-forward peak compressor with soft knee, attack/release
/// smoothing and make-up gain. Operates on the stereo peak (linked channels) so
/// the stereo image is preserved. Allocation-free; coefficients recomputed only
/// when [`set`](Compressor::set) is called.
#[derive(Clone, Copy, Debug)]
pub struct Compressor {
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    makeup: f32,
    attack_coeff: f32,
    release_coeff: f32,
    /// Smoothed gain-reduction envelope in dB (≤ 0).
    envelope_db: f32,
}

impl Compressor {
    /// Build a compressor from settings at `sample_rate`.
    pub fn new(settings: &CompSettings, sample_rate: f32) -> Self {
        let mut c = Compressor {
            threshold_db: 0.0,
            ratio: 1.0,
            knee_db: 0.0,
            makeup: 1.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            envelope_db: 0.0,
        };
        c.set(settings, sample_rate);
        c
    }

    /// Recompute coefficients from new settings, keeping the running envelope.
    pub fn set(&mut self, s: &CompSettings, sample_rate: f32) {
        self.threshold_db = s.threshold_db;
        self.ratio = s.ratio.max(1.0);
        self.knee_db = s.knee_db.max(0.0);
        self.makeup = 10.0_f32.powf(s.makeup_db / 20.0);
        self.attack_coeff = time_to_coeff(s.attack, sample_rate);
        self.release_coeff = time_to_coeff(s.release, sample_rate);
    }

    /// Reset the gain-reduction envelope to unity (transport stop / panic).
    pub fn reset(&mut self) {
        self.envelope_db = 0.0;
    }

    /// Process one stereo frame, applying smoothed gain reduction + make-up.
    pub fn process(&mut self, frame: Frame) -> Frame {
        let peak = frame[0].abs().max(frame[1].abs());
        // Level in dBFS; a tiny floor avoids log(0).
        let level_db = 20.0 * peak.max(1.0e-9).log10();
        let target_reduction = self.target_reduction_db(level_db);

        // Smooth toward the target: faster coefficient when gain reduction is
        // *increasing* (attack), slower when recovering (release).
        let coeff = if target_reduction < self.envelope_db {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope_db = target_reduction + (self.envelope_db - target_reduction) * coeff;

        let gain = 10.0_f32.powf(self.envelope_db / 20.0) * self.makeup;
        [frame[0] * gain, frame[1] * gain]
    }

    /// Static gain reduction (dB, ≤ 0) for an input `level_db`, with a soft knee.
    ///
    /// Canonical soft-knee compressor curve (Reiss/RBJ): output level `y` for
    /// input `x`, then reduction = `y - x`.
    fn target_reduction_db(&self, level_db: f32) -> f32 {
        let x = level_db;
        let t = self.threshold_db;
        let w = self.knee_db;
        let slope = 1.0 / self.ratio;
        let y = if w > 0.0 && (2.0 * (x - t)) > -w && (2.0 * (x - t)) <= w {
            // Knee region: quadratic blend from 1:1 to the ratio slope.
            x + (slope - 1.0) * (x - t + w * 0.5).powi(2) / (2.0 * w)
        } else if 2.0 * (x - t) > w {
            // Above the knee: full ratio.
            t + (x - t) * slope
        } else {
            // Below the knee: no compression.
            x
        };
        y - x
    }
}

/// One-pole smoothing coefficient for a `time`-second attack/release at
/// `sample_rate` (`coeff^n ≈ 1/e` after `time`). `0` time → instantaneous.
fn time_to_coeff(time: f32, sample_rate: f32) -> f32 {
    if time <= 0.0 {
        return 0.0;
    }
    (-1.0 / (time * sample_rate)).exp()
}

/// Hard cap on convolution IR length (frames). Naive time-domain convolution is
/// O(IR) per sample; a longer tail needs partitioned-FFT convolution (Onda 3).
/// An installed IR longer than this is truncated. ~8192 frames ≈ 170 ms @ 48 kHz.
const MAX_IR_FRAMES: usize = 8192;

/// Target L2 energy the procedural reverb IR is normalised to, so the `room` wet
/// send is a controlled, gentle level **independent of the tail length**. Picked
/// for a subtle-but-present room; raise for a wetter default.
const TARGET_IR_ENERGY: f32 = 0.3;

/// A stereo convolution reverb over a (typically procedural) impulse response.
///
/// Time-domain FIR convolution per channel via a ring-buffered history. This is
/// O(IR length) per sample — fine for the short, decimated procedural IRs nemus
/// uses as the `room` send target (capped at [`MAX_IR_FRAMES`]). The IR is
/// generated/installed off the RT path ([`ConvReverb::procedural`] /
/// [`ConvReverb::from_buffer`]); `process` only reads it.
pub struct ConvReverb {
    /// Impulse response, one `[l, r]` tap per frame.
    ir: Vec<Frame>,
    /// Ring of recent input frames, length = IR length.
    history: Vec<Frame>,
    /// Write cursor into `history`.
    pos: usize,
    /// Consecutive silent input frames processed. Once it reaches the IR length
    /// the `history` ring is fully flushed (all zeros), so the wet output is
    /// *exactly* zero and [`process`](Self::process) can skip the O(IR) convolution
    /// until non-silent input returns. The `room` send is exactly `0.0` whenever no
    /// voice feeds the bus (the common case — no `.room()`), so this removes a large
    /// constant per-frame cost with no change to the output.
    silent_for: usize,
}

impl std::fmt::Debug for ConvReverb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConvReverb")
            .field("ir_len", &self.ir.len())
            .finish_non_exhaustive()
    }
}

impl ConvReverb {
    /// Build from an explicit stereo IR. An empty IR falls back to a single unit
    /// tap (dry-through) so the bus never divides by zero; an over-long IR is
    /// truncated to [`MAX_IR_FRAMES`] (a full reverb tail is an Onda 3 FFT path).
    pub fn from_buffer(mut ir: Vec<Frame>) -> Self {
        if ir.is_empty() {
            ir = vec![[1.0, 1.0]];
        }
        ir.truncate(MAX_IR_FRAMES);
        let len = ir.len();
        ConvReverb {
            ir,
            history: vec![[0.0; 2]; len],
            pos: 0,
            // History starts all-zero, so the bus is gated until input arrives.
            silent_for: len,
        }
    }

    /// Synthesise a default procedural IR: an exponentially-decaying, slightly
    /// decorrelated stereo noise tail of `seconds` length, with a few early
    /// reflections. Deterministic (a fixed LCG seed) so renders are reproducible.
    pub fn procedural(seconds: f32, sample_rate: f32) -> Self {
        ConvReverb::from_buffer(procedural_ir(seconds, sample_rate))
    }

    /// Number of IR taps.
    pub fn len(&self) -> usize {
        self.ir.len()
    }

    /// Flush the convolution tail: clear the input history and re-arm the silence
    /// gate so the wet output is immediately, exactly zero. Used on a transport
    /// stop / panic so a `room` tail can't ring on (and keep the bus busy) after
    /// playback has stopped.
    pub fn reset(&mut self) {
        for h in &mut self.history {
            *h = [0.0; 2];
        }
        self.pos = 0;
        self.silent_for = self.ir.len();
    }

    /// Whether the IR is empty (never, after construction).
    pub fn is_empty(&self) -> bool {
        self.ir.is_empty()
    }

    /// Convolve one stereo input frame against the IR, returning the wet output.
    ///
    /// Gated: once the input has been silent for a full IR length the ring is all
    /// zeros and the wet output is exactly zero, so the convolution (the dominant
    /// per-frame cost) is skipped until non-silent input returns — bit-identical to
    /// running it, just without the wasted work when the `room` bus is quiet.
    pub fn process(&mut self, input: Frame) -> Frame {
        let len = self.ir.len();
        if input[0] == 0.0 && input[1] == 0.0 {
            if self.silent_for >= len {
                return [0.0, 0.0];
            }
            self.silent_for += 1;
        } else {
            self.silent_for = 0;
        }
        // Store the newest input at the cursor.
        self.history[self.pos] = input;

        let mut acc = [0.0f32; 2];
        // Walk the IR; tap `k` multiplies the input from `k` frames ago.
        let mut h = self.pos;
        for tap in &self.ir {
            let x = self.history[h];
            acc[0] += x[0] * tap[0];
            acc[1] += x[1] * tap[1];
            // Step backwards through the ring.
            h = if h == 0 { len - 1 } else { h - 1 };
        }

        self.pos = (self.pos + 1) % len;
        acc
    }
}

/// Build a procedural reverb IR: a handful of early reflections plus an
/// exponentially-decaying, channel-decorrelated noise tail. `seconds` caps the
/// tail; the result is normalised to a gentle send level.
fn procedural_ir(seconds: f32, sample_rate: f32) -> Vec<Frame> {
    let len = ((seconds.max(0.05) * sample_rate) as usize)
        .max(1)
        .min(MAX_IR_FRAMES);
    let mut ir = vec![[0.0f32; 2]; len];

    // Deterministic LCG for the diffuse tail (reproducible renders).
    let mut state_l: u32 = 0x1234_5678;
    let mut state_r: u32 = 0x9E37_79B9;
    let mut next = |s: &mut u32| -> f32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*s >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    };

    // Exponential decay so the tail reaches ~-60 dB at `len`.
    let decay = (1.0e-3_f32).powf(1.0 / len as f32);
    let mut env = 1.0f32;
    for tap in ir.iter_mut() {
        tap[0] = next(&mut state_l) * env;
        tap[1] = next(&mut state_r) * env;
        env *= decay;
    }

    // A few early reflections for a sense of space (delays in ms → frames).
    for &(ms, g) in &[(7.0, 0.6), (11.0, 0.5), (17.0, 0.42), (23.0, 0.35)] {
        let i = ((ms / 1000.0) * sample_rate) as usize;
        if i < len {
            ir[i][0] += g;
            ir[i][1] += g * 0.9;
        }
    }
    // Direct early tap so the wet onset isn't pure noise.
    ir[0][0] += 0.5;
    ir[0][1] += 0.5;

    // Normalise the IR to a fixed total energy so the wet send level is sane and
    // genuinely independent of the tail length. The previous form multiplied the
    // `0.06 / sqrt(len)` factor straight back by `sqrt(len)`, cancelling the length
    // normalisation and leaving the raw energy — which grows with the tail, so a
    // `room`-heavy passage overdrove the master into distortion.
    let energy = ir
        .iter()
        .map(|t| t[0] * t[0] + t[1] * t[1])
        .sum::<f32>()
        .sqrt();
    if energy > 0.0 {
        let g = TARGET_IR_ENERGY / energy;
        for tap in ir.iter_mut() {
            tap[0] *= g;
            tap[1] *= g;
        }
    }
    ir
}

// ── Algorithmic reverb (Freeverb topology) ─────────────────────────────────────
//
// The DEFAULT `room` reverb. Unlike [`ConvReverb`] (kept for an explicitly
// installed impulse response) this is **O(1) per output sample** — a fixed bank
// of comb + allpass delay lines — so the per-frame cost is constant regardless of
// the tail length. The convolution's cost is O(IR taps) per sample, which (gated
// on by any `.room()` send) overran the audio callback on `room`-heavy material
// and was heard as distortion. Feedback stays < 1, so the tail is always bounded.

/// Comb-filter delays (samples @ 44.1 kHz), scaled to the actual rate at build.
const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
/// Allpass diffuser delays (samples @ 44.1 kHz).
const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];
/// Right-channel delay offset for a stereo image.
const STEREO_SPREAD: usize = 23;
/// Input attenuation feeding the comb bank (Freeverb's fixed gain).
const REVERB_FIXED_GAIN: f32 = 0.015;

/// A damped feedback comb filter — one decaying reverb "mode".
struct Comb {
    buf: Vec<f32>,
    pos: usize,
    /// One-pole lowpass state for high-frequency damping in the feedback path.
    store: f32,
    feedback: f32,
    damp1: f32,
    damp2: f32,
}

impl Comb {
    fn new(len: usize, feedback: f32, damp: f32) -> Self {
        Comb { buf: vec![0.0; len.max(1)], pos: 0, store: 0.0, feedback, damp1: damp, damp2: 1.0 - damp }
    }
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let out = self.buf[self.pos];
        self.store = out * self.damp2 + self.store * self.damp1;
        self.buf[self.pos] = input + self.store * self.feedback;
        self.pos += 1;
        if self.pos >= self.buf.len() {
            self.pos = 0;
        }
        out
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.store = 0.0;
        self.pos = 0;
    }
}

/// A Schroeder allpass filter — a diffuser that thickens the comb output.
struct Allpass {
    buf: Vec<f32>,
    pos: usize,
    feedback: f32,
}

impl Allpass {
    fn new(len: usize, feedback: f32) -> Self {
        Allpass { buf: vec![0.0; len.max(1)], pos: 0, feedback }
    }
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buf[self.pos];
        let out = -input + buffered;
        self.buf[self.pos] = input + buffered * self.feedback;
        self.pos += 1;
        if self.pos >= self.buf.len() {
            self.pos = 0;
        }
        out
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.pos = 0;
    }
}

/// A compact stereo algorithmic reverb (Freeverb topology): eight damped feedback
/// combs in parallel into four series allpasses, per channel. O(1) per sample.
pub struct Freeverb {
    combs_l: Vec<Comb>,
    combs_r: Vec<Comb>,
    allp_l: Vec<Allpass>,
    allp_r: Vec<Allpass>,
    /// Output (wet) scaling — the bus is a pure send, so this sets the overall
    /// `room` level a `.room(x)` send then scales further.
    wet: f32,
}

impl Freeverb {
    /// Build for `sample_rate`. `room_hint` (0..1, the repurposed reverb "length")
    /// maps to the comb feedback — larger = a longer, more diffuse tail.
    pub fn new(room_hint: f32, sample_rate: f32) -> Self {
        let feedback = room_hint.clamp(0.0, 1.0) * 0.28 + 0.7; // ~0.7..0.98, always < 1
        // Fairly strong HF damping: a darker tail (like Strudel's low-passed reverb)
        // sits behind the mix instead of adding bright, harsh, peak-jagged energy.
        let damp = 0.35;
        let scale = (sample_rate / 44_100.0).max(0.1);
        let s = |n: usize| (((n as f32) * scale) as usize).max(1);
        Freeverb {
            combs_l: COMB_TUNINGS.iter().map(|&t| Comb::new(s(t), feedback, damp)).collect(),
            combs_r: COMB_TUNINGS.iter().map(|&t| Comb::new(s(t + STEREO_SPREAD), feedback, damp)).collect(),
            allp_l: ALLPASS_TUNINGS.iter().map(|&t| Allpass::new(s(t), 0.5)).collect(),
            allp_r: ALLPASS_TUNINGS.iter().map(|&t| Allpass::new(s(t + STEREO_SPREAD), 0.5)).collect(),
            wet: 0.22,
        }
    }
    /// Process one stereo send frame, returning the wet reverb output.
    pub fn process(&mut self, input: Frame) -> Frame {
        let x = (input[0] + input[1]) * REVERB_FIXED_GAIN;
        let mut l = 0.0;
        for c in &mut self.combs_l {
            l += c.process(x);
        }
        for a in &mut self.allp_l {
            l = a.process(l);
        }
        let mut r = 0.0;
        for c in &mut self.combs_r {
            r += c.process(x);
        }
        for a in &mut self.allp_r {
            r = a.process(r);
        }
        [l * self.wet, r * self.wet]
    }
    /// Clear every delay line so the wet output is immediately silent.
    pub fn reset(&mut self) {
        self.combs_l.iter_mut().for_each(Comb::reset);
        self.combs_r.iter_mut().for_each(Comb::reset);
        self.allp_l.iter_mut().for_each(Allpass::reset);
        self.allp_r.iter_mut().for_each(Allpass::reset);
    }
}

/// The `room` reverb: the cheap O(1) algorithmic [`Freeverb`] by default, or a
/// [`ConvReverb`] when an explicit impulse response is installed. The renderer
/// holds one of these on the `room` bus and dispatches `process` / `reset`.
#[derive(Debug)]
pub enum Reverb {
    /// Default: the algorithmic reverb (constant per-sample cost).
    Algo(Freeverb),
    /// An installed impulse response, via time-domain convolution.
    Conv(ConvReverb),
}

impl Reverb {
    /// The default procedural reverb — now the O(1) algorithmic [`Freeverb`].
    pub fn procedural(room_hint: f32, sample_rate: f32) -> Self {
        Reverb::Algo(Freeverb::new(room_hint, sample_rate))
    }
    /// An installed impulse-response reverb (convolution).
    pub fn from_buffer(ir: Vec<Frame>) -> Self {
        Reverb::Conv(ConvReverb::from_buffer(ir))
    }
    /// Process one stereo send frame.
    pub fn process(&mut self, input: Frame) -> Frame {
        match self {
            Reverb::Algo(f) => f.process(input),
            Reverb::Conv(c) => c.process(input),
        }
    }
    /// Flush the reverb tail to immediate silence.
    pub fn reset(&mut self) {
        match self {
            Reverb::Algo(f) => f.reset(),
            Reverb::Conv(c) => c.reset(),
        }
    }
}

impl std::fmt::Debug for Freeverb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Freeverb").field("combs", &self.combs_l.len()).finish_non_exhaustive()
    }
}

/// A stereo feedback delay line (per-track delay bus). The send into the line is
/// summed each frame; the tap is read `time_frames` back and fed back at
/// `feedback`. Reconfiguring time/feedback is cheap and keeps the buffered tail.
#[derive(Clone, Debug)]
pub struct DelayLine {
    buf: Vec<Frame>,
    pos: usize,
    /// Read-back distance in frames (clamped to the buffer length).
    time_frames: usize,
    feedback: f32,
    /// Pending send accumulated this frame, added into the line on `process`.
    input: Frame,
}

impl DelayLine {
    /// Build a delay line sized for up to `max_frames` of delay.
    pub fn new(max_frames: usize) -> Self {
        let cap = max_frames.max(1);
        DelayLine {
            buf: vec![[0.0; 2]; cap],
            pos: 0,
            time_frames: 0,
            feedback: 0.0,
            input: [0.0; 2],
        }
    }

    /// Set the delay time (frames) and feedback `0..1`. Grows the buffer if the
    /// requested time exceeds the current capacity (non-RT; the engine sets this
    /// from a mixer command, not per sample).
    pub fn configure(&mut self, time_frames: u32, feedback: f32) {
        let t = time_frames as usize;
        if t >= self.buf.len() {
            self.buf.resize(t + 1, [0.0; 2]);
        }
        self.time_frames = t;
        self.feedback = feedback.clamp(0.0, 0.999);
    }

    /// Whether this line currently produces echoes.
    pub fn is_active(&self) -> bool {
        self.time_frames > 0
    }

    /// Flush the buffered tail and any pending send to silence, keeping the
    /// configured `time_frames`/`feedback`. A feedback line never decays to exact
    /// zero on its own, so a transport stop must clear it explicitly — otherwise
    /// the echo rings on (audibly, and as perpetual DSP load) after playback ends.
    pub fn reset(&mut self) {
        for f in &mut self.buf {
            *f = [0.0; 2];
        }
        self.pos = 0;
        self.input = [0.0; 2];
    }

    /// Accumulate a send into the line for the current frame.
    pub fn send(&mut self, amount: Frame) {
        self.input[0] += amount[0];
        self.input[1] += amount[1];
    }

    /// Advance one frame: read the delayed tap, write input + feedback, return the
    /// wet echo. Call once per output frame after all sends for that frame.
    pub fn process(&mut self) -> Frame {
        if self.time_frames == 0 {
            self.input = [0.0; 2];
            return [0.0; 2];
        }
        let len = self.buf.len();
        let read = (self.pos + len - self.time_frames % len) % len;
        let echo = self.buf[read];

        // Write the freshly-sent signal plus the fed-back echo at the head.
        self.buf[self.pos] = [
            self.input[0] + echo[0] * self.feedback,
            self.input[1] + echo[1] * self.feedback,
        ];
        self.input = [0.0; 2];
        self.pos = (self.pos + 1) % len;
        echo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The silence gate must not change the output: a reverb driven by a signal,
    /// then gated through silence, then re-excited, matches one that never gates
    /// (here: re-excitation after a full flush produces exactly `ir[0]`, and a
    /// fully-silent run stays at zero).
    #[test]
    fn reverb_gate_is_output_identical() {
        let mut rv = ConvReverb::from_buffer(vec![[0.5, 0.4], [0.3, 0.2], [0.1, 0.05]]);
        let len = rv.len();

        // Impulse → first tap is the dry-ish onset.
        let y0 = rv.process([1.0, 1.0]);
        assert!((y0[0] - 0.5).abs() < 1e-6 && (y0[1] - 0.4).abs() < 1e-6);

        // Run silence well past the IR length: the tail decays to exact zero.
        for _ in 0..(len * 3) {
            let y = rv.process([0.0, 0.0]);
            let _ = y;
        }
        assert_eq!(rv.process([0.0, 0.0]), [0.0, 0.0], "fully flushed bus is exactly zero");

        // Re-excite: with the ring flushed, the impulse again yields ir[0].
        let y = rv.process([1.0, 1.0]);
        assert!(
            (y[0] - 0.5).abs() < 1e-6 && (y[1] - 0.4).abs() < 1e-6,
            "re-excitation after a gated flush must match a fresh impulse",
        );
    }

    /// The procedural IR must be energy-normalised to `TARGET_IR_ENERGY` regardless
    /// of the requested tail length — the regression guard for the `room` bug where
    /// the length normalisation cancelled out and the reverb gain grew with the
    /// tail (overdriving the master into distortion).
    #[test]
    fn procedural_ir_energy_is_normalised_and_length_independent() {
        let energy = |secs: f32| {
            procedural_ir(secs, 48_000.0)
                .iter()
                .map(|t| t[0] * t[0] + t[1] * t[1])
                .sum::<f32>()
                .sqrt()
        };
        for secs in [0.05, 0.1, 0.5, 2.0] {
            let e = energy(secs);
            assert!(
                (e - TARGET_IR_ENERGY).abs() < 1e-3,
                "IR energy {e} for {secs}s should be ~{TARGET_IR_ENERGY}",
            );
        }
    }

    /// The look-ahead limiter must tame a hard overdrive down to (about) the
    /// ceiling, staying finite and not clipping past 1.0 — the property that makes
    /// a slammed master (dense mix + reverb send) clean instead of distorted.
    #[test]
    fn limiter_tames_overdrive_without_clipping() {
        let mut lim = Limiter::new(0.95, 0.05, 48_000.0);
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let y = lim.process([5.0, -5.0]);
            assert!(y[0].is_finite() && y[1].is_finite(), "limited output must be finite");
            // Skip the initial gain ramp; measure the settled region.
            if i > 4_000 {
                peak = peak.max(y[0].abs()).max(y[1].abs());
            }
        }
        assert!(peak <= 1.0, "limited output should stay <= 1.0 (peak {peak})");
        assert!(peak > 0.9, "and should reach the ceiling, not over-duck (peak {peak})");
    }

    /// The algorithmic reverb must stay bounded under a sustained input (feedback
    /// < 1, no blow-up) and go exactly silent after a reset — the property that
    /// makes it a safe, O(1) replacement for the convolution on the `room` bus.
    #[test]
    fn freeverb_is_bounded_and_reset_silences() {
        let mut rv = Freeverb::new(0.5, 48_000.0);
        let mut peak = 0.0f32;
        for _ in 0..48_000 {
            let y = rv.process([1.0, 1.0]);
            assert!(y[0].is_finite() && y[1].is_finite(), "wet must stay finite");
            peak = peak.max(y[0].abs()).max(y[1].abs());
        }
        assert!(peak < 4.0, "wet should stay bounded (peak {peak})");
        rv.reset();
        assert_eq!(rv.process([0.0, 0.0]), [0.0, 0.0], "a reset reverb is exactly silent");
    }

    /// A feedback delay line rings indefinitely; `reset` must silence it at once.
    /// This is the tail that, before the stop-flush fix, kept the renderer busy
    /// (and audible) after the transport stopped.
    #[test]
    fn delay_reset_silences_feedback_tail() {
        let mut dl = DelayLine::new(64);
        dl.configure(8, 0.9); // long feedback: this never decays to zero on its own

        // Excite the line and let the echo build up.
        dl.send([1.0, 1.0]);
        for _ in 0..40 {
            dl.process();
        }
        // Still ringing well after the input stopped.
        let ringing = dl.process();
        assert!(ringing[0].abs() > 0.0, "feedback line should still be ringing");

        // Reset: the buffered tail and the pending send vanish, so every following
        // frame is exact silence even though the line stays configured.
        dl.reset();
        assert!(dl.is_active(), "reset keeps the delay configured");
        for _ in 0..32 {
            assert_eq!(dl.process(), [0.0, 0.0], "a reset line must stay silent");
        }
    }
}
