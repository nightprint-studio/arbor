/**
 * Parametric-EQ magnitude response — the math behind the FX panel's EQ curve.
 *
 * Each band is a single RBJ-cookbook biquad (the same shapes the audio crate's
 * `EqChain` builds); the panel draws the *summed* dB response so a user reads the
 * combined effect of the whole chain. Pure + UI-only: this is for the display
 * curve, not for rendering audio (that's `arbor-nemus-audio/effects.rs`). Match the
 * renderer's coefficients so the picture tracks what's heard.
 */

import type { EqBandValue, EqKind } from '../editor/nemus-edit';

/** Display sample rate — only the *shape* over 20 Hz–20 kHz matters, and 48 kHz
 *  matches the renderer's default so the curve and the sound line up. */
const FS = 48_000;

/** Biquad transfer-function coefficients (`b` = numerator, `a` = denominator). */
interface Biquad {
  b0: number; b1: number; b2: number;
  a0: number; a1: number; a2: number;
}

/** RBJ-cookbook coefficients for one band (mirrors `effects.rs::Biquad`). */
function bandCoeffs(kind: EqKind, freq: number, gainDb: number, q: number): Biquad {
  const w0 = (2 * Math.PI * Math.min(Math.max(freq, 1), FS / 2)) / FS;
  const cw = Math.cos(w0);
  const sw = Math.sin(w0);
  const Q = Math.max(q, 0.01);
  const alpha = sw / (2 * Q);
  const A = Math.pow(10, gainDb / 40);

  switch (kind) {
    case 'peak':
      return {
        b0: 1 + alpha * A, b1: -2 * cw, b2: 1 - alpha * A,
        a0: 1 + alpha / A, a1: -2 * cw, a2: 1 - alpha / A,
      };
    case 'low': {
      const s = 2 * Math.sqrt(A) * alpha;
      return {
        b0: A * ((A + 1) - (A - 1) * cw + s),
        b1: 2 * A * ((A - 1) - (A + 1) * cw),
        b2: A * ((A + 1) - (A - 1) * cw - s),
        a0: (A + 1) + (A - 1) * cw + s,
        a1: -2 * ((A - 1) + (A + 1) * cw),
        a2: (A + 1) + (A - 1) * cw - s,
      };
    }
    case 'high': {
      const s = 2 * Math.sqrt(A) * alpha;
      return {
        b0: A * ((A + 1) + (A - 1) * cw + s),
        b1: -2 * A * ((A - 1) + (A + 1) * cw),
        b2: A * ((A + 1) + (A - 1) * cw - s),
        a0: (A + 1) - (A - 1) * cw + s,
        a1: 2 * ((A - 1) - (A + 1) * cw),
        a2: (A + 1) - (A - 1) * cw - s,
      };
    }
    case 'hpf':
      return {
        b0: (1 + cw) / 2, b1: -(1 + cw), b2: (1 + cw) / 2,
        a0: 1 + alpha, a1: -2 * cw, a2: 1 - alpha,
      };
    case 'lpf':
      return {
        b0: (1 - cw) / 2, b1: 1 - cw, b2: (1 - cw) / 2,
        a0: 1 + alpha, a1: -2 * cw, a2: 1 - alpha,
      };
  }
}

/** |H(e^jw)| in dB for one biquad at frequency `f`. */
function magnitudeDb(c: Biquad, f: number): number {
  const w = (2 * Math.PI * f) / FS;
  const cw = Math.cos(w), c2 = Math.cos(2 * w);
  const sw = Math.sin(w), s2 = Math.sin(2 * w);
  const nRe = c.b0 + c.b1 * cw + c.b2 * c2;
  const nIm = -(c.b1 * sw + c.b2 * s2);
  const dRe = c.a0 + c.a1 * cw + c.a2 * c2;
  const dIm = -(c.a1 * sw + c.a2 * s2);
  const num = Math.hypot(nRe, nIm);
  const den = Math.hypot(dRe, dIm) || 1e-9;
  return 20 * Math.log10((num / den) || 1e-9);
}

/** Summed dB response of `bands` (skipping calculated ones) at each freq in `freqs`. */
export function eqResponseDb(bands: EqBandValue[], freqs: number[]): number[] {
  const coeffs = bands
    .filter((b) => !b.calculated)
    .map((b) => bandCoeffs(b.kind, b.freq, b.gainDb, b.q));
  return freqs.map((f) => coeffs.reduce((sum, c) => sum + magnitudeDb(c, f), 0));
}

/** A log-spaced frequency axis from 20 Hz to 20 kHz (`n` points). */
export function logFreqAxis(n: number): number[] {
  const lo = Math.log10(20), hi = Math.log10(20_000);
  return Array.from({ length: n }, (_, i) => Math.pow(10, lo + ((hi - lo) * i) / (n - 1)));
}

/** Map a frequency (20 Hz–20 kHz) to a 0..1 x-position on the log axis. */
export function freqToX(f: number): number {
  const lo = Math.log10(20), hi = Math.log10(20_000);
  return (Math.log10(Math.min(Math.max(f, 20), 20_000)) - lo) / (hi - lo);
}

/** Inverse of {@link freqToX}: a 0..1 knob position → a frequency in Hz. */
export function xToFreq(x: number): number {
  const lo = Math.log10(20), hi = Math.log10(20_000);
  return Math.round(Math.pow(10, lo + (hi - lo) * Math.min(Math.max(x, 0), 1)));
}

/** A short Hz / kHz label for a frequency readout. */
export function freqLabel(f: number): string {
  return f >= 1000 ? `${(f / 1000).toFixed(f >= 10_000 ? 0 : 1)}k` : `${Math.round(f)}`;
}
