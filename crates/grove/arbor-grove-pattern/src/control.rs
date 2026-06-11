//! `ControlMap` — grove's concrete hap value.
//!
//! Strudel uses an open string-keyed control map because its users extend it.
//! grove's stdlib is **closed**, so the controls are enumerable and we use a
//! **typed struct**: no key typos, field-wise merge, zero-cost. The "what"
//! (sound / note / degree) lives in the same struct as the controls
//! (gain / pan / …), flat like Strudel, so structural combinators never touch
//! it and merging is uniform.
//!
//! Sources from files (`sample`/`audio`) are only **markers** here — the actual
//! decode/playback is `arbor-grove-audio` (Fase 2). The marker does carry the
//! [`SourceKind`] so the audio engine knows whether to play it as a one-shot or
//! a sustained stem — the only thing distinguishing `sample` from `audio`.

/// How a file source ([`ControlMap::source_file`]) should be played back.
///
/// The pattern layer can't act on this — both kinds place the same path marker
/// once per cycle — but it travels on the [`ControlMap`] so the audio engine
/// (Fase 2) can realise the distinction: a one-shot retriggers per onset, a
/// sustained stem starts once and rings through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// A short hit / chop (`sample(...)`): (re)triggered at each onset.
    OneShot,
    /// A long stem / take / ambience (`audio(...)`): played once, sustained.
    Sustained,
}

/// A typed bag of controls describing a single event.
///
/// Every field is `Option` — unset means "inherit / engine default". Build with
/// the constructors and the fluent setters, or merge two maps with
/// [`combine`](ControlMap::combine).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlMap {
    // ── The "what" ──────────────────────────────────────────────────────────
    /// Sample / sound name (the leaf of an `s(...)` island), e.g. `"bd"`.
    pub sound: Option<String>,
    /// Sample variant index (`:n`), only meaningful with `sound`.
    pub variant: Option<u32>,
    /// Resolved pitch as a MIDI-style semitone (`C4 = 60`); `f64` for microtonal.
    pub note: Option<f64>,
    /// Unresolved scale degree — turned into `note` by `scale()`.
    pub degree: Option<i32>,
    /// A file path marker for an imported source (`sample`/`audio`).
    pub source_file: Option<String>,
    /// Playback kind of `source_file` — one-shot vs sustained. Only meaningful
    /// alongside `source_file`; realised by the audio engine.
    pub source_kind: Option<SourceKind>,

    // ── Controls ────────────────────────────────────────────────────────────
    /// Amplitude, multiplicative (default `1`).
    pub gain: Option<f64>,
    /// Stereo position: `0` left, `1` right, `0.5` centre.
    pub pan: Option<f64>,
    /// Reverb send amount `0..1`.
    pub room: Option<f64>,
    /// Low-pass cutoff in Hz.
    pub lpf: Option<f64>,
    /// High-pass cutoff in Hz.
    pub hpf: Option<f64>,
    /// Pitch shift in semitones (resampling).
    pub shift: Option<f64>,
    /// Playback speed factor (resampling; couples pitch + duration).
    pub speed: Option<f64>,
    /// Bitcrush resolution in bits.
    pub crush: Option<f64>,
    /// Waveshaper distortion amount `0..1`.
    pub shape: Option<f64>,
    /// Velocity `0..1`: selects the sampled velocity-layer (timbre) + dynamics.
    /// Distinct from `gain` (output amplitude) — set per the sampled layer.
    pub vel: Option<f64>,
    /// Instrument / voice name (synth preset or sampler bank).
    pub inst: Option<String>,
    /// Articulation name (`legato`/`staccato`/…), resolved by the instrument.
    pub art: Option<String>,
}

impl ControlMap {
    /// A sound leaf (`s("bd")`).
    pub fn sound(name: impl Into<String>) -> Self {
        ControlMap {
            sound: Some(name.into()),
            ..Default::default()
        }
    }

    /// A concrete pitch (MIDI semitone).
    pub fn note(midi: f64) -> Self {
        ControlMap {
            note: Some(midi),
            ..Default::default()
        }
    }

    /// An unresolved scale degree (needs `scale()`).
    pub fn degree(d: i32) -> Self {
        ControlMap {
            degree: Some(d),
            ..Default::default()
        }
    }

    /// A file-source marker (`sample`/`audio`); decoded in the audio crate.
    pub fn source_file(path: impl Into<String>) -> Self {
        ControlMap {
            source_file: Some(path.into()),
            ..Default::default()
        }
    }

    /// Merge `other` onto `self`, with `other` taking precedence.
    ///
    /// `gain` is the exception: it **multiplies** (per the design — gains
    /// compound), defaulting a missing side to `1`. Every other field is
    /// "right wins if set, else keep left".
    pub fn combine(self, other: ControlMap) -> ControlMap {
        ControlMap {
            sound: other.sound.or(self.sound),
            variant: other.variant.or(self.variant),
            note: other.note.or(self.note),
            degree: other.degree.or(self.degree),
            source_file: other.source_file.or(self.source_file),
            source_kind: other.source_kind.or(self.source_kind),
            gain: combine_gain(self.gain, other.gain),
            pan: other.pan.or(self.pan),
            room: other.room.or(self.room),
            lpf: other.lpf.or(self.lpf),
            hpf: other.hpf.or(self.hpf),
            shift: other.shift.or(self.shift),
            speed: other.speed.or(self.speed),
            crush: other.crush.or(self.crush),
            shape: other.shape.or(self.shape),
            vel: other.vel.or(self.vel),
            inst: other.inst.or(self.inst),
            art: other.art.or(self.art),
        }
    }
}

/// Multiplicative gain merge: `None`/`None` stays `None`, otherwise the present
/// sides multiply with a missing side treated as unity.
fn combine_gain(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (None, None) => None,
        (x, y) => Some(x.unwrap_or(1.0) * y.unwrap_or(1.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_right_wins_and_gain_multiplies() {
        let base = ControlMap::sound("bd");
        let mut overlay = ControlMap::default();
        overlay.pan = Some(0.2);
        let merged = base.clone().combine(overlay);
        assert_eq!(merged.sound.as_deref(), Some("bd"));
        assert_eq!(merged.pan, Some(0.2));

        let mut g1 = ControlMap::default();
        g1.gain = Some(0.5);
        let mut g2 = ControlMap::default();
        g2.gain = Some(0.5);
        assert_eq!(g1.combine(g2).gain, Some(0.25));
    }

    #[test]
    fn gain_defaults_missing_side_to_unity() {
        let mut only = ControlMap::default();
        only.gain = Some(0.4);
        assert_eq!(ControlMap::default().combine(only).gain, Some(0.4));
    }
}
