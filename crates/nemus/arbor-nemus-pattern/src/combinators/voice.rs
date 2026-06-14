//! Voice & mix transforms: they stamp controls onto each event. These are
//! specialised to `Pattern<ControlMap>` (the others stay generic).
//!
//! Numeric parameters accept a constant **or** a pattern of numbers (a [`Param`])
//! — `.gain(rand(0.0, 1.0))` varies per event. The control crate's `combine`
//! decides how an overlay merges (gain multiplies, the rest overwrite).
//!
//! Audio interpretation of these controls is `arbor-nemus-audio` (Fase 2); here
//! they are pure data on the hap.

use crate::combinators::compose::stack;
use crate::control::{CompSpec, ControlMap, EqBandSpec, HoldSpec};
use crate::pattern::Pattern;
use crate::pitch::Scale;
use crate::rng::{time_to_rand, SEED_HUMANIZE_GAIN, SEED_HUMANIZE_TIME};
use crate::span::TimeSpan;
use crate::time::Time;

/// Resolution of the quantised timing jitter: a random instant is snapped to one
/// of `±HUMANIZE_RES` exact sub-steps of the amount, so the shift stays an exact
/// rational (no `f64` drift in the time pipeline) while still feeling continuous.
const HUMANIZE_RES: i64 = 4096;

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

    /// Velocity `0..1`: selects the sampled velocity-layer + dynamics (≠ `gain`).
    pub fn vel(self, x: impl Into<Param>) -> Pattern<ControlMap> {
        self.with_control(x.into(), |c, v| c.vel = Some(v))
    }

    /// Instrument / voice name (synth preset or sampler bank).
    pub fn inst(self, name: impl Into<String>) -> Pattern<ControlMap> {
        let name = name.into();
        self.fmap(move |mut c| {
            c.inst = Some(name.clone());
            c
        })
    }

    /// Articulation name (`legato`/`staccato`/…), resolved by the instrument.
    /// Constant like `inst` (not patternised); per-note sequences await the
    /// value-island.
    pub fn art(self, name: impl Into<String>) -> Pattern<ControlMap> {
        let name = name.into();
        self.fmap(move |mut c| {
            c.art = Some(name.clone());
            c
        })
    }

    /// Append one band to the track's parametric-EQ strip insert (`.eq(...)`,
    /// chainable — each call adds a band). Strip-level (not per-voice): the audio
    /// engine derives one EQ chain per track from these. Constant like `art`.
    pub fn add_eq(self, band: EqBandSpec) -> Pattern<ControlMap> {
        self.fmap(move |mut c| {
            c.eq.get_or_insert_with(Vec::new).push(band);
            c
        })
    }

    /// Set the track's compressor strip insert (`.comp(...)`). Strip-level; the
    /// audio engine derives one compressor per track from this. Constant like `art`.
    pub fn comp(self, spec: CompSpec) -> Pattern<ControlMap> {
        self.fmap(move |mut c| {
            c.comp = Some(spec);
            c
        })
    }

    /// Hold / sustain voicing: play the pattern as a monophonic **held** line
    /// (drone / pad) — one voice per track, re-pitched by the next note with no
    /// re-attack (like `legato`), with the per-slot release replaced by `spec`.
    /// Constant like `art` (not patternised).
    pub fn hold(self, spec: HoldSpec) -> Pattern<ControlMap> {
        self.fmap(move |mut c| {
            c.hold = Some(spec);
            c
        })
    }

    /// Transpose by adding `semitones` to every event's `note` (`add`). Events
    /// without a concrete `note` are left untouched — transposition acts on
    /// resolved pitches, not unresolved degrees.
    pub fn add_note(self, semitones: f64) -> Pattern<ControlMap> {
        self.fmap(move |mut c| {
            if let Some(n) = c.note {
                c.note = Some(n + semitones);
            }
            c
        })
    }

    /// Transpose by adding `steps` to every event's scale `degree` (`addDeg`),
    /// before `scale` resolves it. Events without a `degree` are left untouched.
    pub fn add_degree(self, steps: i32) -> Pattern<ControlMap> {
        self.fmap(move |mut c| {
            if let Some(d) = c.degree {
                c.degree = Some(d + steps);
            }
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

    /// "Humanize" the part: nudge each onset by a small random amount of time and
    /// wobble its gain, so a quantised line breathes like a played one. Both
    /// jitters are **seeded by the event's onset** (independent streams), so the
    /// feel is identical every loop and a re-eval never disturbs fixed cycles.
    ///
    /// - `time_amt` — max timing shift in **cycles**, symmetric `±time_amt`
    ///   (0 = no timing jitter).
    /// - `vel_amt` — gain wobble depth, a multiplier in `[1 - vel_amt, 1 + vel_amt]`
    ///   (0 = no gain jitter); it multiplies the existing gain via `combine`.
    ///
    /// The query window is widened by `time_amt` so an onset nudged *into* `span`
    /// from just outside is not lost, and fragments nudged out are re-clipped — a
    /// neighbouring-block query catches them at their real position.
    pub fn humanize(self, time_amt: Time, vel_amt: f64) -> Pattern<ControlMap> {
        let time_amt = time_amt.max(Time::ZERO);
        let vel_amt = vel_amt.max(0.0);
        if time_amt == Time::ZERO && vel_amt == 0.0 {
            return self;
        }
        Pattern::new(move |span: TimeSpan| {
            let widened = TimeSpan::new(span.begin - time_amt, span.end + time_amt);
            let mut out = Vec::new();
            for h in self.query(widened) {
                let onset = h.onset();
                let dt = jitter_time(onset, time_amt);
                let shifted = h.map_time(|t| t + dt);
                // Keep only what still overlaps the *original* window, re-clipping
                // `part` so onset detection downstream stays correct.
                if let Some(part) = shifted.part.sect(span) {
                    let mut hh = shifted;
                    hh.part = part;
                    if vel_amt > 0.0 {
                        let mut overlay = ControlMap::default();
                        overlay.gain = Some(jitter_gain(onset, vel_amt));
                        hh.value = std::mem::take(&mut hh.value).combine(overlay);
                    }
                    out.push(hh);
                }
            }
            out
        })
    }
}

/// Symmetric timing jitter in `±amt`, quantised to [`HUMANIZE_RES`] exact
/// sub-steps and seeded by the onset (so it is stable per instant).
fn jitter_time(onset: Time, amt: Time) -> Time {
    if amt == Time::ZERO {
        return Time::ZERO;
    }
    let r = time_to_rand(onset, SEED_HUMANIZE_TIME); // [0, 1)
    let k = (((2.0 * r) - 1.0) * HUMANIZE_RES as f64).round() as i64; // [-RES, RES]
    amt * Time::new(k, HUMANIZE_RES)
}

/// Gain wobble factor in `[1 - amt, 1 + amt]` (clamped ≥ 0), seeded by the onset.
fn jitter_gain(onset: Time, amt: f64) -> f64 {
    let r = time_to_rand(onset, SEED_HUMANIZE_GAIN); // [0, 1)
    (1.0 + amt * (2.0 * r - 1.0)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combinators::compose::{fastcat, pure, silence};
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
    fn add_note_transposes_resolved_pitches() {
        let p = note(60.0).add_note(7.0);
        assert_eq!(p.query(TimeSpan::cycle(0))[0].value.note, Some(67.0));
        // a degree-only event is untouched (no resolved note yet).
        let d = pure(ControlMap::degree(2)).add_note(7.0);
        let v = &d.query(TimeSpan::cycle(0))[0].value;
        assert_eq!(v.note, None);
        assert_eq!(v.degree, Some(2));
    }

    #[test]
    fn add_degree_shifts_then_scale_resolves() {
        let p = pure(ControlMap::degree(0))
            .add_degree(2)
            .scale(Scale::parse("c:minor").unwrap(), 4);
        // degree 0 + 2 → Eb4 = 63.
        assert_eq!(p.query(TimeSpan::cycle(0))[0].value.note, Some(63.0));
    }

    #[test]
    fn jux_pans_both_sides() {
        let p = note(60.0).jux(|q| q);
        let haps = p.query(TimeSpan::cycle(0));
        let pans: Vec<_> = haps.iter().filter_map(|h| h.value.pan).collect();
        assert!(pans.contains(&0.0));
        assert!(pans.contains(&1.0));
    }

    #[test]
    fn humanize_timing_shifts_onsets_within_bounds() {
        let amt = Time::new(1, 16);
        // Slot 0 is a rest so no onset sits on the cycle boundary (which could
        // legitimately drift into the adjacent cycle and skew the count).
        let p = fastcat(vec![silence(), note(60.0), note(62.0), note(64.0)]).humanize(amt, 0.0);
        let a = p.query(TimeSpan::cycle(0));
        assert_eq!(a, p.query(TimeSpan::cycle(0))); // deterministic every loop
        let onsets: Vec<_> = a.iter().filter(|h| h.has_onset()).collect();
        assert_eq!(onsets.len(), 3); // interior onsets — none lost, none doubled
        let grid = [Time::new(1, 4), Time::new(1, 2), Time::new(3, 4)];
        for h in &onsets {
            let b = h.whole.unwrap().begin;
            let near = grid.iter().any(|g| {
                let d = b - *g;
                d <= amt && d >= -amt
            });
            assert!(near, "onset {b:?} not within ±{amt:?} of a slot");
        }
        // The jitter is real, not a silent no-op (deterministic, but non-trivial).
        assert!((0..8).any(|i| jitter_time(Time::new(i, 8), amt) != Time::ZERO));
    }

    #[test]
    fn humanize_wobbles_gain_in_range_and_is_deterministic() {
        let p = fastcat(vec![note(60.0), note(62.0)]).humanize(Time::ZERO, 0.2);
        let a = p.query(TimeSpan::cycle(0));
        let b = p.query(TimeSpan::cycle(0));
        assert_eq!(a, b); // identical every loop (seeded per onset)
        for h in &a {
            let g = h.value.gain.unwrap();
            assert!((0.8..=1.2).contains(&g), "gain {g} out of ±0.2");
        }
        // Different onsets → different wobble.
        assert_ne!(a[0].value.gain, a[1].value.gain);
    }

    #[test]
    fn humanize_zero_is_identity() {
        let base = fastcat(vec![note(60.0), note(62.0)]);
        let h = base.clone().humanize(Time::ZERO, 0.0);
        assert_eq!(base.query(TimeSpan::cycle(0)), h.query(TimeSpan::cycle(0)));
    }
}
