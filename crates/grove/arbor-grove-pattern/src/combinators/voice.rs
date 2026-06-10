//! Voice & mix transforms: they stamp controls onto each event. These are
//! specialised to `Pattern<ControlMap>` (the others stay generic).
//!
//! Numeric parameters accept a constant **or** a pattern of numbers (a [`Param`])
//! — `.gain(rand(0.0, 1.0))` varies per event. The control crate's `combine`
//! decides how an overlay merges (gain multiplies, the rest overwrite).
//!
//! Audio interpretation of these controls is `arbor-grove-audio` (Fase 2); here
//! they are pure data on the hap.

use crate::combinators::compose::stack;
use crate::control::ControlMap;
use crate::pattern::Pattern;
use crate::pitch::Scale;
use crate::time::Time;

/// A numeric control argument: a constant, or a pattern sampled per event.
#[derive(Clone, Debug)]
pub enum Param {
    Const(f64),
    Pat(Pattern<f64>),
}

impl From<f64> for Param {
    fn from(v: f64) -> Self {
        Param::Const(v)
    }
}

impl From<Pattern<f64>> for Param {
    fn from(p: Pattern<f64>) -> Self {
        Param::Pat(p)
    }
}

impl Param {
    /// Read the parameter at instant `t` (sampling the pattern if needed).
    fn value_at(&self, t: Time) -> Option<f64> {
        match self {
            Param::Const(v) => Some(*v),
            Param::Pat(p) => p.value_at(t),
        }
    }
}

impl Pattern<ControlMap> {
    /// Core helper: overlay a single control (computed by `set` from the
    /// per-event parameter value) onto every hap, merged via `ControlMap::combine`.
    fn with_control(
        self,
        param: Param,
        set: impl Fn(&mut ControlMap, f64) + Send + Sync + 'static,
    ) -> Pattern<ControlMap> {
        Pattern::new(move |span| {
            self.query(span)
                .into_iter()
                .map(|mut h| {
                    if let Some(v) = param.value_at(h.onset()) {
                        let mut overlay = ControlMap::default();
                        set(&mut overlay, v);
                        h.value = std::mem::take(&mut h.value).combine(overlay);
                    }
                    h
                })
                .collect()
        })
    }

    /// Amplitude (multiplicative, default `1`).
    pub fn gain(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.gain = Some(v))
    }

    /// Stereo position (`0` left … `1` right).
    pub fn pan(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.pan = Some(v))
    }

    /// Reverb send `0..1`.
    pub fn room(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.room = Some(v))
    }

    /// Low-pass cutoff (Hz).
    pub fn lpf(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.lpf = Some(v))
    }

    /// High-pass cutoff (Hz).
    pub fn hpf(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.hpf = Some(v))
    }

    /// Pitch shift in semitones (resampling).
    pub fn shift(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.shift = Some(v))
    }

    /// Playback speed factor (resampling).
    pub fn speed(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.speed = Some(v))
    }

    /// Bitcrush resolution (bits).
    pub fn crush(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.crush = Some(v))
    }

    /// Waveshaper distortion `0..1`.
    pub fn shape(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.shape = Some(v))
    }

    /// Instrument / voice name (synth preset or sampler bank).
    pub fn inst(self, name: impl Into<String>) -> Pattern<ControlMap> {
        let name = name.into();
        self.fmap(move |mut c| {
            c.inst = Some(name.clone());
            c
        })
    }

    /// Resolve numeric scale **degrees** to concrete pitches against `scale`,
    /// placing degree 0 at the root in `default_octave`. Haps that already carry
    /// a `note` are untouched.
    pub fn scale(self, scale: Scale, default_octave: i32) -> Pattern<ControlMap> {
        self.fmap(move |mut c| {
            if let Some(d) = c.degree.take() {
                c.note = Some(scale.degree_to_midi(d, default_octave));
            }
            c
        })
    }

    /// Split into stereo: the original panned left, a copy with `f` applied
    /// panned right (deterministic — no RNG).
    pub fn jux(self, f: impl FnOnce(Pattern<ControlMap>) -> Pattern<ControlMap>) -> Pattern<ControlMap> {
        let left = self.clone().pan(0.0);
        let right = f(self).pan(1.0);
        stack(vec![left, right])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinators::compose::{fastcat, pure};
    use crate::combinators::generative::rand;
    use crate::span::TimeSpan;

    fn note(midi: f64) -> Pattern<ControlMap> {
        pure(ControlMap::note(midi))
    }

    #[test]
    fn gain_multiplies_pan_overwrites() {
        let p = note(60.0).gain(0.5).gain(0.5).pan(0.2);
        let h = &p.query(TimeSpan::cycle(0))[0];
        assert_eq!(h.value.gain, Some(0.25));
        assert_eq!(h.value.pan, Some(0.2));
    }

    #[test]
    fn patternized_gain_varies_per_event() {
        let p = fastcat(vec![note(60.0), note(62.0)]).gain(rand(0.0, 1.0));
        let haps = p.query(TimeSpan::cycle(0));
        let g0 = haps[0].value.gain.unwrap();
        let g1 = haps[1].value.gain.unwrap();
        assert!((0.0..=1.0).contains(&g0) && (0.0..=1.0).contains(&g1));
        assert_ne!(g0, g1); // different onsets → different samples
    }

    #[test]
    fn scale_resolves_degrees() {
        let p = pure(ControlMap::degree(2)).scale(Scale::parse("c:minor").unwrap(), 4);
        let h = &p.query(TimeSpan::cycle(0))[0];
        assert_eq!(h.value.note, Some(63.0)); // Eb4
        assert_eq!(h.value.degree, None); // consumed
    }

    #[test]
    fn jux_pans_both_sides() {
        let p = note(60.0).jux(|q| q);
        let haps = p.query(TimeSpan::cycle(0));
        let pans: Vec<_> = haps.iter().filter_map(|h| h.value.pan).collect();
        assert!(pans.contains(&0.0));
        assert!(pans.contains(&1.0));
    }
}
