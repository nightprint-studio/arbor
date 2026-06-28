//! Minimal music-theory model: note names, scales, degree resolution.
//!
//! `scale()` (in [`crate::combinators::voice`]) interprets numeric **degree**
//! leaves against a scale — pure arithmetic over a static interval table, no
//! external crates, so it belongs in this dependency-free core rather than the
//! language layer. Pitches are MIDI-style semitone numbers (`C4 = 60`), carried
//! as `f64` so later pitch shifting can be microtonal.

use crate::error::{PatternError, Result};

/// MIDI semitone of middle C (C4).
pub const MIDDLE_C: f64 = 60.0;

/// Pitch class (0..12) of a natural note letter.
fn letter_pitch_class(letter: char) -> Option<i32> {
    Some(match letter {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    })
}

/// Parse a note name like `c`, `cs4`, `ef3`, `bf2` into a MIDI semitone.
///
/// Grammar (mini-notation `NOTE_NAME`): `[a-g] (s|f)? [octave]?`. `s` = sharp,
/// `f` = flat (so `bf` is B-flat, never ambiguous with the note B). A missing
/// octave uses `default_octave`.
pub fn parse_note(name: &str, default_octave: i32) -> Result<f64> {
    let mut chars = name.chars();
    let letter = chars
        .next()
        .and_then(|c| letter_pitch_class(c.to_ascii_lowercase()))
        .ok_or_else(|| PatternError::UnknownNote(name.to_string()))?;

    let rest: String = chars.collect();
    let (accidental, digits) = match rest.chars().next() {
        Some('s') => (1, &rest[1..]),
        Some('f') => (-1, &rest[1..]),
        _ => (0, rest.as_str()),
    };

    let octave = if digits.is_empty() {
        default_octave
    } else {
        digits
            .parse::<i32>()
            .map_err(|_| PatternError::UnknownNote(name.to_string()))?
    };

    Ok(midi_from(letter + accidental, octave))
}

/// MIDI semitone of `pitch_class` at `octave` (`C4 = 60`).
fn midi_from(pitch_class: i32, octave: i32) -> f64 {
    ((octave + 1) * 12 + pitch_class) as f64
}

/// A named mode: its canonical name, accepted aliases, and one-octave ascending
/// semitone intervals. The single source of truth for both [`Scale::parse`] and
/// the catalogue tooling consumes ([`mode_table`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleMode {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub intervals: &'static [i32],
}

/// Every mode `Scale::parse` accepts. Adding a mode here surfaces it everywhere
/// (parser + the editor's scale catalogue) at once.
const MODES: &[ScaleMode] = &[
    ScaleMode { name: "major",         aliases: &["ionian"],                 intervals: &[0, 2, 4, 5, 7, 9, 11] },
    ScaleMode { name: "minor",         aliases: &["aeolian"],                intervals: &[0, 2, 3, 5, 7, 8, 10] },
    ScaleMode { name: "dorian",        aliases: &[],                         intervals: &[0, 2, 3, 5, 7, 9, 10] },
    ScaleMode { name: "phrygian",      aliases: &[],                         intervals: &[0, 1, 3, 5, 7, 8, 10] },
    ScaleMode { name: "lydian",        aliases: &[],                         intervals: &[0, 2, 4, 6, 7, 9, 11] },
    ScaleMode { name: "mixolydian",    aliases: &[],                         intervals: &[0, 2, 4, 5, 7, 9, 10] },
    ScaleMode { name: "locrian",       aliases: &[],                         intervals: &[0, 1, 3, 5, 6, 8, 10] },
    ScaleMode { name: "harmonicminor", aliases: &["harmonic_minor"],         intervals: &[0, 2, 3, 5, 7, 8, 11] },
    ScaleMode { name: "melodicminor",  aliases: &["melodic_minor"],          intervals: &[0, 2, 3, 5, 7, 9, 11] },
    ScaleMode { name: "majpent",       aliases: &["majorpentatonic"],        intervals: &[0, 2, 4, 7, 9] },
    ScaleMode { name: "minpent",       aliases: &["minorpentatonic"],        intervals: &[0, 3, 5, 7, 10] },
    // Japanese pentatonic / hexatonic modes (used in the author's pieces).
    ScaleMode { name: "hirajoshi",     aliases: &[],                         intervals: &[0, 2, 3, 7, 8] },
    ScaleMode { name: "insen",         aliases: &["in_sen", "in-sen"],       intervals: &[0, 1, 5, 7, 10] },
    ScaleMode { name: "iwato",         aliases: &[],                         intervals: &[0, 1, 5, 6, 10] },
    ScaleMode { name: "kumoi",         aliases: &[],                         intervals: &[0, 2, 3, 7, 9] },
    ScaleMode { name: "chromatic",     aliases: &[],                         intervals: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11] },
];

/// The full mode catalogue (canonical entries) — for the editor's scale-aware
/// tooling (snap-to-scale, change-scale). Aliases live on each entry.
pub fn mode_table() -> &'static [ScaleMode] {
    MODES
}

/// A scale: a root pitch class plus ascending semitone intervals (one octave).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scale {
    pub root_pc: i32,
    pub intervals: Vec<i32>,
}

impl Scale {
    /// Look up a named mode's interval set (by canonical name or any alias).
    fn mode_intervals(mode: &str) -> Result<Vec<i32>> {
        MODES
            .iter()
            .find(|m| m.name == mode || m.aliases.contains(&mode))
            .map(|m| m.intervals.to_vec())
            .ok_or_else(|| PatternError::UnknownScale(mode.to_string()))
    }

    /// Parse a `"<root>:<mode>"` spec, e.g. `"c:minor"`, `"ef:dorian"`.
    pub fn parse(spec: &str) -> Result<Scale> {
        let (root, mode) = spec
            .split_once(':')
            .ok_or_else(|| PatternError::BadScaleSpec(spec.to_string()))?;
        // Root: reuse note parsing at octave 0, take its pitch class.
        let root_midi = parse_note(root.trim(), 0)? as i32;
        let root_pc = root_midi.rem_euclid(12);
        Ok(Scale {
            root_pc,
            intervals: Scale::mode_intervals(mode.trim())?,
        })
    }

    /// Resolve a (possibly out-of-range, possibly negative) scale degree to a
    /// MIDI semitone, with `default_octave` placing degree 0 at the root.
    pub fn degree_to_midi(&self, degree: i32, default_octave: i32) -> f64 {
        let len = self.intervals.len() as i32;
        let octave_shift = degree.div_euclid(len);
        let idx = degree.rem_euclid(len) as usize;
        midi_from(self.root_pc + self.intervals[idx], default_octave) + (12 * octave_shift) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_notes() {
        assert_eq!(parse_note("c", 4).unwrap(), 60.0);
        assert_eq!(parse_note("c4", 4).unwrap(), 60.0);
        assert_eq!(parse_note("cs4", 4).unwrap(), 61.0);
        assert_eq!(parse_note("ef4", 4).unwrap(), 63.0); // E-flat = D#
        assert_eq!(parse_note("bf3", 4).unwrap(), 58.0); // B-flat below middle C
        assert_eq!(parse_note("a4", 4).unwrap(), 69.0); // A440
    }

    #[test]
    fn rejects_bad_notes() {
        assert!(parse_note("h4", 4).is_err());
        assert!(parse_note("cx", 4).is_err());
    }

    #[test]
    fn resolves_degrees() {
        let s = Scale::parse("c:minor").unwrap();
        assert_eq!(s.degree_to_midi(0, 4), 60.0); // C4
        assert_eq!(s.degree_to_midi(2, 4), 63.0); // Eb4
        assert_eq!(s.degree_to_midi(7, 4), 72.0); // C5 (octave up)
        assert_eq!(s.degree_to_midi(-1, 4), 58.0); // Bb3 (wraps down)
    }

    #[test]
    fn scale_spec_errors() {
        assert!(Scale::parse("c-minor").is_err());
        assert!(Scale::parse("c:bogus").is_err());
    }

    #[test]
    fn resolves_japanese_scales() {
        // Hirajoshi on D: D E F A Bb (semitones 0 2 3 7 8 from root).
        let hira = Scale::parse("d:hirajoshi").unwrap();
        assert_eq!(hira.degree_to_midi(0, 4), 62.0); // D4
        assert_eq!(hira.degree_to_midi(1, 4), 64.0); // E4
        assert_eq!(hira.degree_to_midi(2, 4), 65.0); // F4
        assert_eq!(hira.degree_to_midi(5, 4), 74.0); // D5 (wraps an octave)

        // In-sen aliases all resolve to the same intervals.
        assert_eq!(
            Scale::parse("c:insen").unwrap(),
            Scale::parse("c:in-sen").unwrap()
        );
        assert_eq!(
            Scale::parse("c:insen").unwrap(),
            Scale::parse("c:in_sen").unwrap()
        );
    }
}
