//! `ControlMap` — nemus's concrete hap value.
//!
//! Strudel uses an open string-keyed control map because its users extend it.
//! nemus's stdlib is **closed**, so the controls are enumerable and we use a
//! **typed struct**: no key typos, field-wise merge, zero-cost. The "what"
//! (sound / note / degree) lives in the same struct as the controls
//! (gain / pan / …), flat like Strudel, so structural combinators never touch
//! it and merging is uniform.
//!
//! Sources from files (`sample`/`audio`) are only **markers** here — the actual
//! decode/playback is `arbor-nemus-audio` (Fase 2). The marker does carry the
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

/// How long a `.hold(...)` note sustains before releasing — the monophonic,
/// connected "drone / pad" voicing.
///
/// A held note reuses the legato machinery (one voice per track, re-pitched by
/// the next note with no envelope re-attack) **and** suppresses the per-slot
/// note-off: a plain note releases when its slot ends, a held note's release is
/// driven by this policy instead. The pattern layer can't act on it — it travels
/// on the [`ControlMap`] for the audio engine to realise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HoldSpec {
    /// Ring until the next note on the track re-pitches it (or transport stop) —
    /// the continuous pad / drone. No self-release; a rest does not break it.
    Drone,
    /// Release after this many **cycles** (beats), regardless of the slot length.
    Cycles(f64),
    /// Release after this many absolute **seconds** (converted via the clock).
    Seconds(f64),
}

/// One parametric-EQ band on a track's strip insert, authored with
/// `.eq(kind, freq, gainDb, q?)`. Pure data: the audio engine maps it onto its own
/// biquad band (the pattern crate stays audio-free). Like `delay`, it travels
/// per-event but the engine realises it as a **per-track** strip insert.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqBandSpec {
    /// Band shape.
    pub kind: EqShape,
    /// Centre / corner frequency in Hz.
    pub freq: f64,
    /// Gain in dB (peak / shelf bands only; ignored for hpf/lpf).
    pub gain_db: f64,
    /// Quality factor (bandwidth for peak, slope for shelf, resonance for hpf/lpf).
    pub q: f64,
}

/// The shape of an [`EqBandSpec`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqShape {
    /// Bell / peaking boost-cut around `freq`.
    Peak,
    /// Low shelf below `freq`.
    LowShelf,
    /// High shelf above `freq`.
    HighShelf,
    /// High-pass (rumble removal); `gain_db` ignored.
    Hpf,
    /// Low-pass (top-end taming); `gain_db` ignored.
    Lpf,
}

/// Per-track compressor settings, authored with
/// `.comp(thresholdDb, ratio, attack?, release?, makeup?, knee?)`. Pure data
/// realised by the audio engine as a strip insert.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompSpec {
    /// Threshold in dBFS below which no reduction is applied.
    pub threshold_db: f64,
    /// Compression ratio (e.g. `4.0` = 4:1).
    pub ratio: f64,
    /// Attack time in seconds.
    pub attack: f64,
    /// Release time in seconds.
    pub release: f64,
    /// Make-up gain in dB applied after compression.
    pub makeup_db: f64,
    /// Soft-knee width in dB (0 = hard knee).
    pub knee_db: f64,
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

    // ── Delay (feedback echo) ─────────────────────────────────────────────────
    // A real feedback echo, distinct from `off` (which retriggers the *pattern*).
    // The three controls travel per-event but the audio engine realises them as a
    // **per-track delay bus** (Fase 5): `delay_mix` is the send into the bus,
    // while `delay`/`feedback` configure that bus's line — so the echoes ring on
    // independently of the source voice's lifetime. All override on `combine`.
    /// Delay time in **fractions of a cycle** (e.g. `0.25` = a quarter-cycle).
    pub delay: Option<f64>,
    /// Delay feedback `0..1` — how much of the echo feeds back into the line.
    pub feedback: Option<f64>,
    /// Delay send / wet mix `0..1` — how much of this event feeds the delay bus.
    pub delay_mix: Option<f64>,

    /// Velocity `0..1`: selects the sampled velocity-layer (timbre) + dynamics.
    /// Distinct from `gain` (output amplitude) — set per the sampled layer.
    pub vel: Option<f64>,
    /// Instrument / voice name (synth preset or sampler bank).
    pub inst: Option<String>,
    /// Articulation name (`legato`/`staccato`/…), resolved by the instrument.
    pub art: Option<String>,
    /// Sustain / "hold" voicing — a monophonic held note (drone / pad). `Some`
    /// connects the note mono per track (like `legato`) and replaces the per-slot
    /// release with the [`HoldSpec`] policy. Realised by the audio engine.
    pub hold: Option<HoldSpec>,

    // ── Strip inserts (per-track FX) ─────────────────────────────────────────
    // EQ / compressor are strip-level (not per-voice). They travel per-event but
    // the audio engine derives one config per track from them — like `delay`'s bus.
    /// Per-track parametric-EQ bands (`.eq(...)`, chainable — each call appends a
    /// band). Override on `combine`.
    pub eq: Option<Vec<EqBandSpec>>,
    /// Per-track compressor (`.comp(...)`). Override on `combine`.
    pub comp: Option<CompSpec>,
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
            delay: other.delay.or(self.delay),
            feedback: other.feedback.or(self.feedback),
            delay_mix: other.delay_mix.or(self.delay_mix),
            vel: other.vel.or(self.vel),
            inst: other.inst.or(self.inst),
            art: other.art.or(self.art),
            hold: other.hold.or(self.hold),
            eq: other.eq.or(self.eq),
            comp: other.comp.or(self.comp),
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

    #[test]
    fn delay_fields_override_and_carry_through() {
        // Right wins when set; left kept when right is unset (no multiply).
        let mut base = ControlMap::default();
        base.delay = Some(0.25);
        base.feedback = Some(0.3);
        base.delay_mix = Some(0.5);
        let mut overlay = ControlMap::default();
        overlay.feedback = Some(0.6); // only feedback overridden
        let merged = base.combine(overlay);
        assert_eq!(merged.delay, Some(0.25));
        assert_eq!(merged.feedback, Some(0.6));
        assert_eq!(merged.delay_mix, Some(0.5));
    }
}
