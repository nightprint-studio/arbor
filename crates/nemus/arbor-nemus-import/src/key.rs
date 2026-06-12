//! **L2 — key/scale detection**: a weighted pitch-class histogram fitted to the
//! best scale, plus the inverse map (pitch → scale degree) the emitter needs.
//!
//! The candidate set includes the non-Western modes the author works in
//! (*hirajoshi*, *in-sen*, *iwato*, *kumoi*), so a pentatonic piece is recognised
//! as such rather than being forced into the nearest Western mode. The scale
//! interval tables themselves live in `arbor-nemus-pattern` (single source).

use arbor_nemus_pattern::prelude::Scale;

use crate::model::Song;

/// Modes we try to fit, by `arbor-nemus-pattern` name.
const MODES: &[&str] = &[
    "major",
    "minor",
    "dorian",
    "phrygian",
    "lydian",
    "mixolydian",
    "locrian",
    "harmonicminor",
    "melodicminor",
    "majpent",
    "minpent",
    "hirajoshi",
    "insen",
    "iwato",
    "kumoi",
];

/// Canonical name per pitch class for building a `<root>:<mode>` spec.
const PC_NAMES: [&str; 12] = [
    "c", "cs", "d", "ef", "e", "f", "fs", "g", "af", "a", "bf", "b",
];

/// A fitted key: the scale, how well it covers the material, and enough to map
/// pitches back to degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectedKey {
    /// Root pitch class (0 = C).
    pub root_pc: i32,
    /// Mode name, accepted by [`Scale::parse`].
    pub mode: String,
    /// `"<root>:<mode>"`, e.g. `"ef:dorian"` — the argument for `.scale(...)`.
    pub spec: String,
    /// Weighted fraction of note time that falls inside the scale (`0..=1`).
    pub coverage: f64,
    /// Ascending semitone intervals of the mode (one octave).
    pub intervals: Vec<i32>,
}

/// Fit the best scale to a song's pitched material, or `None` if it has none.
pub fn detect(song: &Song) -> Option<DetectedKey> {
    let mut hist = [0.0f64; 12];
    let mut total = 0.0;
    for t in &song.tracks {
        if t.is_drum {
            continue;
        }
        for n in &t.notes {
            let pc = n.pitch.rem_euclid(12) as usize;
            let w = n.dur.to_f64().max(1e-6);
            hist[pc] += w;
            total += w;
        }
    }
    if total <= 0.0 {
        return None;
    }

    let mut best: Option<DetectedKey> = None;
    for mode in MODES {
        let intervals = match Scale::parse(&format!("c:{mode}")) {
            Ok(s) => s.intervals,
            Err(_) => continue,
        };
        for root_pc in 0..12 {
            let in_scale: f64 = (0..12)
                .filter(|pc| intervals.iter().any(|iv| (root_pc + iv).rem_euclid(12) == *pc))
                .map(|pc| hist[pc as usize])
                .sum();
            let cand = DetectedKey {
                root_pc,
                mode: (*mode).to_string(),
                spec: format!("{}:{}", PC_NAMES[root_pc as usize], mode),
                coverage: in_scale / total,
                intervals: intervals.clone(),
            };
            best = Some(match best {
                None => cand,
                Some(b) => better(b, cand),
            });
        }
    }
    best
}

/// Prefer higher coverage; on a tie prefer the *smaller* scale (a pentatonic is
/// more specific than the major scale that contains it), then keep the earlier
/// (stable mode/root order) to stay deterministic.
fn better(a: DetectedKey, b: DetectedKey) -> DetectedKey {
    const EPS: f64 = 1e-9;
    if b.coverage > a.coverage + EPS {
        b
    } else if a.coverage > b.coverage + EPS {
        a
    } else if b.intervals.len() < a.intervals.len() {
        b
    } else {
        a
    }
}

/// MIDI semitone of pitch-class `pc` at `octave` (`midi_from(0,4) = 60`).
fn midi_from(pc: i32, octave: i32) -> i32 {
    (octave + 1) * 12 + pc
}

/// The scale degree resolving to `pitch` under `key` at `ref_octave`, or `None`
/// if `pitch` is not a member of the scale. Inverse of
/// `Scale::degree_to_midi` — `ref_octave` must match the evaluator's
/// `default_octave` for the round-trip to be pitch-accurate.
pub fn degree_of(key: &DetectedKey, pitch: i32, ref_octave: i32) -> Option<i32> {
    let pc = pitch.rem_euclid(12);
    let idx = key
        .intervals
        .iter()
        .position(|iv| (key.root_pc + iv).rem_euclid(12) == pc)?;
    let base = midi_from(key.root_pc + key.intervals[idx], ref_octave);
    let diff = pitch - base;
    if diff % 12 != 0 {
        return None;
    }
    Some(idx as i32 + key.intervals.len() as i32 * (diff / 12))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Note, NoteTrack};
    use arbor_nemus_pattern::prelude::Time;

    fn song_of(pitches: &[i32]) -> Song {
        let notes = pitches
            .iter()
            .enumerate()
            .map(|(i, &p)| Note {
                start: Time::new(i as i64, 4),
                dur: Time::new(1, 4),
                pitch: p,
                vel: 0.8,
            })
            .collect();
        Song {
            tracks: vec![NoteTrack {
                name: "t".into(),
                is_drum: false,
                notes,
            }],
            cps: 0.5,
            len_cycles: 1,
        }
    }

    #[test]
    fn full_coverage_for_in_scale_material() {
        // C major pentatonic: C D E G A — covered exactly by majpent on C.
        let key = detect(&song_of(&[60, 62, 64, 67, 69])).unwrap();
        assert!((key.coverage - 1.0).abs() < 1e-9);
        assert_eq!(key.root_pc, 0);
        // The smaller (pentatonic) scale wins the tie against major.
        assert_eq!(key.intervals.len(), 5);
    }

    #[test]
    fn degree_round_trips_through_pattern_scale() {
        let key = detect(&song_of(&[60, 62, 64, 67, 69])).unwrap();
        let scale = Scale::parse(&key.spec).unwrap();
        for &p in &[60, 62, 64, 67, 69, 72] {
            let d = degree_of(&key, p, 4).expect("in scale");
            assert_eq!(scale.degree_to_midi(d, 4) as i32, p);
        }
    }

    #[test]
    fn out_of_scale_pitch_has_no_degree() {
        let key = detect(&song_of(&[60, 62, 64, 67, 69])).unwrap();
        assert_eq!(degree_of(&key, 61, 4), None); // C# not in C pentatonic
    }
}
