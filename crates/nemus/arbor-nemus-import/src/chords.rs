//! **L2 — chord grouping**: a set of simultaneous pitches → a chord symbol.
//!
//! The inverse of the language's `'name` postfix. We read the interval templates
//! from `arbor-nemus-lang`'s own chord table (the single source of truth, so the
//! symbols we emit are exactly the ones the evaluator accepts) and match a
//! voicing's pitch-class set against them.
//!
//! Only triads and sevenths are recognised. Extended chords (9/11/13) collapse
//! to the same pitch-class set as their parent seventh once reduced mod-12, so
//! recognising them reliably from a voicing is ambiguous; those are left to be
//! emitted as parallel note lanes (`a & b & c`) instead — still correct, just
//! not folded into a symbol.

use arbor_nemus_lang::prelude::chord_intervals;

/// Names tried, richest first so a seventh wins over the triad it contains.
const CANDIDATES: &[&str] = &[
    "maj7", "min7", "7", "dim7", "m7b5", "minMaj7", "aug7", "6", "min6", "maj", "min", "dim",
    "aug", "sus2", "sus4", "5",
];

/// A recognised chord: its root (an actual sounding MIDI pitch) and a symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    /// Root as a sounding MIDI pitch (root position → the bass note).
    pub root: i32,
    /// Symbol from the `'name` vocabulary (`"maj7"`, `"min"`, …).
    pub name: &'static str,
}

/// Recognise a chord from simultaneous absolute-MIDI `pitches`, or `None`.
pub fn recognize(pitches: &[i32]) -> Option<Chord> {
    let mut pcs: Vec<i32> = pitches.iter().map(|p| p.rem_euclid(12)).collect();
    pcs.sort_unstable();
    pcs.dedup();
    if pcs.len() < 2 {
        return None;
    }
    let bass = *pitches.iter().min().unwrap();

    // Try the bass's pitch class as the root first (root position), then the
    // others — the first template match wins.
    let mut roots = vec![bass.rem_euclid(12)];
    for &pc in &pcs {
        if !roots.contains(&pc) {
            roots.push(pc);
        }
    }

    for root_pc in roots {
        let mut rel: Vec<i32> = pcs.iter().map(|pc| (pc - root_pc).rem_euclid(12)).collect();
        rel.sort_unstable();
        rel.dedup();
        for &name in CANDIDATES {
            let Some(tmpl) = chord_intervals(name) else {
                continue;
            };
            let mut t: Vec<i32> = tmpl.iter().map(|iv| iv.rem_euclid(12)).collect();
            t.sort_unstable();
            t.dedup();
            if t == rel {
                let root = pitches
                    .iter()
                    .copied()
                    .filter(|p| p.rem_euclid(12) == root_pc)
                    .min()
                    .unwrap_or(bass);
                return Some(Chord { root, name });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_basic_triads() {
        assert_eq!(recognize(&[60, 64, 67]).unwrap().name, "maj");
        assert_eq!(recognize(&[60, 63, 67]).unwrap().name, "min");
        assert_eq!(recognize(&[60, 63, 66]).unwrap().name, "dim");
        let power = recognize(&[60, 67]).unwrap();
        assert_eq!(power.name, "5");
        assert_eq!(power.root, 60);
    }

    #[test]
    fn recognises_sevenths_over_triads() {
        assert_eq!(recognize(&[60, 64, 67, 71]).unwrap().name, "maj7");
        assert_eq!(recognize(&[60, 63, 67, 70]).unwrap().name, "min7");
        assert_eq!(recognize(&[60, 64, 67, 70]).unwrap().name, "7");
    }

    #[test]
    fn root_position_picks_the_bass() {
        // First inversion of C major (E G C) still reads as a C major chord.
        let c = recognize(&[64, 67, 72]).unwrap();
        assert_eq!(c.name, "maj");
        assert_eq!(c.root, 72); // the only sounding C
    }

    #[test]
    fn rejects_non_chords() {
        assert_eq!(recognize(&[60]), None);
        assert_eq!(recognize(&[60, 61, 62]), None); // cluster
    }
}
