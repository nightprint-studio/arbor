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

    /// Reset the gain-reduction state to unity (transport stop / panic).
    pub fn reset(&mut self) {
        self.gain = 1.0;
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

    // Normalise to keep the send level sane regardless of length.
    let norm = 0.06 / (len as f32).sqrt().max(1.0);
    for tap in ir.iter_mut() {
        tap[0] *= norm * (len as f32).sqrt();
        tap[1] *= norm * (len as f32).sqrt();
    }
    ir
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
